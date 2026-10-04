//! Lowers a [`VerifiedFn`] to Cranelift IR.
//!
//! Every opcode the verifier accepts is lowered here with its exact VM
//! semantics; every situation the native code does not implement branches to
//! a deopt block (see the module docs of `vm::jit`). There are deliberately
//! no fallback arms: an opcode reaching the `unreachable` arm means the
//! verifier and builder disagree, which is reported as a compile error and
//! the function stays in the VM.
//!
//! ## Native ABI
//!
//! * body:  `extern "C" fn(ctx: *mut JitCtx, depth: i64, a0: i64, ..) -> i64`
//! * entry: `extern "C" fn(ctx: *mut JitCtx, args: *const i64) -> i64`
//!
//! Int values are raw `i64`; Bool values are `0`/`1`. On deopt the code
//! stores `1` into `ctx.status` and returns `0`; callers of the body check
//! `ctx.status` after every self-call and unwind immediately.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{AbiParam, Block, InstBuilder, MemFlags, UserFuncName, Value};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{FuncId, Linkage, Module};

use crate::vm::bytecode::*;
use crate::vm::jit::tier::{CTX_CANCEL_OFFSET, CTX_MAX_DEPTH_OFFSET, CTX_STATUS_OFFSET};
use crate::vm::jit::verifier::{RegState, VerifiedFn};

fn body_signature<M: Module>(module: &M, arity: usize) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(I64)); // ctx
    sig.params.push(AbiParam::new(I64)); // depth
    for _ in 0..arity {
        sig.params.push(AbiParam::new(I64));
    }
    sig.returns.push(AbiParam::new(I64));
    sig
}

/// Emit the body and entry trampoline for `vf` and return the entry's id.
/// `symbol` must be unique within `module`.
pub fn build_function<M: Module>(
    module: &mut M,
    chunk: &Chunk,
    vf: &VerifiedFn,
    symbol: &str,
) -> Result<FuncId, String> {
    let arity = vf.sig.arity();
    let body_sig = body_signature(module, arity);
    let body = module
        .declare_function(&format!("{}_body", symbol), Linkage::Local, &body_sig)
        .map_err(|e| format!("declare error: {}", e))?;
    build_body(module, chunk, vf, body, &body_sig)?;

    let mut entry_sig = module.make_signature();
    entry_sig.params.push(AbiParam::new(I64)); // ctx
    entry_sig.params.push(AbiParam::new(I64)); // args ptr
    entry_sig.returns.push(AbiParam::new(I64));
    let entry = module
        .declare_function(&format!("{}_entry", symbol), Linkage::Local, &entry_sig)
        .map_err(|e| format!("declare error: {}", e))?;

    let mut ctx = module.make_context();
    ctx.func.signature = entry_sig;
    ctx.func.name = UserFuncName::user(0, entry.as_u32());
    let mut fbc = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fbc);
        let block = b.create_block();
        b.append_block_params_for_function_params(block);
        b.switch_to_block(block);
        b.seal_block(block);
        let ctx_ptr = b.block_params(block)[0];
        let args_ptr = b.block_params(block)[1];
        let body_ref = module.declare_func_in_func(body, b.func);
        let mut call_args = Vec::with_capacity(arity + 2);
        call_args.push(ctx_ptr);
        call_args.push(b.ins().iconst(I64, 0));
        for i in 0..arity {
            let v = b
                .ins()
                .load(I64, MemFlags::trusted(), args_ptr, (i * 8) as i32);
            call_args.push(v);
        }
        let call = b.ins().call(body_ref, &call_args);
        let r = b.inst_results(call)[0];
        b.ins().return_(&[r]);
        b.finalize();
    }
    module
        .define_function(entry, &mut ctx)
        .map_err(|e| format!("define error: {}", e))?;

    Ok(entry)
}

