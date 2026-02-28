use cranelift_codegen::ir::condcodes::IntCC;
/// Translates Forge bytecode (Chunk) into Cranelift IR.
/// Shared core used by both JIT and AOT compilation.
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, UserFuncName};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::Module;

use crate::vm::bytecode::*;
use crate::vm::jit::runtime;

/// Build a Cranelift IR function from a bytecode Chunk.
/// Signature: fn(vm_ptr: i64, arg0: i64, arg1: i64, ...) -> i64
/// All values are tagged 64-bit integers (see runtime.rs encoding).
pub fn build_function<M: Module>(
    module: &mut M,
    chunk: &Chunk,
    func_name: &str,
) -> Result<cranelift_module::FuncId, String> {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(I64));
    for _ in 0..chunk.arity {
        sig.params.push(AbiParam::new(I64));
    }
    sig.returns.push(AbiParam::new(I64));

    let func_id = module
        .declare_function(func_name, cranelift_module::Linkage::Local, &sig)
        .map_err(|e| format!("declare error: {}", e))?;

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());

    let mut fbc = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fbc);

        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        b.seal_block(entry);

        let num_regs = (chunk.max_registers.max(chunk.arity) as usize) + 1;
        let mut regs: Vec<Variable> = Vec::with_capacity(num_regs);
        for _ in 0..num_regs {
            let v = b.declare_var(I64);
            regs.push(v);
        }

        for i in 0..chunk.arity as usize {
            let param = b.block_params(entry)[1 + i];
            b.def_var(regs[i], param);
        }
        let null_enc = runtime::encode_null() as i64;
        for i in chunk.arity as usize..num_regs {
            let nv = b.ins().iconst(I64, null_enc);
            b.def_var(regs[i], nv);
        }

        let code_len = chunk.code.len();
        let mut blocks = Vec::with_capacity(code_len + 1);
        for _ in 0..=code_len {
            blocks.push(b.create_block());
        }

        b.ins().jump(blocks[0], &[]);

        let mask_val: i64 = ((1u64 << 60) - 1) as i64;
        let bool_tag: i64 = runtime::encode_bool(false) as i64;

        for (ip, &inst) in chunk.code.iter().enumerate() {
            b.switch_to_block(blocks[ip]);

            let op = decode_op(inst);
            let a = decode_a(inst) as usize;
            let bb = decode_b(inst) as usize;
            let cc = decode_c(inst) as usize;
            let bx = decode_bx(inst);
            let sbx = decode_sbx(inst);
            let opcode: OpCode = unsafe { std::mem::transmute(op) };

            let next = blocks[ip + 1];

            match opcode {
                OpCode::LoadNull => {
                    let v = b.ins().iconst(I64, null_enc);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadTrue => {
                    let v = b.ins().iconst(I64, runtime::encode_bool(true) as i64);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadFalse => {
                    let v = b.ins().iconst(I64, bool_tag);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::LoadConst => {
                    let val = match &chunk.constants[bx as usize] {
                        Constant::Int(n) => runtime::encode_int(*n) as i64,
                        Constant::Float(f) => {
                            let bits = f.to_bits();
                            ((1u64 << 60) | (bits & ((1u64 << 60) - 1))) as i64
                        }
                        Constant::Bool(v) => runtime::encode_bool(*v) as i64,
                        _ => null_enc,
                    };
                    let v = b.ins().iconst(I64, val);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::Move => {
                    let v = b.use_var(regs[bb]);
                    b.def_var(regs[a], v);
                    b.ins().jump(next, &[]);
                }
                OpCode::Add | OpCode::Sub | OpCode::Mul => {
                    let left = b.use_var(regs[bb]);
                    let right = b.use_var(regs[cc]);
                    let mask = b.ins().iconst(I64, mask_val);
                    let l = b.ins().band(left, mask);
                    let r = b.ins().band(right, mask);
                    let result = match opcode {
                        OpCode::Add => b.ins().iadd(l, r),
                        OpCode::Sub => b.ins().isub(l, r),
                        OpCode::Mul => b.ins().imul(l, r),
                        _ => unreachable!(),
                    };
                    let masked = b.ins().band(result, mask);
                    b.def_var(regs[a], masked);
                    b.ins().jump(next, &[]);
                }
                OpCode::Div => {
                    let left = b.use_var(regs[bb]);
                    let right = b.use_var(regs[cc]);
                    let mask = b.ins().iconst(I64, mask_val);
                    let l = b.ins().band(left, mask);
                    let r = b.ins().band(right, mask);
                    let result = b.ins().sdiv(l, r);
                    let masked = b.ins().band(result, mask);
                    b.def_var(regs[a], masked);
                    b.ins().jump(next, &[]);
                }
                OpCode::Eq | OpCode::NotEq => {
                    let left = b.use_var(regs[bb]);
                    let right = b.use_var(regs[cc]);
                    let cc_code = if opcode == OpCode::Eq {
                        IntCC::Equal
                    } else {
                        IntCC::NotEqual
                    };
                    let cmp = b.ins().icmp(cc_code, left, right);
                    let ext = b.ins().uextend(I64, cmp);
                    let tag = b.ins().iconst(I64, bool_tag);
                    let result = b.ins().bor(tag, ext);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Lt | OpCode::Gt | OpCode::LtEq | OpCode::GtEq => {
                    let left = b.use_var(regs[bb]);
                    let right = b.use_var(regs[cc]);
                    let mask = b.ins().iconst(I64, mask_val);
                    let l = b.ins().band(left, mask);
                    let r = b.ins().band(right, mask);
                    let cc_code = match opcode {
                        OpCode::Lt => IntCC::SignedLessThan,
                        OpCode::Gt => IntCC::SignedGreaterThan,
                        OpCode::LtEq => IntCC::SignedLessThanOrEqual,
                        OpCode::GtEq => IntCC::SignedGreaterThanOrEqual,
                        _ => unreachable!(),
                    };
                    let cmp = b.ins().icmp(cc_code, l, r);
                    let ext = b.ins().uextend(I64, cmp);
                    let tag = b.ins().iconst(I64, bool_tag);
                    let result = b.ins().bor(tag, ext);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Jump | OpCode::Loop => {
                    let target = ((ip as i32) + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    b.ins().jump(t, &[]);
                }
                OpCode::JumpIfFalse => {
                    let cond = b.use_var(regs[a]);
                    let one = b.ins().iconst(I64, 1);
                    let payload = b.ins().band(cond, one);
                    let zero = b.ins().iconst(I64, 0);
                    let is_false = b.ins().icmp(IntCC::Equal, payload, zero);
                    let target = ((ip as i32) + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    b.ins().brif(is_false, t, &[], next, &[]);
                }
                OpCode::JumpIfTrue => {
                    let cond = b.use_var(regs[a]);
                    let one = b.ins().iconst(I64, 1);
                    let payload = b.ins().band(cond, one);
                    let zero = b.ins().iconst(I64, 0);
                    let is_true = b.ins().icmp(IntCC::NotEqual, payload, zero);
                    let target = ((ip as i32) + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    b.ins().brif(is_true, t, &[], next, &[]);
                }
                OpCode::Return => {
                    let val = b.use_var(regs[a]);
                    b.ins().return_(&[val]);
                }
                OpCode::ReturnNull => {
                    let nv = b.ins().iconst(I64, null_enc);
                    b.ins().return_(&[nv]);
                }
                _ => {
                    let nv = b.ins().iconst(I64, null_enc);
                    b.ins().return_(&[nv]);
                }
            }
        }

        b.switch_to_block(blocks[code_len]);
        let nv = b.ins().iconst(I64, null_enc);
        b.ins().return_(&[nv]);

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
