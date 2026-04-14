/// Translates Forge bytecode to Cranelift IR.
/// Type-aware: uses I64 for integers, F64 for floats, I8 (0/1) for bools.
/// Functions with string/collection ops use I64 registers and call runtime bridges.
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, StackSlotData, StackSlotKind, UserFuncName};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::Module;

use crate::vm::bytecode::*;
use crate::vm::jit::runtime::{TAG_BOOL, TAG_INT, TAG_OBJ, TAG_SHIFT};
use crate::vm::jit::type_analysis::{self, RegType};

/// Pre-allocated GcRef indices for string constants in the chunk.
/// `string_refs[i]` is `Some(gcref_index)` if `chunk.constants[i]` is a Str,
/// `None` otherwise.  Only needed when `type_info.has_string_ops` is true.
pub type StringRefs = Vec<Option<i64>>;

/// Emit Cranelift IR to tag-encode a raw i64 value based on its known RegType.
/// Used when passing values to collection bridges that expect tagged encoding.
fn emit_tag_encode(
    b: &mut FunctionBuilder,
    val: cranelift_codegen::ir::Value,
    reg_type: RegType,
) -> cranelift_codegen::ir::Value {
    match reg_type {
        RegType::Int => {
            let tag = b.ins().iconst(I64, (TAG_INT << TAG_SHIFT) as i64);
            let masked = b.ins().band_imm(val, 0x0FFF_FFFF_FFFF_FFFF_i64);
            b.ins().bor(tag, masked)
        }
        RegType::Bool => {
            let tag = b.ins().iconst(I64, (TAG_BOOL << TAG_SHIFT) as i64);
            b.ins().bor(tag, val)
        }
        RegType::StringRef | RegType::ObjRef => {
            let tag = b.ins().iconst(I64, (TAG_OBJ << TAG_SHIFT) as i64);
            b.ins().bor(tag, val)
        }
        RegType::Unknown => {
            // Unknown type — decode as int (best effort for integer-mode functions).
            // ObjRef values stored in collections will be mis-tagged; such functions
            // should be rejected by type_analysis once per-value tagging is added.
            let tag = b.ins().iconst(I64, (TAG_INT << TAG_SHIFT) as i64);
            let masked = b.ins().band_imm(val, 0x0FFF_FFFF_FFFF_FFFF_i64);
            b.ins().bor(tag, masked)
        }
        RegType::Float => {
            // Should not happen in collection-mode (float+collection is rejected),
            // but handle defensively
            val
        }
    }
}

/// Emit Cranelift IR to decode a tagged i64 value back to a raw payload.
/// Extracts the lower 60 bits with sign extension for ints.
fn emit_tag_decode_int(
    b: &mut FunctionBuilder,
    tagged: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    // Extract payload (lower 60 bits)
    let payload = b.ins().band_imm(tagged, 0x0FFF_FFFF_FFFF_FFFF_i64);
    // Sign-extend from 60 bits: if bit 59 is set, fill upper bits with 1s
    let shifted_left = b.ins().ishl_imm(payload, 4);
    b.ins().sshr_imm(shifted_left, 4)
}

/// Emit Cranelift IR to decode a tagged i64 value and extract the raw GcRef index.
fn emit_tag_decode_obj(
    b: &mut FunctionBuilder,
    tagged: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    // ObjRef payload is always positive (usize), just mask
    b.ins().band_imm(tagged, 0x0FFF_FFFF_FFFF_FFFF_i64)
}

/// Helper to import a bridge function signature and get a FuncRef.
fn import_bridge<M: Module>(
    module: &mut M,
    b: &mut FunctionBuilder,
    name: &str,
    params: &[cranelift_codegen::ir::Type],
    returns: &[cranelift_codegen::ir::Type],
) -> Result<cranelift_codegen::ir::FuncRef, String> {
    let mut sig = module.make_signature();
    for &p in params {
        sig.params.push(AbiParam::new(p));
    }
    for &r in returns {
        sig.returns.push(AbiParam::new(r));
    }
    let func_id = module
        .declare_function(name, cranelift_module::Linkage::Import, &sig)
        .map_err(|e| format!("declare {}: {}", name, e))?;
    Ok(module.declare_func_in_func(func_id, b.func))
}

