/// Translates Forge bytecode to Cranelift IR.
/// I64-everywhere ABI: all registers, params, and returns use I64.
/// Float values are stored as IEEE 754 bit patterns via bitcast I64↔F64.
/// Functions with string/collection/global ops call runtime bridges.
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
            // Float values are stored as IEEE 754 bits in I64 registers.
            // Tag with TAG_FLOAT (1) for bridge calls.
            let tag = b.ins().iconst(I64, (1_u64 << TAG_SHIFT) as i64);
            let masked = b.ins().band_imm(val, 0x0FFF_FFFF_FFFF_FFFF_i64);
            b.ins().bor(tag, masked)
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

    // Unified condition: need vm_ptr for string ops, collection ops, or global access
    let needs_vm_ptr =
        type_info.has_string_ops || type_info.has_collection_ops || type_info.has_global_ops;

    // I64-everywhere ABI: all params and returns are I64.
    // Float values are stored as their IEEE 754 bit pattern (via bitcast).
    // This allows mixing float ops with string/collection/global bridges.
    let mut sig = module.make_signature();
    if needs_vm_ptr {
        sig.params.push(AbiParam::new(I64)); // vm_ptr
    }
    for _ in 0..chunk.arity {
        sig.params.push(AbiParam::new(I64));
    }
    sig.returns.push(AbiParam::new(I64));

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

        // Import global access and general call bridges (when has_global_ops)
        struct GlobalBridges {
            get_global: cranelift_codegen::ir::FuncRef,
            set_global: cranelift_codegen::ir::FuncRef,
            call_native: cranelift_codegen::ir::FuncRef,
        }

        let glob = if type_info.has_global_ops {
            Some(GlobalBridges {
                // rt_get_global(vm_ptr, name_ref) -> tagged_val
                get_global: import_bridge(module, &mut b, "rt_get_global", &[I64, I64], &[I64])?,
                // rt_set_global(vm_ptr, name_ref, tagged_val)
                set_global: import_bridge(module, &mut b, "rt_set_global", &[I64, I64, I64], &[])?,
                // rt_call_native(vm_ptr, func_tagged, args_ptr, argc) -> tagged_val
                call_native: import_bridge(
                    module,
                    &mut b,
                    "rt_call_native",
                    &[I64, I64, I64, I64],
                    &[I64],
                )?,
            })
        } else {
            None
        };

        let num_regs = (chunk.max_registers.max(chunk.arity) as usize) + 1;
        let mut regs: Vec<Variable> = Vec::with_capacity(num_regs);
        for _ in 0..num_regs {
            regs.push(b.declare_var(I64));
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
        let zero_val = b.ins().iconst(I64, 0);
        for i in chunk.arity as usize..num_regs {
            b.def_var(regs[i], zero_val);
        }

        let code_len = chunk.code.len();
        let mut blocks = Vec::with_capacity(code_len + 1);
        for _ in 0..=code_len {
            blocks.push(b.create_block());
        }
        b.ins().jump(blocks[0], &[]);

        // Helper closures for float bitcast in I64-everywhere mode.
        // Float values live as their IEEE 754 bit pattern in I64 registers.
        // reg_type lookup helper
        let reg_type = |idx: usize| -> RegType {
            type_info
                .reg_types
                .get(idx)
                .copied()
                .unwrap_or(RegType::Int)
        };

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
                    let v = b.ins().iconst(I64, 0);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadTrue => {
                    let v = b.ins().iconst(I64, 1);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadFalse => {
                    let v = b.ins().iconst(I64, 0);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadConst => {
                    let v = match &chunk.constants[bx as usize] {
                        Constant::Int(n) => b.ins().iconst(I64, *n),
                        Constant::Float(f) => {
                            // Store IEEE 754 bits in I64 register via bitcast
                            let fval = b.ins().f64const(*f);
                            b.ins()
                                .bitcast(I64, cranelift_codegen::ir::MemFlags::new(), fval)
                        }
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
                    };
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::Move | OpCode::GetLocal | OpCode::SetLocal => {
                    let v = b.use_var(regs[bb]);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod => {
                    let l_raw = b.use_var(regs[bb]);
                    let r_raw = b.use_var(regs[cc]);
                    let l_is_float = reg_type(bb) == RegType::Float;
                    let r_is_float = reg_type(cc) == RegType::Float;
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let result = if l_is_float || r_is_float {
                        // Bitcast I64 → F64 for float operands, convert int operands
                        let l = if l_is_float {
                            b.ins().bitcast(F64, mf, l_raw)
                        } else {
                            b.ins().fcvt_from_sint(F64, l_raw)
                        };
                        let r = if r_is_float {
                            b.ins().bitcast(F64, mf, r_raw)
                        } else {
                            b.ins().fcvt_from_sint(F64, r_raw)
                        };
                        let fres = match opcode {
                            OpCode::Add => b.ins().fadd(l, r),
                            OpCode::Sub => b.ins().fsub(l, r),
                            OpCode::Mul => b.ins().fmul(l, r),
                            OpCode::Div => b.ins().fdiv(l, r),
                            OpCode::Mod => {
                                let div = b.ins().fdiv(l, r);
                                let trunc = b.ins().trunc(div);
                                let prod = b.ins().fmul(trunc, r);
                                b.ins().fsub(l, prod)
                            }
                            _ => unreachable!(),
                        };
                        // Bitcast F64 → I64 to store in I64 register
                        b.ins().bitcast(I64, mf, fres)
                    } else {
                        match opcode {
                            OpCode::Add => b.ins().iadd(l_raw, r_raw),
                            OpCode::Sub => b.ins().isub(l_raw, r_raw),
                            OpCode::Mul => b.ins().imul(l_raw, r_raw),
                            OpCode::Div => b.ins().sdiv(l_raw, r_raw),
                            OpCode::Mod => b.ins().srem(l_raw, r_raw),
                            _ => unreachable!(),
                        }
                    };
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Neg => {
                    let v = b.use_var(regs[bb]);
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let result = if reg_type(bb) == RegType::Float {
                        let fv = b.ins().bitcast(F64, mf, v);
                        let neg = b.ins().fneg(fv);
                        b.ins().bitcast(I64, mf, neg)
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
                    let l_is_float = reg_type(bb) == RegType::Float;
                    let r_is_float = reg_type(cc) == RegType::Float;
                    let mf = cranelift_codegen::ir::MemFlags::new();
                    let cmp_val = if is_string_cmp {
                        let vm_val = b.use_var(vm_ptr_var.expect("BUG: string op without vm_ptr"));
                        let eq_ref = bridge_eq.expect("BUG: string op without bridge_eq");
                        let call_inst = b.ins().call(eq_ref, &[vm_val, l, r]);
                        let eq_result = b.inst_results(call_inst)[0];
                        if matches!(opcode, OpCode::NotEq) {
                            let one = b.ins().iconst(I64, 1);
                            b.ins().isub(one, eq_result)
                        } else {
                            eq_result
                        }
                    } else if l_is_float || r_is_float {
                        let lf = if l_is_float {
                            b.ins().bitcast(F64, mf, l)
                        } else {
                            b.ins().fcvt_from_sint(F64, l)
                        };
                        let rf = if r_is_float {
                            b.ins().bitcast(F64, mf, r)
                        } else {
                            b.ins().fcvt_from_sint(F64, r)
                        };
                        let fcc = match opcode {
                            OpCode::Eq => FloatCC::Equal,
                            OpCode::NotEq => FloatCC::NotEqual,
                            OpCode::Lt => FloatCC::LessThan,
                            OpCode::Gt => FloatCC::GreaterThan,
                            OpCode::LtEq => FloatCC::LessThanOrEqual,
                            OpCode::GtEq => FloatCC::GreaterThanOrEqual,
                            _ => unreachable!(),
                        };
                        let cmp = b.ins().fcmp(fcc, lf, rf);
                        b.ins().uextend(I64, cmp)
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
                    // All bools are I64 (0/1). Result is always I64.
                    let zero = b.ins().iconst(I64, 0);
                    let is_zero = b.ins().icmp(IntCC::Equal, v, zero);
                    let result = b.ins().uextend(I64, is_zero);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::And => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let zero = b.ins().iconst(I64, 0);
                    let l_truthy = b.ins().icmp(IntCC::NotEqual, l, zero);
                    let r_truthy = b.ins().icmp(IntCC::NotEqual, r, zero);
                    let both = b.ins().band(l_truthy, r_truthy);
                    let result = b.ins().uextend(I64, both);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Or => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let zero = b.ins().iconst(I64, 0);
                    let l_truthy = b.ins().icmp(IntCC::NotEqual, l, zero);
                    let r_truthy = b.ins().icmp(IntCC::NotEqual, r, zero);
                    let either = b.ins().bor(l_truthy, r_truthy);
                    let result = b.ins().uextend(I64, either);
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
                    // All condition registers are I64 (bools = 0/1)
                    let zero = b.ins().iconst(I64, 0);
                    let is_false = b.ins().icmp(IntCC::Equal, cond, zero);
                    b.ins().brif(is_false, t, &[], next, &[]);
                }
                OpCode::JumpIfTrue => {
                    let cond = b.use_var(regs[a]);
                    let target = ((ip as i32) + 1 + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    let zero = b.ins().iconst(I64, 0);
                    let is_true = b.ins().icmp(IntCC::NotEqual, cond, zero);
                    b.ins().brif(is_true, t, &[], next, &[]);
                }
                OpCode::GetGlobal => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: GetGlobal without vm_ptr"));
                    let glob_ref = glob
                        .as_ref()
                        .expect("BUG: GetGlobal without global bridges");
                    // bx is the constant pool index for the global name string.
                    // Load the pre-interned GcRef index from string_refs.
                    let name_gc_idx = string_refs
                        .and_then(|refs| refs.get(bx as usize))
                        .and_then(|r| *r)
                        .unwrap_or(0);
                    let name_ref = b.ins().iconst(I64, name_gc_idx);
                    let call_inst = b.ins().call(glob_ref.get_global, &[vm_val, name_ref]);
                    let tagged_result = b.inst_results(call_inst)[0];
                    // Result is tagged; decode as int (Unknown registers).
                    let decoded = emit_tag_decode_int(&mut b, tagged_result);
                    b.def_var(regs[a], decoded);
                    b.ins().jump(next, &[]);
                }
                OpCode::SetGlobal => {
                    let vm_val = b.use_var(vm_ptr_var.expect("BUG: SetGlobal without vm_ptr"));
                    let glob_ref = glob
                        .as_ref()
                        .expect("BUG: SetGlobal without global bridges");
                    let name_gc_idx = string_refs
                        .and_then(|refs| refs.get(bx as usize))
                        .and_then(|r| *r)
                        .unwrap_or(0);
                    let name_ref = b.ins().iconst(I64, name_gc_idx);
                    let val = b.use_var(regs[a]);
                    let reg_type = type_info
                        .reg_types
                        .get(a)
                        .copied()
                        .unwrap_or(RegType::Unknown);
                    let tagged_val = emit_tag_encode(&mut b, val, reg_type);
                    b.ins()
                        .call(glob_ref.set_global, &[vm_val, name_ref, tagged_val]);
                    b.ins().jump(next, &[]);
                }
                OpCode::Closure | OpCode::GetUpvalue | OpCode::SetUpvalue => {
                    b.ins().jump(next, &[]);
                }
                OpCode::Call => {
                    let arg_count = bb;
                    let dst = cc;
                    if type_info.has_global_ops {
                        // General call via rt_call_native bridge.
                        // Function register may hold a value from GetGlobal.
                        let vm_val =
                            b.use_var(vm_ptr_var.expect("BUG: bridge Call without vm_ptr"));
                        let glob_ref = glob
                            .as_ref()
                            .expect("BUG: bridge Call without global bridges");
                        // Tag-encode the function value as TAG_OBJ (functions
                        // are always object references in the VM).
                        let func_val = b.use_var(regs[a]);
                        let func_tagged = emit_tag_encode(&mut b, func_val, RegType::ObjRef);
                        if arg_count == 0 {
                            // No args — pass null pointer and 0 count
                            let null_ptr = b.ins().iconst(I64, 0);
                            let zero = b.ins().iconst(I64, 0);
                            let call_inst = b
                                .ins()
                                .call(glob_ref.call_native, &[vm_val, func_tagged, null_ptr, zero]);
                            let tagged_result = b.inst_results(call_inst)[0];
                            let decoded = emit_tag_decode_int(&mut b, tagged_result);
                            b.def_var(regs[dst], decoded);
                        } else {
                            // Stack-allocate buffer for tagged arguments
                            let slot = b.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                (arg_count * 8) as u32,
                                8,
                            ));
                            for i in 0..arg_count {
                                let arg_val = b.use_var(regs[a + 1 + i]);
                                let arg_type = type_info
                                    .reg_types
                                    .get(a + 1 + i)
                                    .copied()
                                    .unwrap_or(RegType::Unknown);
                                let tagged_arg = emit_tag_encode(&mut b, arg_val, arg_type);
                                b.ins().stack_store(tagged_arg, slot, (i * 8) as i32);
                            }
                            let args_ptr = b.ins().stack_addr(I64, slot, 0);
                            let argc = b.ins().iconst(I64, arg_count as i64);
                            let call_inst = b
                                .ins()
                                .call(glob_ref.call_native, &[vm_val, func_tagged, args_ptr, argc]);
                            let tagged_result = b.inst_results(call_inst)[0];
                            let decoded = emit_tag_decode_int(&mut b, tagged_result);
                            b.def_var(regs[dst], decoded);
                        }
                    } else {
                        // Self-recursive call via direct Cranelift call
                        let mut call_args = Vec::with_capacity(arg_count + 1);
                        if let Some(vp) = vm_ptr_var {
                            call_args.push(b.use_var(vp));
                        }
                        for i in 0..arg_count {
                            call_args.push(b.use_var(regs[a + 1 + i]));
                        }
                        let call_inst = b.ins().call(self_ref, &call_args);
                        let result = b.inst_results(call_inst)[0];
                        b.def_var(regs[dst], result);
                    }
                    b.ins().jump(next, &[]);
                }
                OpCode::Return => {
                    let val = b.use_var(regs[a]);
                    b.ins().return_(&[val]);
                }
                OpCode::ReturnNull => {
                    let zero = b.ins().iconst(I64, 0);
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
        let final_zero = b.ins().iconst(I64, 0);
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
