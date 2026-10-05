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
//! * body:  `extern "C" fn(ctx: *mut JitCtx, depth: i64, a0, ..) -> r`
//!   where Int/Bool arguments and results are `i64` and Float ones `f64`;
//! * entry: `extern "C" fn(ctx: *mut JitCtx, args: *const i64) -> i64`
//!   where a Float argument or result travels as its bit pattern.
//!
//! Int values are raw `i64`; Bool values are `0`/`1`. On deopt the code
//! stores `1` into `ctx.status` and returns `0`; callers of a body (self-
//! calls and calls of other compiled functions) check `ctx.status` after
//! every call and unwind immediately.
//!
//! ## Registers
//!
//! Each VM register is two Cranelift variables, one `i64` and one `f64`.
//! The verifier's state says which one holds the register's value at every
//! program point; the other is stale and never read.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, InstBuilder, MemFlags, Type, UserFuncName, Value,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{FuncId, Linkage, Module};

use crate::vm::bytecode::*;
use crate::vm::jit::tier::{CTX_CANCEL_OFFSET, CTX_MAX_DEPTH_OFFSET, CTX_STATUS_OFFSET};
use crate::vm::jit::types::{FnId, JitType, TypeSig};
use crate::vm::jit::verifier::{CallInfo, MathFn, OpInfo, PureOp, RegState, VerifiedFn};

/// Native machine type of a value kind.
fn clif_type(t: JitType) -> Type {
    match t {
        JitType::Float => F64,
        JitType::Int | JitType::Bool => I64,
    }
}

/// The body signature of a specialization for `sig` returning `ret`.
pub fn body_signature<M: Module>(
    module: &M,
    sig: &TypeSig,
    ret: JitType,
) -> cranelift_codegen::ir::Signature {
    let mut s = module.make_signature();
    s.params.push(AbiParam::new(I64)); // ctx
    s.params.push(AbiParam::new(I64)); // depth
    for t in &sig.0 {
        s.params.push(AbiParam::new(clif_type(*t)));
    }
    s.returns.push(AbiParam::new(clif_type(ret)));
    s
}

/// Body functions of already-compiled specializations, by identity.
pub type CalleeBodies = HashMap<(FnId, TypeSig), FuncId>;

/// The ids of one compiled specialization.
pub struct Built {
    /// `extern "C" fn(*mut JitCtx, *const i64) -> i64`.
    pub entry: FuncId,
    /// Directly callable from other compiled bodies (see [`body_signature`]).
    pub body: FuncId,
}

/// Emit the body and entry trampoline for `vf` and return their ids.
/// `symbol` must be unique within `module`; `callees` must contain every
/// callee `vf` calls.
pub fn build_function<M: Module>(
    module: &mut M,
    chunk: &Chunk,
    vf: &VerifiedFn,
    symbol: &str,
    callees: &CalleeBodies,
) -> Result<Built, String> {
    let body_sig = body_signature(module, &vf.sig, vf.ret);
    let body = module
        .declare_function(&format!("{}_body", symbol), Linkage::Local, &body_sig)
        .map_err(|e| format!("declare error: {}", e))?;
    build_body(module, chunk, vf, body, &body_sig, callees)?;

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
        let mut call_args = Vec::with_capacity(vf.sig.arity() + 2);
        call_args.push(ctx_ptr);
        call_args.push(b.ins().iconst(I64, 0));
        for (i, t) in vf.sig.0.iter().enumerate() {
            let v = b
                .ins()
                .load(clif_type(*t), MemFlags::trusted(), args_ptr, (i * 8) as i32);
            call_args.push(v);
        }
        let call = b.ins().call(body_ref, &call_args);
        let mut r = b.inst_results(call)[0];
        if vf.ret == JitType::Float {
            r = b.ins().bitcast(I64, MemFlags::new(), r);
        }
        b.ins().return_(&[r]);
        b.finalize();
    }
    module
        .define_function(entry, &mut ctx)
        .map_err(|e| format!("define error: {}", e))?;

    Ok(Built { entry, body })
}