pub fn build_function<M: Module>(
    module: &mut M,
    chunk: &Chunk,
    func_name: &str,
    string_refs: Option<&StringRefs>,
) -> Result<cranelift_module::FuncId, String> {
    let type_info = type_analysis::analyze(chunk);
    if type_info.has_unsupported_ops {
        return Err("function uses unsupported operations (strings/arrays/objects)".to_string());
    }

    // Unified condition: need vm_ptr for string ops OR collection ops
    let needs_vm_ptr = type_info.has_string_ops || type_info.has_collection_ops;

    // String/collection functions cannot use floats (rejected by type_analysis).
    let ret_type = if type_info.has_float { F64 } else { I64 };
    let param_type = if type_info.has_float { F64 } else { I64 };

    let mut sig = module.make_signature();
    if needs_vm_ptr {
        sig.params.push(AbiParam::new(I64)); // vm_ptr
    }
    for _ in 0..chunk.arity {
        sig.params.push(AbiParam::new(param_type));
    }
    sig.returns.push(AbiParam::new(ret_type));

    let func_id = module
        .declare_function(func_name, cranelift_module::Linkage::Local, &sig)
        .map_err(|e| format!("declare error: {}", e))?;

    let mut ctx = module.make_context();
    ctx.func.signature = sig.clone();
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());

    let mut fbc = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fbc);

        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        b.seal_block(entry);

        let self_ref = module.declare_func_in_func(func_id, b.func);

        // Import string bridges (when has_string_ops)
        let bridge_concat = if type_info.has_string_ops {
            Some(import_bridge(
                module,
                &mut b,
                "rt_string_concat",
                &[I64, I64, I64],
                &[I64],
            )?)
        } else {
            None
        };
        let bridge_eq = if type_info.has_string_ops {
            Some(import_bridge(
                module,
                &mut b,
                "rt_string_eq",
                &[I64, I64, I64],
                &[I64],
            )?)
        } else {
            None
        };

        // Import generalized Len bridge (when has_string_ops OR has_collection_ops)
        let bridge_len = if needs_vm_ptr {
            Some(import_bridge(
                module,
                &mut b,
                "rt_obj_len",
                &[I64, I64],
                &[I64],
            )?)
        } else {
            None
        };

        // Import collection bridges (when has_collection_ops)
        struct CollectionBridges {
            array_new: cranelift_codegen::ir::FuncRef,
            empty_array: cranelift_codegen::ir::FuncRef,
            array_get: cranelift_codegen::ir::FuncRef,
            array_set: cranelift_codegen::ir::FuncRef,
            object_new: cranelift_codegen::ir::FuncRef,
            empty_object: cranelift_codegen::ir::FuncRef,
            object_get: cranelift_codegen::ir::FuncRef,
            object_set: cranelift_codegen::ir::FuncRef,
            extract_field: cranelift_codegen::ir::FuncRef,
            interpolate: cranelift_codegen::ir::FuncRef,
            empty_string: cranelift_codegen::ir::FuncRef,
        }

        let coll = if type_info.has_collection_ops {
            Some(CollectionBridges {
                array_new: import_bridge(module, &mut b, "rt_array_new", &[I64, I64, I64], &[I64])?,
                empty_array: import_bridge(module, &mut b, "rt_empty_array", &[I64], &[I64])?,
                array_get: import_bridge(module, &mut b, "rt_array_get", &[I64, I64, I64], &[I64])?,
                array_set: import_bridge(
                    module,
                    &mut b,
                    "rt_array_set",
                    &[I64, I64, I64, I64],
                    &[],
                )?,
                object_new: import_bridge(
                    module,
                    &mut b,
                    "rt_object_new",
                    &[I64, I64, I64],
                    &[I64],
                )?,
                empty_object: import_bridge(module, &mut b, "rt_empty_object", &[I64], &[I64])?,
                object_get: import_bridge(
                    module,
                    &mut b,
                    "rt_object_get",
                    &[I64, I64, I64],
                    &[I64],
                )?,
                object_set: import_bridge(
                    module,
                    &mut b,
                    "rt_object_set",
                    &[I64, I64, I64, I64],
                    &[],
                )?,
                extract_field: import_bridge(
                    module,
                    &mut b,
                    "rt_extract_field",
                    &[I64, I64, I64],
                    &[I64],
                )?,
                interpolate: import_bridge(
                    module,
                    &mut b,
                    "rt_interpolate",
                    &[I64, I64, I64],
                    &[I64],
                )?,
                empty_string: import_bridge(module, &mut b, "rt_empty_string", &[I64], &[I64])?,
            })
        } else {
            None
        };

        let num_regs = (chunk.max_registers.max(chunk.arity) as usize) + 1;
        let var_type = if type_info.has_float { F64 } else { I64 };
        let mut regs: Vec<Variable> = Vec::with_capacity(num_regs);
        for _ in 0..num_regs {
            regs.push(b.declare_var(var_type));
        }

        // For string/collection functions, first param is vm_ptr
        let vm_ptr_var = if needs_vm_ptr {
            let var = b.declare_var(I64);
            let vm_param = b.block_params(entry)[0];
            b.def_var(var, vm_param);
            Some(var)
        } else {
            None
        };

        let param_offset = if needs_vm_ptr { 1 } else { 0 };
        for i in 0..chunk.arity as usize {
            let param = b.block_params(entry)[i + param_offset];
            b.def_var(regs[i], param);
        }
        let zero_val = if type_info.has_float {
            b.ins().f64const(0.0)
        } else {
            b.ins().iconst(I64, 0)
        };
        for i in chunk.arity as usize..num_regs {
            b.def_var(regs[i], zero_val);
        }

        let code_len = chunk.code.len();
        let mut blocks = Vec::with_capacity(code_len + 1);
        for _ in 0..=code_len {
            blocks.push(b.create_block());
        }
        b.ins().jump(blocks[0], &[]);

        let use_float = type_info.has_float;

        for (ip, &inst) in chunk.code.iter().enumerate() {
            b.switch_to_block(blocks[ip]);
            let op = decode_op(inst);
            let a = decode_a(inst) as usize;
            let bb = decode_b(inst) as usize;
            let cc = decode_c(inst) as usize;
            let bx = decode_bx(inst);
            let sbx = decode_sbx(inst);
            let next = blocks[ip + 1];
            let Ok(opcode) = OpCode::try_from(op) else {
                b.ins().jump(next, &[]);
                continue;
            };

            match opcode {
                OpCode::LoadNull => {
                    let v = if use_float {
                        b.ins().f64const(0.0)
                    } else {
                        b.ins().iconst(I64, 0)
                    };
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadTrue => {
                    let v = if use_float {
                        b.ins().f64const(1.0)
                    } else {
                        b.ins().iconst(I64, 1)
                    };
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadFalse => {
                    let v = if use_float {
                        b.ins().f64const(0.0)
                    } else {
                        b.ins().iconst(I64, 0)
                    };
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadConst => {
                    let v = if use_float {
                        match &chunk.constants[bx as usize] {
                            Constant::Int(n) => b.ins().f64const(*n as f64),
                            Constant::Float(f) => b.ins().f64const(*f),
                            Constant::Bool(v) => b.ins().f64const(if *v { 1.0 } else { 0.0 }),
                            _ => b.ins().f64const(0.0),
                        }
                    } else {
                        match &chunk.constants[bx as usize] {
                            Constant::Int(n) => b.ins().iconst(I64, *n),
                            Constant::Float(f) => b.ins().iconst(I64, *f as i64),
                            Constant::Bool(v) => b.ins().iconst(I64, if *v { 1 } else { 0 }),
                            Constant::Str(_) => {
                                // Load pre-allocated GcRef index for this string constant
                                let gc_idx = string_refs
                                    .and_then(|refs| refs.get(bx as usize))
                                    .and_then(|r| *r)
                                    .unwrap_or(0);
                                b.ins().iconst(I64, gc_idx)
                            }
                            _ => b.ins().iconst(I64, 0),
                        }
                    };
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::Move | OpCode::GetLocal | OpCode::SetLocal => {
                    let v = b.use_var(regs[bb]);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::Add => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = if use_float {
                        b.ins().fadd(l, r)
                    } else {
                        b.ins().iadd(l, r)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Sub => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = if use_float {
                        b.ins().fsub(l, r)
                    } else {
                        b.ins().isub(l, r)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Mul => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = if use_float {
                        b.ins().fmul(l, r)
                    } else {
                        b.ins().imul(l, r)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Div => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = if use_float {
                        b.ins().fdiv(l, r)
                    } else {
                        b.ins().sdiv(l, r)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Mod => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = if use_float {
                        // fmod: a - trunc(a/b) * b
                        let div = b.ins().fdiv(l, r);
                        let trunc = b.ins().trunc(div);
                        let prod = b.ins().fmul(trunc, r);
                        b.ins().fsub(l, prod)
                    } else {
                        b.ins().srem(l, r)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Neg => {
                    let v = b.use_var(regs[bb]);
                    let result = if use_float {
                        b.ins().fneg(v)
                    } else {
                        b.ins().ineg(v)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Eq
                | OpCode::NotEq
                | OpCode::Lt
                | OpCode::Gt
                | OpCode::LtEq
                | OpCode::GtEq => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    // String equality: call rt_string_eq bridge
                    let is_string_cmp = bb < type_info.reg_types.len()
                        && cc < type_info.reg_types.len()
                        && type_info.reg_types[bb] == RegType::StringRef
                        && type_info.reg_types[cc] == RegType::StringRef
                        && matches!(opcode, OpCode::Eq | OpCode::NotEq);
                    let cmp_val = if is_string_cmp {
                        let vm_val = b.use_var(vm_ptr_var.expect("BUG: string op without vm_ptr"));
                        let eq_ref = bridge_eq.expect("BUG: string op without bridge_eq");
                        let call_inst = b.ins().call(eq_ref, &[vm_val, l, r]);
                        let eq_result = b.inst_results(call_inst)[0];
                        if matches!(opcode, OpCode::NotEq) {
                            // Flip: eq returns 1 for equal, we want 1 for not-equal
                            let one = b.ins().iconst(I64, 1);
                            b.ins().isub(one, eq_result)
                        } else {
                            eq_result
                        }
                    } else if use_float {
                        let fcc = match opcode {
                            OpCode::Eq => FloatCC::Equal,
                            OpCode::NotEq => FloatCC::NotEqual,
                            OpCode::Lt => FloatCC::LessThan,
                            OpCode::Gt => FloatCC::GreaterThan,
                            OpCode::LtEq => FloatCC::LessThanOrEqual,
                            OpCode::GtEq => FloatCC::GreaterThanOrEqual,
                            _ => unreachable!(),
                        };
                        let cmp = b.ins().fcmp(fcc, l, r);
                        let extended = b.ins().uextend(I64, cmp);
                        b.ins().fcvt_from_uint(F64, extended)
                    } else {
                        let icc = match opcode {
                            OpCode::Eq => IntCC::Equal,
                            OpCode::NotEq => IntCC::NotEqual,
                            OpCode::Lt => IntCC::SignedLessThan,
                            OpCode::Gt => IntCC::SignedGreaterThan,
                            OpCode::LtEq => IntCC::SignedLessThanOrEqual,
                            OpCode::GtEq => IntCC::SignedGreaterThanOrEqual,
                            _ => unreachable!(),
                        };
                        let cmp = b.ins().icmp(icc, l, r);
                        b.ins().uextend(I64, cmp)
                    };
                    b.def_var(regs[a], cmp_val);
                    b.ins().jump(next, &[]);
                }
                OpCode::Not => {
                    let v = b.use_var(regs[bb]);
                    let result = if use_float {
                        let zero = b.ins().f64const(0.0);
                        let is_zero = b.ins().fcmp(FloatCC::Equal, v, zero);
                        let extended = b.ins().uextend(I64, is_zero);
                        b.ins().fcvt_from_uint(F64, extended)
                    } else {
                        let zero = b.ins().iconst(I64, 0);
                        let is_zero = b.ins().icmp(IntCC::Equal, v, zero);
                        b.ins().uextend(I64, is_zero)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::And => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = if use_float {
                        let zero = b.ins().f64const(0.0);
                        let l_truthy = b.ins().fcmp(FloatCC::NotEqual, l, zero);
                        let r_truthy = b.ins().fcmp(FloatCC::NotEqual, r, zero);
                        let both = b.ins().band(l_truthy, r_truthy);
                        let extended = b.ins().uextend(I64, both);
                        b.ins().fcvt_from_uint(F64, extended)
                    } else {
                        let zero = b.ins().iconst(I64, 0);
                        let l_truthy = b.ins().icmp(IntCC::NotEqual, l, zero);
                        let r_truthy = b.ins().icmp(IntCC::NotEqual, r, zero);
                        let both = b.ins().band(l_truthy, r_truthy);
                        b.ins().uextend(I64, both)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Or => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = if use_float {
                        let zero = b.ins().f64const(0.0);
                        let l_truthy = b.ins().fcmp(FloatCC::NotEqual, l, zero);
                        let r_truthy = b.ins().fcmp(FloatCC::NotEqual, r, zero);
                        let either = b.ins().bor(l_truthy, r_truthy);
                        let extended = b.ins().uextend(I64, either);
                        b.ins().fcvt_from_uint(F64, extended)
                    } else {
                        let zero = b.ins().iconst(I64, 0);
                        let l_truthy = b.ins().icmp(IntCC::NotEqual, l, zero);
                        let r_truthy = b.ins().icmp(IntCC::NotEqual, r, zero);
                        let either = b.ins().bor(l_truthy, r_truthy);
                        b.ins().uextend(I64, either)
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Jump | OpCode::Loop => {
                    let target = ((ip as i32) + 1 + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    b.ins().jump(t, &[]);
                }
                OpCode::JumpIfFalse => {
                    let cond = b.use_var(regs[a]);
                    let target = ((ip as i32) + 1 + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    if use_float {
                        let zero = b.ins().f64const(0.0);
                        let is_false = b.ins().fcmp(FloatCC::Equal, cond, zero);
                        b.ins().brif(is_false, t, &[], next, &[]);
                    } else {
                        let zero = b.ins().iconst(I64, 0);
                        let is_false = b.ins().icmp(IntCC::Equal, cond, zero);
                        b.ins().brif(is_false, t, &[], next, &[]);
                    }
                }
                OpCode::JumpIfTrue => {
                    let cond = b.use_var(regs[a]);
                    let target = ((ip as i32) + 1 + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    if use_float {
                        let zero = b.ins().f64const(0.0);
                        let is_true = b.ins().fcmp(FloatCC::NotEqual, cond, zero);
                        b.ins().brif(is_true, t, &[], next, &[]);
                    } else {
                        let zero = b.ins().iconst(I64, 0);
                        let is_true = b.ins().icmp(IntCC::NotEqual, cond, zero);
                        b.ins().brif(is_true, t, &[], next, &[]);
                    }
                }
                OpCode::GetGlobal
                | OpCode::SetGlobal
                | OpCode::Closure
                | OpCode::GetUpvalue
                | OpCode::SetUpvalue => {
                    b.ins().jump(next, &[]);
                }
                OpCode::Call => {
                    let arg_count = bb;
                    let dst = cc;
                    let mut call_args = Vec::with_capacity(arg_count + 1);
                    // Pass vm_ptr as first arg when in string/collection mode
                    if let Some(vp) = vm_ptr_var {
                        call_args.push(b.use_var(vp));
                    }
                    for i in 0..arg_count {
                        call_args.push(b.use_var(regs[a + 1 + i]));
                    }
                    let call_inst = b.ins().call(self_ref, &call_args);
                    let result = b.inst_results(call_inst)[0];
                    b.def_var(regs[dst], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Return => {
                    let val = b.use_var(regs[a]);
                    b.ins().return_(&[val]);
                }
                OpCode::ReturnNull => {
                    let zero = if use_float {
                        b.ins().f64const(0.0)
                    } else {
                        b.ins().iconst(I64, 0)
                    };
                    b.ins().return_(&[zero]);
                }
                OpCode::Concat => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: Concat without vm_ptr"));
                    let concat_ref = bridge_concat.expect("BUG: Concat without bridge_concat");
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let call_inst = b.ins().call(concat_ref, &[vm_val, l, r]);
                    let result = b.inst_results(call_inst)[0];
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Len => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: Len without vm_ptr"));
                    let len_ref = bridge_len.expect("BUG: Len without bridge_len");
                    let s = b.use_var(regs[bb]);
                    let call_inst = b.ins().call(len_ref, &[vm_val, s]);
                    let result = b.inst_results(call_inst)[0];
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }

                // ---- Collection opcodes ----
                OpCode::NewArray => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: NewArray without vm_ptr"));
                    let coll_ref = coll
                        .as_ref()
                        .expect("BUG: NewArray without collection bridges");
                    let start = bb;
                    let count = cc;
                    let result = if count == 0 {
                        let call_inst = b.ins().call(coll_ref.empty_array, &[vm_val]);
                        b.inst_results(call_inst)[0]
                    } else {
                        // Stack-allocate buffer for tagged elements
                        let slot = b.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            (count * 8) as u32,
                            8,
                        ));
                        for i in 0..count {
                            let val = b.use_var(regs[start + i]);
                            let reg_type = type_info
                                .reg_types
                                .get(start + i)
                                .copied()
                                .unwrap_or(RegType::Unknown);
                            let tagged = emit_tag_encode(&mut b, val, reg_type);
                            b.ins().stack_store(tagged, slot, (i * 8) as i32);
                        }
                        let ptr = b.ins().stack_addr(I64, slot, 0);
                        let count_val = b.ins().iconst(I64, count as i64);
                        let call_inst = b.ins().call(coll_ref.array_new, &[vm_val, ptr, count_val]);
                        b.inst_results(call_inst)[0]
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::GetIndex => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: GetIndex without vm_ptr"));
                    let coll_ref = coll
                        .as_ref()
                        .expect("BUG: GetIndex without collection bridges");
                    let arr = b.use_var(regs[bb]);
                    let idx = b.use_var(regs[cc]);
                    let call_inst = b.ins().call(coll_ref.array_get, &[vm_val, arr, idx]);
                    let tagged_result = b.inst_results(call_inst)[0];
                    // Decode based on destination register type
                    let dst_type = type_info
                        .reg_types
                        .get(a)
                        .copied()
                        .unwrap_or(RegType::Unknown);
                    let decoded = match dst_type {
                        RegType::ObjRef | RegType::StringRef => {
                            emit_tag_decode_obj(&mut b, tagged_result)
                        }
                        _ => emit_tag_decode_int(&mut b, tagged_result),
                    };
                    b.def_var(regs[a], decoded);
                    b.ins().jump(next, &[]);
                }
                OpCode::SetIndex => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: SetIndex without vm_ptr"));
                    let coll_ref = coll
                        .as_ref()
                        .expect("BUG: SetIndex without collection bridges");
                    let arr = b.use_var(regs[a]);
                    let idx = b.use_var(regs[bb]);
                    let val = b.use_var(regs[cc]);
                    let reg_type = type_info
                        .reg_types
                        .get(cc)
                        .copied()
                        .unwrap_or(RegType::Unknown);
                    let tagged_val = emit_tag_encode(&mut b, val, reg_type);
                    b.ins()
                        .call(coll_ref.array_set, &[vm_val, arr, idx, tagged_val]);
                    b.ins().jump(next, &[]);
                }
                OpCode::NewObject => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: NewObject without vm_ptr"));
                    let coll_ref = coll
                        .as_ref()
                        .expect("BUG: NewObject without collection bridges");
                    let start = bb;
                    let pair_count = cc;
                    let result = if pair_count == 0 {
                        let call_inst = b.ins().call(coll_ref.empty_object, &[vm_val]);
                        b.inst_results(call_inst)[0]
                    } else {
                        // Stack-allocate buffer for tagged key-value pairs
                        let slot = b.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            (pair_count * 2 * 8) as u32,
                            8,
                        ));
                        for i in 0..pair_count {
                            let key = b.use_var(regs[start + i * 2]);
                            let val = b.use_var(regs[start + i * 2 + 1]);
                            // Keys are always StringRef
                            let tagged_key = emit_tag_encode(&mut b, key, RegType::StringRef);
                            let val_reg_type = type_info
                                .reg_types
                                .get(start + i * 2 + 1)
                                .copied()
                                .unwrap_or(RegType::Unknown);
                            let tagged_val = emit_tag_encode(&mut b, val, val_reg_type);
                            b.ins().stack_store(tagged_key, slot, (i * 2 * 8) as i32);
                            b.ins()
                                .stack_store(tagged_val, slot, ((i * 2 + 1) * 8) as i32);
                        }
                        let ptr = b.ins().stack_addr(I64, slot, 0);
                        let count_val = b.ins().iconst(I64, pair_count as i64);
                        let call_inst =
                            b.ins().call(coll_ref.object_new, &[vm_val, ptr, count_val]);
                        b.inst_results(call_inst)[0]
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::GetField => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: GetField without vm_ptr"));
                    let coll_ref = coll
                        .as_ref()
                        .expect("BUG: GetField without collection bridges");
                    let obj = b.use_var(regs[bb]);
                    // C is the constant pool index for the field name string.
                    // Load the pre-interned GcRef index for this constant.
                    let field_gc_idx = string_refs
                        .and_then(|refs| refs.get(cc))
                        .and_then(|r| *r)
                        .unwrap_or(0);
                    let field_ref = b.ins().iconst(I64, field_gc_idx);
                    let call_inst = b.ins().call(coll_ref.object_get, &[vm_val, obj, field_ref]);
                    let tagged_result = b.inst_results(call_inst)[0];
                    let dst_type = type_info
                        .reg_types
                        .get(a)
                        .copied()
                        .unwrap_or(RegType::Unknown);
                    let decoded = match dst_type {
                        RegType::ObjRef | RegType::StringRef => {
                            emit_tag_decode_obj(&mut b, tagged_result)
                        }
                        _ => emit_tag_decode_int(&mut b, tagged_result),
                    };
                    b.def_var(regs[a], decoded);
                    b.ins().jump(next, &[]);
                }
                OpCode::SetField => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: SetField without vm_ptr"));
                    let coll_ref = coll
                        .as_ref()
                        .expect("BUG: SetField without collection bridges");
                    let obj = b.use_var(regs[a]);
                    // B is the constant pool index for the field name
                    let field_gc_idx = string_refs
                        .and_then(|refs| refs.get(bb))
                        .and_then(|r| *r)
                        .unwrap_or(0);
                    let field_ref = b.ins().iconst(I64, field_gc_idx);
                    let val = b.use_var(regs[cc]);
                    let reg_type = type_info
                        .reg_types
                        .get(cc)
                        .copied()
                        .unwrap_or(RegType::Unknown);
                    let tagged_val = emit_tag_encode(&mut b, val, reg_type);
                    b.ins()
                        .call(coll_ref.object_set, &[vm_val, obj, field_ref, tagged_val]);
                    b.ins().jump(next, &[]);
                }
                OpCode::ExtractField => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: ExtractField without vm_ptr"));
                    let coll_ref = coll
                        .as_ref()
                        .expect("BUG: ExtractField without collection bridges");
                    let obj = b.use_var(regs[bb]);
                    let field_idx = b.ins().iconst(I64, cc as i64);
                    let call_inst = b
                        .ins()
                        .call(coll_ref.extract_field, &[vm_val, obj, field_idx]);
                    let tagged_result = b.inst_results(call_inst)[0];
                    let dst_type = type_info
                        .reg_types
                        .get(a)
                        .copied()
                        .unwrap_or(RegType::Unknown);
                    let decoded = match dst_type {
                        RegType::ObjRef | RegType::StringRef => {
                            emit_tag_decode_obj(&mut b, tagged_result)
                        }
                        _ => emit_tag_decode_int(&mut b, tagged_result),
                    };
                    b.def_var(regs[a], decoded);
                    b.ins().jump(next, &[]);
                }
                OpCode::Interpolate => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: Interpolate without vm_ptr"));
                    let coll_ref = coll
                        .as_ref()
                        .expect("BUG: Interpolate without collection bridges");
                    let start = bb;
                    let count = cc;
                    let result = if count == 0 {
                        let call_inst = b.ins().call(coll_ref.empty_string, &[vm_val]);
                        b.inst_results(call_inst)[0]
                    } else {
                        let slot = b.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            (count * 8) as u32,
                            8,
                        ));
                        for i in 0..count {
                            let val = b.use_var(regs[start + i]);
                            let reg_type = type_info
                                .reg_types
                                .get(start + i)
                                .copied()
                                .unwrap_or(RegType::Unknown);
                            let tagged = emit_tag_encode(&mut b, val, reg_type);
                            b.ins().stack_store(tagged, slot, (i * 8) as i32);
                        }
                        let ptr = b.ins().stack_addr(I64, slot, 0);
                        let count_val = b.ins().iconst(I64, count as i64);
                        let call_inst = b
                            .ins()
                            .call(coll_ref.interpolate, &[vm_val, ptr, count_val]);
                        b.inst_results(call_inst)[0]
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }

                _ => {
                    b.ins().jump(next, &[]);
                }
            }
        }

        b.switch_to_block(blocks[code_len]);
        let final_zero = if use_float {
            b.ins().f64const(0.0)
        } else {
            b.ins().iconst(I64, 0)
        };
        b.ins().return_(&[final_zero]);

        for block in &blocks {
            b.seal_block(*block);
        }
        b.finalize();
    }

    module
        .define_function(func_id, &mut ctx)
        .map_err(|e| format!("define error: {}", e))?;

    Ok(func_id)
}