fn build_body<M: Module>(
    module: &mut M,
    chunk: &Chunk,
    vf: &VerifiedFn,
    body: FuncId,
    sig: &cranelift_codegen::ir::Signature,
) -> Result<(), String> {
    let arity = vf.sig.arity();
    let mut ctx = module.make_context();
    ctx.func.signature = sig.clone();
    ctx.func.name = UserFuncName::user(0, body.as_u32());
    let mut fbc = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fbc);
        let self_ref = module.declare_func_in_func(body, b.func);
        let flags = MemFlags::trusted();

        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let ctx_ptr = b.block_params(entry)[0];
        let depth = b.block_params(entry)[1];

        let regs: Vec<Variable> = (0..vf.num_regs).map(|_| b.declare_var(I64)).collect();
        for (i, reg) in regs.iter().enumerate() {
            let v = if i < arity {
                b.block_params(entry)[i + 2]
            } else {
                // Never read before written (verifier), but every variable
                // needs a definition on every path for SSA construction.
                b.ins().iconst(I64, 0)
            };
            b.def_var(*reg, v);
        }

        let code_len = chunk.code.len();
        let blocks: Vec<Block> = (0..code_len).map(|_| b.create_block()).collect();

        // Shared exits.
        let deopt = b.create_block();
        let unwind = b.create_block();

        // Stack budget: the VM refuses a call once it holds MAX_FRAMES frames.
        let max_depth = b.ins().load(I64, flags, ctx_ptr, CTX_MAX_DEPTH_OFFSET);
        let too_deep = b.ins().icmp(IntCC::SignedGreaterThan, depth, max_depth);
        b.ins().brif(too_deep, deopt, &[], blocks[0], &[]);

        // Branch to `deopt` when `cond` holds, otherwise continue in a fresh
        // block which becomes current.
        fn guard(b: &mut FunctionBuilder, cond: Value, deopt: Block) {
            let cont = b.create_block();
            b.ins().brif(cond, deopt, &[], cont, &[]);
            b.switch_to_block(cont);
        }
        // Deopt if the VM's cancellation flag is set (checked where the VM
        // checks it: before `Loop` and `Call`).
        fn cancel_check(b: &mut FunctionBuilder, ctx_ptr: Value, deopt: Block) {
            let flags = MemFlags::trusted();
            let flag_ptr = b.ins().load(I64, flags, ctx_ptr, CTX_CANCEL_OFFSET);
            let flag = b.ins().load(I8, flags, flag_ptr, 0);
            let set = b.ins().icmp_imm(IntCC::NotEqual, flag, 0);
            guard(b, set, deopt);
        }
        fn to_i64(b: &mut FunctionBuilder, cmp: Value) -> Value {
            b.ins().uextend(I64, cmp)
        }

        for (ip, &inst) in chunk.code.iter().enumerate() {
            b.switch_to_block(blocks[ip]);
            let Some(state) = vf.states[ip].as_ref() else {
                // Unreachable per the verifier; never executed.
                b.ins().jump(deopt, &[]);
                continue;
            };
            let opcode = OpCode::try_from(decode_op(inst))
                .map_err(|bad| format!("BUG: verifier accepted invalid opcode {}", bad))?;
            let a = decode_a(inst) as usize;
            let rb = decode_b(inst) as usize;
            let rc = decode_c(inst) as usize;
            let bx = decode_bx(inst) as usize;
            let sbx = decode_sbx(inst) as i64;
            let next = blocks.get(ip + 1).copied();
            let jump_target = || blocks[(ip as i64 + 1 + sbx) as usize];
            let fall = |b: &mut FunctionBuilder| -> Result<(), String> {
                let n = next.ok_or_else(|| "BUG: verifier allowed fall-off".to_string())?;
                b.ins().jump(n, &[]);
                Ok(())
            };

            match opcode {
                OpCode::LoadConst => {
                    let v = match chunk.constants.get(bx) {
                        Some(Constant::Int(n)) => b.ins().iconst(I64, *n),
                        Some(Constant::Bool(v)) => b.ins().iconst(I64, i64::from(*v)),
                        _ => return Err(format!("BUG: unverified constant at ip {}", ip)),
                    };
                    b.def_var(regs[a], v);
                    fall(&mut b)?;
                }
                OpCode::LoadTrue | OpCode::LoadFalse => {
                    let v = b.ins().iconst(I64, i64::from(opcode == OpCode::LoadTrue));
                    b.def_var(regs[a], v);
                    fall(&mut b)?;
                }
                OpCode::Move | OpCode::GetLocal | OpCode::SetLocal => {
                    let v = b.use_var(regs[rb]);
                    b.def_var(regs[a], v);
                    fall(&mut b)?;
                }
                OpCode::Add | OpCode::Sub | OpCode::Mul => {
                    let x = b.use_var(regs[rb]);
                    let y = b.use_var(regs[rc]);
                    // Wrapping op + exact signed-overflow test; on overflow
                    // the VM would produce a Float, so deopt.
                    let (r, overflow) = match opcode {
                        OpCode::Add => {
                            let r = b.ins().iadd(x, y);
                            let xr = b.ins().bxor(x, r);
                            let yr = b.ins().bxor(y, r);
                            let t = b.ins().band(xr, yr);
                            (r, b.ins().icmp_imm(IntCC::SignedLessThan, t, 0))
                        }
                        OpCode::Sub => {
                            let r = b.ins().isub(x, y);
                            let xy = b.ins().bxor(x, y);
                            let xr = b.ins().bxor(x, r);
                            let t = b.ins().band(xy, xr);
                            (r, b.ins().icmp_imm(IntCC::SignedLessThan, t, 0))
                        }
                        _ => {
                            let r = b.ins().imul(x, y);
                            let hi = b.ins().smulhi(x, y);
                            let sign = b.ins().sshr_imm(r, 63);
                            (r, b.ins().icmp(IntCC::NotEqual, hi, sign))
                        }
                    };
                    guard(&mut b, overflow, deopt);
                    b.def_var(regs[a], r);
                    fall(&mut b)?;
                }
                OpCode::Div | OpCode::Mod => {
                    let x = b.use_var(regs[rb]);
                    let y = b.use_var(regs[rc]);
                    // Divisor 0: VM error. MIN / -1: VM (Rust) panics. Both
                    // are reproduced by re-running in the VM.
                    let zero = b.ins().icmp_imm(IntCC::Equal, y, 0);
                    guard(&mut b, zero, deopt);
                    let x_min = b.ins().icmp_imm(IntCC::Equal, x, i64::MIN);
                    let y_m1 = b.ins().icmp_imm(IntCC::Equal, y, -1);
                    let both = b.ins().band(x_min, y_m1);
                    guard(&mut b, both, deopt);
                    let r = if opcode == OpCode::Div {
                        b.ins().sdiv(x, y)
                    } else {
                        b.ins().srem(x, y)
                    };
                    b.def_var(regs[a], r);
                    fall(&mut b)?;
                }
                OpCode::Neg => {
                    let x = b.use_var(regs[rb]);
                    let is_min = b.ins().icmp_imm(IntCC::Equal, x, i64::MIN);
                    guard(&mut b, is_min, deopt);
                    let r = b.ins().ineg(x);
                    b.def_var(regs[a], r);
                    fall(&mut b)?;
                }
                OpCode::Eq
                | OpCode::NotEq
                | OpCode::Lt
                | OpCode::Gt
                | OpCode::LtEq
                | OpCode::GtEq => {
                    // Int values compare numerically; Bool values are
                    // normalized 0/1, so the same integer compare is exact.
                    let cc = match opcode {
                        OpCode::Eq => IntCC::Equal,
                        OpCode::NotEq => IntCC::NotEqual,
                        OpCode::Lt => IntCC::SignedLessThan,
                        OpCode::Gt => IntCC::SignedGreaterThan,
                        OpCode::LtEq => IntCC::SignedLessThanOrEqual,
                        _ => IntCC::SignedGreaterThanOrEqual,
                    };
                    let x = b.use_var(regs[rb]);
                    let y = b.use_var(regs[rc]);
                    let c = b.ins().icmp(cc, x, y);
                    let v = to_i64(&mut b, c);
                    b.def_var(regs[a], v);
                    fall(&mut b)?;
                }
                OpCode::Not => {
                    // Truthiness of Int is `!= 0`, of Bool is the bool.
                    let x = b.use_var(regs[rb]);
                    let c = b.ins().icmp_imm(IntCC::Equal, x, 0);
                    let v = to_i64(&mut b, c);
                    b.def_var(regs[a], v);
                    fall(&mut b)?;
                }
                OpCode::And | OpCode::Or => {
                    let x = b.use_var(regs[rb]);
                    let y = b.use_var(regs[rc]);
                    let xt = b.ins().icmp_imm(IntCC::NotEqual, x, 0);
                    let yt = b.ins().icmp_imm(IntCC::NotEqual, y, 0);
                    let c = if opcode == OpCode::And {
                        b.ins().band(xt, yt)
                    } else {
                        b.ins().bor(xt, yt)
                    };
                    let v = to_i64(&mut b, c);
                    b.def_var(regs[a], v);
                    fall(&mut b)?;
                }
                OpCode::Jump => {
                    b.ins().jump(jump_target(), &[]);
                }
                OpCode::Loop => {
                    cancel_check(&mut b, ctx_ptr, deopt);
                    b.ins().jump(jump_target(), &[]);
                }
                OpCode::JumpIfFalse | OpCode::JumpIfTrue => {
                    let cond = b.use_var(regs[a]);
                    let n = next.ok_or_else(|| "BUG: verifier allowed fall-off".to_string())?;
                    if opcode == OpCode::JumpIfFalse {
                        b.ins().brif(cond, n, &[], jump_target(), &[]);
                    } else {
                        b.ins().brif(cond, jump_target(), &[], n, &[]);
                    }
                }
                OpCode::GetGlobal => {
                    // Verified to be this function's own binding; the value is
                    // only ever used as a self-call target.
                    let v = b.ins().iconst(I64, 0);
                    b.def_var(regs[a], v);
                    fall(&mut b)?;
                }
                OpCode::Call => {
                    if state.get(a) != Some(&RegState::SelfFn) {
                        return Err(format!("BUG: unverified call at ip {}", ip));
                    }
                    cancel_check(&mut b, ctx_ptr, deopt);
                    let mut args = Vec::with_capacity(arity + 2);
                    args.push(ctx_ptr);
                    args.push(b.ins().iadd_imm(depth, 1));
                    for i in 0..rb {
                        args.push(b.use_var(regs[a + 1 + i]));
                    }
                    let call = b.ins().call(self_ref, &args);
                    let r = b.inst_results(call)[0];
                    let status = b.ins().load(I64, flags, ctx_ptr, CTX_STATUS_OFFSET);
                    let failed = b.ins().icmp_imm(IntCC::NotEqual, status, 0);
                    let cont = b.create_block();
                    b.ins().brif(failed, unwind, &[], cont, &[]);
                    b.switch_to_block(cont);
                    b.def_var(regs[rc], r);
                    fall(&mut b)?;
                }
                OpCode::Return => {
                    if !matches!(state.get(a), Some(RegState::Val(t)) if *t == vf.ret) {
                        return Err(format!("BUG: unverified return at ip {}", ip));
                    }
                    let v = b.use_var(regs[a]);
                    b.ins().return_(&[v]);
                }
                other => {
                    return Err(format!(
                        "BUG: verifier accepted {:?} at ip {} but the builder cannot lower it",
                        other, ip
                    ));
                }
            }
        }

        // deopt: flag the context and return.
        b.switch_to_block(deopt);
        let one = b.ins().iconst(I64, 1);
        b.ins().store(flags, one, ctx_ptr, CTX_STATUS_OFFSET);
        let z = b.ins().iconst(I64, 0);
        b.ins().return_(&[z]);

        // unwind: a callee already flagged the context.
        b.switch_to_block(unwind);
        let z = b.ins().iconst(I64, 0);
        b.ins().return_(&[z]);

        b.seal_all_blocks();
        b.finalize();
    }

    module
        .define_function(body, &mut ctx)
        .map_err(|e| format!("define error: {}", e))?;
    Ok(())
}