/// Pure math bridges (`vm::jit::math_bridges`) imported on demand.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Bridge {
    Fmod,
    Round,
    Sin,
    Cos,
    Tan,
    Ln,
    Powf,
    Fmin,
    Fmax,
    IntPow,
}

impl Bridge {
    fn symbol(self) -> &'static str {
        match self {
            Bridge::Fmod => "forge_rt_fmod",
            Bridge::Round => "forge_rt_round",
            Bridge::Sin => "forge_rt_sin",
            Bridge::Cos => "forge_rt_cos",
            Bridge::Tan => "forge_rt_tan",
            Bridge::Ln => "forge_rt_ln",
            Bridge::Powf => "forge_rt_powf",
            Bridge::Fmin => "forge_rt_fmin",
            Bridge::Fmax => "forge_rt_fmax",
            Bridge::IntPow => "forge_rt_int_pow",
        }
    }

    fn signature<M: Module>(self, module: &M) -> cranelift_codegen::ir::Signature {
        let mut s = module.make_signature();
        match self {
            Bridge::Round | Bridge::Sin | Bridge::Cos | Bridge::Tan | Bridge::Ln => {
                s.params.push(AbiParam::new(F64));
                s.returns.push(AbiParam::new(F64));
            }
            Bridge::Fmod | Bridge::Powf | Bridge::Fmin | Bridge::Fmax => {
                s.params.push(AbiParam::new(F64));
                s.params.push(AbiParam::new(F64));
                s.returns.push(AbiParam::new(F64));
            }
            Bridge::IntPow => {
                s.params.push(AbiParam::new(I64)); // ctx
                s.params.push(AbiParam::new(I64));
                s.params.push(AbiParam::new(I64));
                s.returns.push(AbiParam::new(I64));
            }
        }
        s
    }
}

/// Function references a body imports, created on first use.
struct Imports {
    bridges: HashMap<Bridge, FuncRef>,
    bodies: HashMap<FuncId, FuncRef>,
}

impl Imports {
    fn bridge<M: Module>(
        &mut self,
        module: &mut M,
        b: &mut FunctionBuilder,
        which: Bridge,
    ) -> Result<FuncRef, String> {
        if let Some(r) = self.bridges.get(&which) {
            return Ok(*r);
        }
        let id = module
            .declare_function(which.symbol(), Linkage::Import, &which.signature(module))
            .map_err(|e| format!("declare error: {}", e))?;
        let r = module.declare_func_in_func(id, b.func);
        self.bridges.insert(which, r);
        Ok(r)
    }

    fn body<M: Module>(&mut self, module: &mut M, b: &mut FunctionBuilder, id: FuncId) -> FuncRef {
        *self
            .bodies
            .entry(id)
            .or_insert_with(|| module.declare_func_in_func(id, b.func))
    }
}

/// Native variables of the VM registers.
struct Regs {
    int: Vec<Variable>,
    float: Vec<Variable>,
}

impl Regs {
    fn get(&self, b: &mut FunctionBuilder, reg: usize, t: JitType) -> Value {
        match t {
            JitType::Float => b.use_var(self.float[reg]),
            JitType::Int | JitType::Bool => b.use_var(self.int[reg]),
        }
    }

    fn set(&self, b: &mut FunctionBuilder, reg: usize, t: JitType, v: Value) {
        match t {
            JitType::Float => b.def_var(self.float[reg], v),
            JitType::Int | JitType::Bool => b.def_var(self.int[reg], v),
        }
    }

    /// The register as an `f64` (`as f64` for an Int, like the VM).
    fn as_f64(&self, b: &mut FunctionBuilder, reg: usize, t: JitType) -> Value {
        match t {
            JitType::Float => b.use_var(self.float[reg]),
            JitType::Int | JitType::Bool => {
                let v = b.use_var(self.int[reg]);
                b.ins().fcvt_from_sint(F64, v)
            }
        }
    }

