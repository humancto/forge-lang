/// Translates Forge bytecode to Cranelift IR using raw i64 values.
/// No tagged encoding — integers are raw i64, bools are 0/1.
/// This works for integer-only functions (fib, factorial, etc).
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, UserFuncName};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::Module;

use crate::vm::bytecode::*;

pub fn build_function<M: Module>(
    module: &mut M,
    chunk: &Chunk,
    func_name: &str,
) -> Result<cranelift_module::FuncId, String> {
    let mut sig = module.make_signature();
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

        let num_regs = (chunk.max_registers.max(chunk.arity) as usize) + 1;
        let mut regs: Vec<Variable> = Vec::with_capacity(num_regs);
        for _ in 0..num_regs {
            regs.push(b.declare_var(I64));
        }

        for i in 0..chunk.arity as usize {
            let param = b.block_params(entry)[i];
            b.def_var(regs[i], param);
        }
        for i in chunk.arity as usize..num_regs {
            let zero = b.ins().iconst(I64, 0);
            b.def_var(regs[i], zero);
        }

        let code_len = chunk.code.len();
        let mut blocks = Vec::with_capacity(code_len + 1);
        for _ in 0..=code_len {
            blocks.push(b.create_block());
        }
        b.ins().jump(blocks[0], &[]);

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
                    let val = match &chunk.constants[bx as usize] {
                        Constant::Int(n) => *n,
                        Constant::Float(f) => *f as i64,
                        Constant::Bool(v) => {
                            if *v {
                                1
                            } else {
                                0
                            }
                        }
                        _ => 0,
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
                OpCode::Add => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = b.ins().iadd(l, r);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Sub => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = b.ins().isub(l, r);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Mul => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = b.ins().imul(l, r);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Div => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = b.ins().sdiv(l, r);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Mod => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = b.ins().srem(l, r);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Neg => {
                    let v = b.use_var(regs[bb]);
                    let result = b.ins().ineg(v);
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
                    let cc_code = match opcode {
                        OpCode::Eq => IntCC::Equal,
                        OpCode::NotEq => IntCC::NotEqual,
                        OpCode::Lt => IntCC::SignedLessThan,
                        OpCode::Gt => IntCC::SignedGreaterThan,
                        OpCode::LtEq => IntCC::SignedLessThanOrEqual,
                        OpCode::GtEq => IntCC::SignedGreaterThanOrEqual,
                        _ => unreachable!(),
                    };
                    let cmp = b.ins().icmp(cc_code, l, r);
                    let result = b.ins().uextend(I64, cmp);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Not => {
                    let v = b.use_var(regs[bb]);
                    let zero = b.ins().iconst(I64, 0);
                    let is_zero = b.ins().icmp(IntCC::Equal, v, zero);
                    let result = b.ins().uextend(I64, is_zero);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::And => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = b.ins().band(l, r);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Or => {
                    let l = b.use_var(regs[bb]);
                    let r = b.use_var(regs[cc]);
                    let result = b.ins().bor(l, r);
                    b.def_var(regs[a], result);
                    b.ins().jump(next, &[]);
                }
                OpCode::Jump | OpCode::Loop => {
                    // VM pre-increments ip before applying sbx, so target = ip + 1 + sbx
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
                    let zero = b.ins().iconst(I64, 0);
                    let is_false = b.ins().icmp(IntCC::Equal, cond, zero);
                    let target = ((ip as i32) + 1 + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    b.ins().brif(is_false, t, &[], next, &[]);
                }
                OpCode::JumpIfTrue => {
                    let cond = b.use_var(regs[a]);
                    let zero = b.ins().iconst(I64, 0);
                    let is_true = b.ins().icmp(IntCC::NotEqual, cond, zero);
                    let target = ((ip as i32) + 1 + (sbx as i32)) as usize;
                    let t = if target < blocks.len() {
                        blocks[target]
                    } else {
                        next
                    };
                    b.ins().brif(is_true, t, &[], next, &[]);
                }
                OpCode::GetGlobal | OpCode::SetGlobal | OpCode::Closure => {
                    b.ins().jump(next, &[]);
                }
                OpCode::Call => {
                    let arg_count = bb;
                    let dst = cc;
                    let mut call_args = Vec::with_capacity(arg_count);
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
                    let zero = b.ins().iconst(I64, 0);
                    b.ins().return_(&[zero]);
                }
                _ => {
                    b.ins().jump(next, &[]);
                }
            }
        }

        b.switch_to_block(blocks[code_len]);
        let zero = b.ins().iconst(I64, 0);
        b.ins().return_(&[zero]);

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