    /// Truthiness (`semantics::is_truthy`) as an `i8` condition.
    fn truthy(&self, b: &mut FunctionBuilder, reg: usize, t: JitType) -> Value {
        match t {
            JitType::Float => {
                let v = b.use_var(self.float[reg]);
                let zero = b.ins().f64const(0.0);
                // Unordered-or-not-equal: NaN is truthy, like `f != 0.0`.
                b.ins().fcmp(FloatCC::NotEqual, v, zero)
            }
            JitType::Int | JitType::Bool => {
                let v = b.use_var(self.int[reg]);
                b.ins().icmp_imm(IntCC::NotEqual, v, 0)
            }
        }
    }
}

/// The value kind the verifier proved for `reg` at this point.
fn kind(state: &[RegState], reg: usize, ip: usize) -> Result<JitType, String> {
    match state.get(reg) {
        Some(RegState::Val(t)) => Ok(*t),
        other => Err(format!(
            "BUG: register r{} at ip {} is {:?}, not a verified value",
            reg, ip, other
        )),
    }
}

fn build_body<M: Module>(
    module: &mut M,
    chunk: &Chunk,
    vf: &VerifiedFn,
    body: FuncId,
    sig: &cranelift_codegen::ir::Signature,
    callees: &CalleeBodies,
) -> Result<(), String> {
    let mut ctx = module.make_context();
    ctx.func.signature = sig.clone();
    ctx.func.name = UserFuncName::user(0, body.as_u32());
    let mut fbc = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fbc);
        let self_ref = module.declare_func_in_func(body, b.func);
        let mut imports = Imports {
            bridges: HashMap::new(),
            bodies: HashMap::new(),
        };
        let flags = MemFlags::trusted();

        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let ctx_ptr = b.block_params(entry)[0];
        let depth = b.block_params(entry)[1];

        let regs = Regs {
            int: (0..vf.num_regs).map(|_| b.declare_var(I64)).collect(),
            float: (0..vf.num_regs).map(|_| b.declare_var(F64)).collect(),
        };
        // Never read before written (verifier), but every variable needs a
        // definition on every path for SSA construction.
        let zero_i = b.ins().iconst(I64, 0);
        let zero_f = b.ins().f64const(0.0);
        for i in 0..vf.num_regs {
            b.def_var(regs.int[i], zero_i);
            b.def_var(regs.float[i], zero_f);
        }
        for (i, t) in vf.sig.0.iter().enumerate() {
            let p = b.block_params(entry)[i + 2];
            regs.set(&mut b, i, *t, p);
        }

        let code_len = chunk.code.len();
        let blocks: Vec<Block> = (0..code_len).map(|_| b.create_block()).collect();

        // Shared exits.
        let deopt = b.create_block();
        let unwind = b.create_block();

        // Stack budget: the VM refuses a call beyond the shared recursion limit
        // (runtime/recursion.rs); native code deopts before exceeding it.
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
        // Continue only if `ctx.status` is still 0 (after a call that may
        // have deopted).
        fn status_check(b: &mut FunctionBuilder, ctx_ptr: Value, unwind: Block) {
            let status = b
                .ins()
                .load(I64, MemFlags::trusted(), ctx_ptr, CTX_STATUS_OFFSET);
            let failed = b.ins().icmp_imm(IntCC::NotEqual, status, 0);
            let cont = b.create_block();
            b.ins().brif(failed, unwind, &[], cont, &[]);
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
        // Int overflow-checked add/sub/mul; deopts where the VM would
        // promote to Float.
        fn checked_int(
            b: &mut FunctionBuilder,
            op: OpCode,
            x: Value,
            y: Value,
            deopt: Block,
        ) -> Value {
            let (r, overflow) = match op {
                OpCode::Add | OpCode::AddLocal => {
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
            guard(b, overflow, deopt);
            r
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
            let k = |reg: usize| kind(state, reg, ip);

            match opcode {
                OpCode::LoadConst => {
                    match chunk.constants.get(bx) {
                        Some(Constant::Int(n)) => {
                            let v = b.ins().iconst(I64, *n);
                            regs.set(&mut b, a, JitType::Int, v);
                        }
                        Some(Constant::Bool(v)) => {
                            let v = b.ins().iconst(I64, i64::from(*v));
                            regs.set(&mut b, a, JitType::Bool, v);
                        }
                        Some(Constant::Float(f)) => {
                            let v = b.ins().f64const(*f);
                            regs.set(&mut b, a, JitType::Float, v);
                        }
                        // A method name: compile-time only (`RegState::Str`).
                        Some(Constant::Str(_)) => {}
                        _ => return Err(format!("BUG: unverified constant at ip {}", ip)),
                    }
                    fall(&mut b)?;
                }
                OpCode::LoadTrue | OpCode::LoadFalse => {
                    let v = b.ins().iconst(I64, i64::from(opcode == OpCode::LoadTrue));
                    regs.set(&mut b, a, JitType::Bool, v);
                    fall(&mut b)?;
                }
                OpCode::Move | OpCode::GetLocal | OpCode::SetLocal => {
                    // References, method names and the range-loop mode are
                    // compile-time facts with no native value.
                    if let Some(RegState::Val(t)) = state.get(rb) {
                        let v = regs.get(&mut b, rb, *t);
                        regs.set(&mut b, a, *t, v);
                    }
                    fall(&mut b)?;
                }
                OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::AddLocal => {
                    // `AddLocal` is `Add` with A as both destination and
                    // left operand (registers are plain variables here).
                    let (xr, yr) = if opcode == OpCode::AddLocal {
                        (a, rb)
                    } else {
                        (rb, rc)
                    };
                    let (xt, yt) = (k(xr)?, k(yr)?);
                    if xt == JitType::Int && yt == JitType::Int {
                        let x = regs.get(&mut b, xr, JitType::Int);
                        let y = regs.get(&mut b, yr, JitType::Int);
                        let r = checked_int(&mut b, opcode, x, y, deopt);
                        regs.set(&mut b, a, JitType::Int, r);
                    } else {
                        let x = regs.as_f64(&mut b, xr, xt);
                        let y = regs.as_f64(&mut b, yr, yt);
                        let r = match opcode {
                            OpCode::Add | OpCode::AddLocal => b.ins().fadd(x, y),
                            OpCode::Sub => b.ins().fsub(x, y),
                            _ => b.ins().fmul(x, y),
                        };
                        regs.set(&mut b, a, JitType::Float, r);
                    }
                    fall(&mut b)?;
                }
                OpCode::Div | OpCode::Mod => {
                    let (xt, yt) = (k(rb)?, k(rc)?);
                    if xt == JitType::Int && yt == JitType::Int {
                        let x = regs.get(&mut b, rb, JitType::Int);
                        let y = regs.get(&mut b, rc, JitType::Int);
                        // Divisor 0: VM error. MIN / -1: VM promotes to
                        // Float (Div) or yields 0 (Mod). Re-run in the VM.
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
                        regs.set(&mut b, a, JitType::Int, r);
                    } else {
                        let x = regs.as_f64(&mut b, rb, xt);
                        let y = regs.as_f64(&mut b, rc, yt);
                        let r = if opcode == OpCode::Div {
                            b.ins().fdiv(x, y)
                        } else {
                            let f = imports.bridge(module, &mut b, Bridge::Fmod)?;
                            let call = b.ins().call(f, &[x, y]);
                            b.inst_results(call)[0]
                        };
                        regs.set(&mut b, a, JitType::Float, r);
                    }
                    fall(&mut b)?;
                }
                OpCode::Neg => {
                    match k(rb)? {
                        JitType::Float => {
                            let x = regs.get(&mut b, rb, JitType::Float);
                            let r = b.ins().fneg(x);
                            regs.set(&mut b, a, JitType::Float, r);
                        }
                        _ => {
                            let x = regs.get(&mut b, rb, JitType::Int);
                            let is_min = b.ins().icmp_imm(IntCC::Equal, x, i64::MIN);
                            guard(&mut b, is_min, deopt);
                            let r = b.ins().ineg(x);
                            regs.set(&mut b, a, JitType::Int, r);
                        }
                    }
                    fall(&mut b)?;
                }
                OpCode::Lt | OpCode::Gt | OpCode::LtEq | OpCode::GtEq => {
                    let (xt, yt) = (k(rb)?, k(rc)?);
                    let c = if xt == JitType::Int && yt == JitType::Int {
                        let cc = match opcode {
                            OpCode::Lt => IntCC::SignedLessThan,
                            OpCode::Gt => IntCC::SignedGreaterThan,
                            OpCode::LtEq => IntCC::SignedLessThanOrEqual,
                            _ => IntCC::SignedGreaterThanOrEqual,
                        };
                        let x = regs.get(&mut b, rb, JitType::Int);
                        let y = regs.get(&mut b, rc, JitType::Int);
                        b.ins().icmp(cc, x, y)
                    } else {
                        // Ordered comparisons: false when either is NaN,
                        // like `PartialOrd` on f64.
                        let cc = match opcode {
                            OpCode::Lt => FloatCC::LessThan,
                            OpCode::Gt => FloatCC::GreaterThan,
                            OpCode::LtEq => FloatCC::LessThanOrEqual,
                            _ => FloatCC::GreaterThanOrEqual,
                        };
                        let x = regs.as_f64(&mut b, rb, xt);
                        let y = regs.as_f64(&mut b, rc, yt);
                        b.ins().fcmp(cc, x, y)
                    };
                    let v = to_i64(&mut b, c);
                    regs.set(&mut b, a, JitType::Bool, v);
                    fall(&mut b)?;
                }
                OpCode::Eq | OpCode::NotEq => {
                    let eq = opcode == OpCode::Eq;
                    let (xt, yt) = (k(rb)?, k(rc)?);
                    let v = if xt == yt && xt != JitType::Float {
                        // Int == Int, Bool == Bool (normalized 0/1).
                        let cc = if eq { IntCC::Equal } else { IntCC::NotEqual };
                        let x = regs.get(&mut b, rb, xt);
                        let y = regs.get(&mut b, rc, yt);
                        let c = b.ins().icmp(cc, x, y);
                        to_i64(&mut b, c)
                    } else if xt.is_numeric() && yt.is_numeric() {
                        // IEEE equality; `!=` is its negation (true for NaN).
                        let cc = if eq {
                            FloatCC::Equal
                        } else {
                            FloatCC::NotEqual
                        };
                        let x = regs.as_f64(&mut b, rb, xt);
                        let y = regs.as_f64(&mut b, rc, yt);
                        let c = b.ins().fcmp(cc, x, y);
                        to_i64(&mut b, c)
                    } else {
                        // Bool vs number: never equal.
                        b.ins().iconst(I64, i64::from(!eq))
                    };
                    regs.set(&mut b, a, JitType::Bool, v);
                    fall(&mut b)?;
                }
                OpCode::Not => {
                    let t = regs.truthy(&mut b, rb, k(rb)?);
                    let one = b.ins().iconst(I8, 1);
                    let c = b.ins().bxor(t, one);
                    let v = to_i64(&mut b, c);
                    regs.set(&mut b, a, JitType::Bool, v);
                    fall(&mut b)?;
                }
                OpCode::And | OpCode::Or => {
                    let xt = regs.truthy(&mut b, rb, k(rb)?);
                    let yt = regs.truthy(&mut b, rc, k(rc)?);
                    let c = if opcode == OpCode::And {
                        b.ins().band(xt, yt)
                    } else {
                        b.ins().bor(xt, yt)
                    };
                    let v = to_i64(&mut b, c);
                    regs.set(&mut b, a, JitType::Bool, v);
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
                    let n = next.ok_or_else(|| "BUG: verifier allowed fall-off".to_string())?;
                    if state.get(a) == Some(&RegState::RangeFast) {
                        // The counting-loop mode is statically true.
                        if opcode == OpCode::JumpIfTrue {
                            b.ins().jump(jump_target(), &[]);
                        } else {
                            b.ins().jump(n, &[]);
                        }
                    } else {
                        // `brif` tests an integer for non-zero, which is
                        // exactly Int/Bool truthiness; only a Float needs
                        // a compare.
                        let cond = match k(a)? {
                            JitType::Float => regs.truthy(&mut b, a, JitType::Float),
                            t => regs.get(&mut b, a, t),
                        };
                        if opcode == OpCode::JumpIfFalse {
                            b.ins().brif(cond, n, &[], jump_target(), &[]);
                        } else {
                            b.ins().brif(cond, jump_target(), &[], n, &[]);
                        }
                    }
                }
                OpCode::GetGlobal | OpCode::GetUpvalue => {
                    // A guarded reference (self, callee, builtin, module):
                    // compile-time only.
                    fall(&mut b)?;
                }
                OpCode::GetField => {
                    let Some(OpInfo::FloatConst(f)) = vf.ops[ip] else {
                        return Err(format!("BUG: unverified GetField at ip {}", ip));
                    };
                    let v = b.ins().f64const(f);
                    regs.set(&mut b, a, JitType::Float, v);
                    fall(&mut b)?;
                }
                OpCode::Call => {
                    let Some(OpInfo::Call(info)) = &vf.ops[ip] else {
                        return Err(format!("BUG: unverified call at ip {}", ip));
                    };
                    match info {
                        CallInfo::SelfCall | CallInfo::Callee { .. } => {
                            let (target, csig, ret) = match info {
                                CallInfo::Callee { id, sig, ret, .. } => {
                                    let body_id =
                                        callees.get(&(*id, sig.clone())).ok_or_else(|| {
                                            format!("BUG: callee at ip {} was not compiled", ip)
                                        })?;
                                    (imports.body(module, &mut b, *body_id), sig, *ret)
                                }
                                _ => (self_ref, &vf.sig, vf.ret),
                            };
                            cancel_check(&mut b, ctx_ptr, deopt);
                            let mut args = Vec::with_capacity(csig.arity() + 2);
                            args.push(ctx_ptr);
                            args.push(b.ins().iadd_imm(depth, 1));
                            for (i, t) in csig.0.iter().enumerate() {
                                args.push(regs.get(&mut b, a + 1 + i, *t));
                            }
                            let call = b.ins().call(target, &args);
                            let r = b.inst_results(call)[0];
                            status_check(&mut b, ctx_ptr, unwind);
                            regs.set(&mut b, rc, ret, r);
                        }
                        CallInfo::Pure {
                            op,
                            first_arg,
                            args,
                            ret,
                        } => {
                            let base = a + first_arg;
                            let r = lower_pure(
                                module,
                                &mut b,
                                &mut imports,
                                &regs,
                                *op,
                                base,
                                args,
                                ctx_ptr,
                                deopt,
                                unwind,
                            )?;
                            regs.set(&mut b, rc, *ret, r);
                        }
                    }
                    fall(&mut b)?;
                }
                OpCode::ForRangePrep => {
                    let (start, end) = if rb == 1 {
                        (
                            b.ins().iconst(I64, 0),
                            regs.get(&mut b, a + 1, JitType::Int),
                        )
                    } else {
                        (
                            regs.get(&mut b, a + 1, JitType::Int),
                            regs.get(&mut b, a + 2, JitType::Int),
                        )
                    };
                    regs.set(&mut b, a, JitType::Int, start);
                    regs.set(&mut b, a + 1, JitType::Int, end);
                    fall(&mut b)?;
                }
                OpCode::ForRangeNext => {
                    let exit = next.ok_or_else(|| "BUG: verifier allowed fall-off".to_string())?;
                    let body_block = blocks
                        .get(ip + 2)
                        .copied()
                        .ok_or_else(|| "BUG: verifier allowed fall-off".to_string())?;
                    let cur = regs.get(&mut b, a, JitType::Int);
                    let end = regs.get(&mut b, a + 1, JitType::Int);
                    let more = b.ins().icmp(IntCC::SignedLessThan, cur, end);
                    let step = b.create_block();
                    b.ins().brif(more, step, &[], exit, &[]);
                    b.switch_to_block(step);
                    regs.set(&mut b, rb, JitType::Int, cur);
                    // `cur < end <= i64::MAX`: no overflow.
                    let inc = b.ins().iadd_imm(cur, 1);
                    regs.set(&mut b, a, JitType::Int, inc);
                    b.ins().jump(body_block, &[]);
                }
                OpCode::Return => {
                    if !matches!(state.get(a), Some(RegState::Val(t)) if *t == vf.ret) {
                        return Err(format!("BUG: unverified return at ip {}", ip));
                    }
                    let v = regs.get(&mut b, a, vf.ret);
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
        let z = zero_of(&mut b, vf.ret);
        b.ins().return_(&[z]);

        // unwind: a callee already flagged the context.
        b.switch_to_block(unwind);
        let z = zero_of(&mut b, vf.ret);
        b.ins().return_(&[z]);

        b.seal_all_blocks();
        b.finalize();
    }

    module
        .define_function(body, &mut ctx)
        .map_err(|e| format!("define error: {}", e))?;
    Ok(())
}

/// The placeholder result returned along with a deopt status.
fn zero_of(b: &mut FunctionBuilder, t: JitType) -> Value {
    match t {
        JitType::Float => b.ins().f64const(0.0),
        JitType::Int | JitType::Bool => b.ins().iconst(I64, 0),
    }
}

/// Lower a verified pure builtin call; returns the result value (of the
/// verified result kind). Arguments are in registers `base..`.
#[allow(clippy::too_many_arguments)]
fn lower_pure<M: Module>(
    module: &mut M,
    b: &mut FunctionBuilder,
    imports: &mut Imports,
    regs: &Regs,
    op: PureOp,
    base: usize,
    args: &[JitType],
    ctx_ptr: Value,
    deopt: Block,
    unwind: Block,
) -> Result<Value, String> {
    fn guard(b: &mut FunctionBuilder, cond: Value, deopt: Block) {
        let cont = b.create_block();
        b.ins().brif(cond, deopt, &[], cont, &[]);
        b.switch_to_block(cont);
    }
    let all_int = args.iter().all(|t| *t == JitType::Int);
    let f64_arg = |b: &mut FunctionBuilder, i: usize| regs.as_f64(b, base + i, args[i]);
    let int_arg = |b: &mut FunctionBuilder, i: usize| regs.get(b, base + i, JitType::Int);
    let mut call =
        |b: &mut FunctionBuilder, which: Bridge, xs: &[Value]| -> Result<Value, String> {
            let f = imports.bridge(module, b, which)?;
            let inst = b.ins().call(f, xs);
            Ok(b.inst_results(inst)[0])
        };
    Ok(match op {
        PureOp::ToFloat => f64_arg(b, 0),
        PureOp::ToInt => match args[0] {
            // `n as i64`: saturating, NaN -> 0.
            JitType::Float => {
                let x = regs.get(b, base, JitType::Float);
                b.ins().fcvt_to_sint_sat(I64, x)
            }
            // Int unchanged; Bool is already 0/1.
            t => regs.get(b, base, t),
        },
        PureOp::Math(f) => match f {
            MathFn::Sqrt => {
                let x = f64_arg(b, 0);
                b.ins().sqrt(x)
            }
            MathFn::Sin | MathFn::Cos | MathFn::Tan | MathFn::Log => {
                let x = f64_arg(b, 0);
                let which = match f {
                    MathFn::Sin => Bridge::Sin,
                    MathFn::Cos => Bridge::Cos,
                    MathFn::Tan => Bridge::Tan,
                    _ => Bridge::Ln,
                };
                call(b, which, &[x])?
            }
            MathFn::Abs => {
                if args[0] == JitType::Float {
                    let x = regs.get(b, base, JitType::Float);
                    b.ins().fabs(x)
                } else {
                    // `int_abs`: |MIN| is a Float in the VM.
                    let x = int_arg(b, 0);
                    let is_min = b.ins().icmp_imm(IntCC::Equal, x, i64::MIN);
                    guard(b, is_min, deopt);
                    let neg = b.ins().ineg(x);
                    let is_neg = b.ins().icmp_imm(IntCC::SignedLessThan, x, 0);
                    b.ins().select(is_neg, neg, x)
                }
            }
            MathFn::Floor | MathFn::Ceil | MathFn::Round => {
                if args[0] == JitType::Int {
                    int_arg(b, 0)
                } else {
                    let x = regs.get(b, base, JitType::Float);
                    let r = match f {
                        MathFn::Floor => b.ins().floor(x),
                        MathFn::Ceil => b.ins().ceil(x),
                        // Half away from zero (Cranelift's `nearest` is
                        // ties-to-even).
                        _ => call(b, Bridge::Round, &[x])?,
                    };
                    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
                    let lo = b.ins().f64const(-LIMIT);
                    let hi = b.ins().f64const(LIMIT);
                    let ge = b.ins().fcmp(FloatCC::GreaterThanOrEqual, r, lo);
                    let lt = b.ins().fcmp(FloatCC::LessThan, r, hi);
                    let ok = b.ins().band(ge, lt);
                    let bad = b.ins().icmp_imm(IntCC::Equal, ok, 0);
                    guard(b, bad, deopt);
                    b.ins().fcvt_to_sint_sat(I64, r)
                }
            }
            MathFn::Pow => {
                if all_int {
                    let x = int_arg(b, 0);
                    let y = int_arg(b, 1);
                    let r = call(b, Bridge::IntPow, &[ctx_ptr, x, y])?;
                    // The bridge flags a deopt itself.
                    let status = b
                        .ins()
                        .load(I64, MemFlags::trusted(), ctx_ptr, CTX_STATUS_OFFSET);
                    let failed = b.ins().icmp_imm(IntCC::NotEqual, status, 0);
                    guard(b, failed, unwind);
                    r
                } else {
                    let x = f64_arg(b, 0);
                    let y = f64_arg(b, 1);
                    call(b, Bridge::Powf, &[x, y])?
                }
            }
            MathFn::Min | MathFn::Max => {
                let is_max = f == MathFn::Max;
                if all_int {
                    let x = int_arg(b, 0);
                    let y = int_arg(b, 1);
                    if is_max {
                        b.ins().smax(x, y)
                    } else {
                        b.ins().smin(x, y)
                    }
                } else {
                    let x = f64_arg(b, 0);
                    let y = f64_arg(b, 1);
                    let which = if is_max { Bridge::Fmax } else { Bridge::Fmin };
                    call(b, which, &[x, y])?
                }
            }
            MathFn::Clamp => {
                // `v.max(lo).min(hi)` in both kinds.
                if all_int {
                    let v = int_arg(b, 0);
                    let lo = int_arg(b, 1);
                    let hi = int_arg(b, 2);
                    let m = b.ins().smax(v, lo);
                    b.ins().smin(m, hi)
                } else {
                    let v = f64_arg(b, 0);
                    let lo = f64_arg(b, 1);
                    let hi = f64_arg(b, 2);
                    let m = call(b, Bridge::Fmax, &[v, lo])?;
                    call(b, Bridge::Fmin, &[m, hi])?
                }
            }
        },
    })
}
