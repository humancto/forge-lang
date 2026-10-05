//! Compilability proof for a function under a given [`TypeSig`].
//!
//! The verifier is the single gatekeeper of the JIT. It runs a
//! flow-sensitive abstract interpretation over the bytecode CFG, tracking the
//! [`RegState`] of every register before every reachable instruction, and
//! checks each reachable instruction against an explicit allowlist with
//! exact operand-type requirements. Anything not on the allowlist, any read
//! of a register that is not definitely initialized with a supported kind,
//! any call of something other than itself, a verified pure function or a
//! known pure builtin, any control flow leaving the code, and any
//! inconsistent return kind is a [`Reject`] carrying a structured reason.
//!
//! The resulting [`VerifiedFn`] is everything the IR builder needs; the
//! builder has no "unsupported → skip" arms.
//!
//! ## Globals and guards
//!
//! A function may read a global only to *call* it (or, for `math`, to read a
//! Float constant member). Every such read is resolved against the VM's
//! globals *at verification time* through [`Env`] and recorded as a
//! [`Guard`]; the tier re-checks every guard (transitively, including the
//! callees') before each native entry. Native code cannot run Forge code or
//! assign globals, so a guard that holds at entry holds for the whole native
//! call. A global whose binding changed fails its guard and the call runs in
//! the VM.
//!
//! ## Value semantics (must match `machine.rs` / `semantics` exactly)
//!
//! Numbers follow `semantics::binary`: Int op Int stays Int (overflow,
//! `MIN / -1`, division by zero → deopt, the VM then promotes or errors);
//! any Float operand makes the operation Float (no errors: `x / 0.0` is
//! inf/NaN), Ints are converted with `as f64`. Float `%` is Rust's `%`.
//!
//! | opcode | accepted operands | result | deopt when |
//! |---|---|---|---|
//! | `LoadConst` | Int/Bool/Float constant; string (only as a method name) | same | — |
//! | `LoadTrue`/`LoadFalse` | — | Bool | — |
//! | `Move`/`GetLocal`/`SetLocal` | any defined | copy | — |
//! | `Add`/`Sub`/`Mul`/`AddLocal` | numbers | Int or Float | Int overflow |
//! | `Div`/`Mod` | numbers | Int or Float | Int: divisor 0, `MIN / -1` |
//! | `Neg` | number | same | Int `MIN` |
//! | `Lt`/`Gt`/`LtEq`/`GtEq` | numbers | Bool | — |
//! | `Eq`/`NotEq` | any two values | Bool | — |
//! | `Not`/`And`/`Or` | any value (truthiness) | Bool | — |
//! | `Jump`/`Loop`/`JumpIf*` | cond any value | — | `Loop`: task cancelled |
//! | `GetGlobal` | self, a verified function, `float`/`int`/`range`/`math`, the method-call intrinsic | reference | — (guards) |
//! | `GetField` | `math` Float constant | Float | — (guard) |
//! | `Call` | self / callee with `arity` args (callee specialized for their kinds), pure builtin | its result | stack depth, cancelled, builtin-specific |
//! | `ForRangePrep`/`ForRangeNext` | `range` with Int args | Int counters | — |
//! | `Return` | a value, same kind on every path | — | — |

use std::fmt;
use std::sync::Arc;

use crate::vm::bytecode::*;
use crate::vm::jit::types::{FnId, JitType, TypeSig};

/// A builtin global the JIT implements natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    /// The compiler intrinsic behind `obj.method(args)`.
    CallMethod,
    /// `float(x)`.
    Float,
    /// `int(x)`.
    Int,
    /// `range(..)`, only as the subject of a counting `for` loop.
    Range,
}

impl Builtin {
    /// The global name (and `NativeFunction` name) of the builtin.
    pub fn name(self) -> &'static str {
        match self {
            Builtin::CallMethod => "__forge_call_method",
            Builtin::Float => "float",
            Builtin::Int => "int",
            Builtin::Range => "range",
        }
    }

    fn from_global(name: &str) -> Option<Builtin> {
        [
            Builtin::CallMethod,
            Builtin::Float,
            Builtin::Int,
            Builtin::Range,
        ]
        .into_iter()
        .find(|b| b.name() == name)
    }
}

/// The module whose members the JIT implements.
pub const MATH_MODULE: &str = "math";

/// A pure `math` member function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MathFn {
    Sqrt,
    Abs,
    Floor,
    Ceil,
    Round,
    Sin,
    Cos,
    Tan,
    Log,
    Pow,
    Min,
    Max,
    Clamp,
}

impl MathFn {
    const ALL: [MathFn; 13] = [
        MathFn::Sqrt,
        MathFn::Abs,
        MathFn::Floor,
        MathFn::Ceil,
        MathFn::Round,
        MathFn::Sin,
        MathFn::Cos,
        MathFn::Tan,
        MathFn::Log,
        MathFn::Pow,
        MathFn::Min,
        MathFn::Max,
        MathFn::Clamp,
    ];

    /// Member name in the `math` module.
    pub fn member(self) -> &'static str {
        match self {
            MathFn::Sqrt => "sqrt",
            MathFn::Abs => "abs",
            MathFn::Floor => "floor",
            MathFn::Ceil => "ceil",
            MathFn::Round => "round",
            MathFn::Sin => "sin",
            MathFn::Cos => "cos",
            MathFn::Tan => "tan",
            MathFn::Log => "log",
            MathFn::Pow => "pow",
            MathFn::Min => "min",
            MathFn::Max => "max",
            MathFn::Clamp => "clamp",
        }
    }

    fn from_member(name: &str) -> Option<MathFn> {
        MathFn::ALL.into_iter().find(|f| f.member() == name)
    }

    /// Result kind for these argument kinds (exactly `stdlib::math::
    /// call_vm`), or `None` when the JIT does not implement the call. Only
    /// the canonical argument count is accepted.
    fn result(self, args: &[JitType]) -> Option<JitType> {
        use JitType::{Float, Int};
        let numeric = args.iter().all(|t| t.is_numeric());
        let all_int = args.iter().all(|t| *t == Int);
        let arity = match self {
            MathFn::Pow | MathFn::Min | MathFn::Max => 2,
            MathFn::Clamp => 3,
            _ => 1,
        };
        if args.len() != arity || !numeric {
            return None;
        }
        Some(match self {
            MathFn::Sqrt | MathFn::Sin | MathFn::Cos | MathFn::Tan | MathFn::Log => Float,
            // Int: `int_abs` (MIN deopts). Float: `abs`.
            MathFn::Abs => args[0],
            // Float input: an Int when representable (otherwise deopt).
            MathFn::Floor | MathFn::Ceil | MathFn::Round => Int,
            // Int, Int: `int_pow` (negative exponent / overflow deopt).
            MathFn::Pow | MathFn::Min | MathFn::Max | MathFn::Clamp => {
                if all_int {
                    Int
                } else {
                    Float
                }
            }
        })
    }
}

/// A call the JIT performs without leaving native code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PureOp {
    Math(MathFn),
    /// `float(x)`
    ToFloat,
    /// `int(x)`
    ToInt,
}

/// What a global name is bound to right now (see [`Env::global`]).
#[derive(Debug, Clone)]
pub enum GlobalView {
    /// A closure over this function prototype.
    Closure(Arc<Chunk>),
    /// A builtin `NativeFunction` with this name.
    Native(String),
    /// Anything else (or unbound).
    Other,
}

/// A member of a global object (see [`Env::member`]).
#[derive(Debug, Clone, PartialEq)]
pub enum MemberView {
    Native(String),
    Float(f64),
    Other,
}

/// The verifier's view of the world outside the function.
pub trait Env {
    /// Current binding of global `name`.
    fn global(&self, name: &str) -> GlobalView;
    /// Current value of `object.field` for the global object `object`.
    fn member(&self, object: &str, field: &str) -> MemberView;
    /// Ensure `chunk` has a compiled specialization for `sig` (verifying
    /// and compiling it if needed) and return its return kind.
    fn callee(&mut self, chunk: &Arc<Chunk>, sig: &TypeSig) -> Result<JitType, String>;
}

/// An environment with no usable globals (unit tests, self-contained code).
pub struct NoGlobals;

impl Env for NoGlobals {
    fn global(&self, _: &str) -> GlobalView {
        GlobalView::Other
    }
    fn member(&self, _: &str, _: &str) -> MemberView {
        MemberView::Other
    }
    fn callee(&mut self, _: &Arc<Chunk>, _: &TypeSig) -> Result<JitType, String> {
        Err("no callee environment".into())
    }
}

/// An assumption about the VM's globals that native code relies on.
#[derive(Debug, Clone)]
pub enum Guard {
    /// Global `name` is a closure over code identical to `chunk`
    /// (`tier::same_code`). Covers self-calls and calls of other functions.
    Closure { name: String, chunk: Arc<Chunk> },
    /// Global `name` is the builtin `NativeFunction` of the same name.
    Native(Builtin),
    /// Global object `object` has member `field` equal to `expect`.
    Member {
        object: &'static str,
        field: &'static str,
        expect: MemberExpect,
    },
}

/// Expected value of a guarded object member.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemberExpect {
    /// `NativeFunction` named `<object>.<field>`.
    Native,
    /// A Float with exactly these bits.
    FloatBits(u64),
}

impl Guard {
    /// Same assumption (used to deduplicate transitive guard lists).
    pub fn same(&self, other: &Guard) -> bool {
        match (self, other) {
            (Guard::Closure { name: a, chunk: x }, Guard::Closure { name: b, chunk: y }) => {
                a == b && (Arc::ptr_eq(x, y) || x.proto_id == y.proto_id)
            }
            (Guard::Native(a), Guard::Native(b)) => a == b,
            (
                Guard::Member {
                    object: o1,
                    field: f1,
                    expect: e1,
                },
                Guard::Member {
                    object: o2,
                    field: f2,
                    expect: e2,
                },
            ) => o1 == o2 && f1 == f2 && e1 == e2,
            _ => false,
        }
    }
}

/// Add `g` to `guards` unless an identical assumption is already there.
pub fn push_guard(guards: &mut Vec<Guard>, g: Guard) {
    if !guards.iter().any(|x| x.same(&g)) {
        guards.push(g);
    }
}

/// A global (or derived) reference held in a register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ref {
    /// One of the native builtins.
    Builtin(Builtin),
    /// The `math` module object.
    Math,
    /// `callees[i]` of the function being verified.
    Callee(u16),
}

/// Abstract state of one register at one program point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegState {
    /// Not written on any path yet (the VM would read stale garbage).
    Undef,
    /// Holds a value of this kind on every path.
    Val(JitType),
    /// Holds this function itself (loaded via `GetGlobal` of its own name).
    SelfFn,
    /// Holds a guarded global reference; only usable as a call target or
    /// receiver.
    Ref(Ref),
    /// Holds the string constant with this index; only usable as a method
    /// name.
    Str(u16),
    /// The mode register of a counting `for` loop, known to select the
    /// counting path (`ForRangePrep` succeeded: the `range` guard holds and
    /// the arguments are verified Ints).
    RangeFast,
    /// Different kinds (or undefined) on different paths.
    Conflict,
}

impl RegState {
    fn join(self, other: RegState) -> RegState {
        if self == other {
            self
        } else {
            RegState::Conflict
        }
    }
}

impl fmt::Display for RegState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegState::Undef => write!(f, "uninitialized"),
            RegState::Val(t) => write!(f, "{}", t),
            RegState::SelfFn => write!(f, "self function"),
            RegState::Ref(Ref::Builtin(b)) => write!(f, "builtin {}", b.name()),
            RegState::Ref(Ref::Math) => write!(f, "module math"),
            RegState::Ref(Ref::Callee(_)) => write!(f, "function"),
            RegState::Str(_) => write!(f, "string"),
            RegState::RangeFast => write!(f, "range-loop mode"),
            RegState::Conflict => write!(f, "conflicting kinds"),
        }
    }
}

/// Why a function cannot be compiled for a signature.
#[derive(Debug, Clone, PartialEq)]
pub enum RejectKind {
    /// The specialization signature does not match the function's arity.
    ArityMismatch { expected: usize, got: usize },
    /// The function has no instructions.
    EmptyFunction,
    /// The byte does not decode to any opcode.
    InvalidOpcode(u8),
    /// A valid opcode that this tier does not implement.
    UnsupportedOpcode(OpCode),
    /// A constant kind this tier does not implement (null).
    UnsupportedConstant(&'static str),
    /// A constant index outside the constant pool.
    ConstantOutOfRange(usize),
    /// A register operand outside the frame.
    RegisterOutOfRange(usize),
    /// A jump whose target is outside the function.
    JumpOutOfRange(i64),
    /// Control can run past the last instruction.
    FallsOffEnd,
    /// An operand register does not hold an accepted kind on every path.
    OperandType {
        op: OpCode,
        reg: usize,
        found: RegState,
    },
    /// `GetGlobal` of a global that is not itself, a function or a
    /// supported builtin.
    NonSelfGlobal(String),
    /// `Call` of a value that is not itself, a function or a pure builtin.
    NonSelfCall,
    /// A self-call with a different argument count than the arity.
    SelfCallArity { expected: usize, got: usize },
    /// A self-call whose argument kinds differ from this specialization.
    SelfCallArgType {
        index: usize,
        expected: JitType,
        found: RegState,
    },
    /// A call of another function with the wrong argument count.
    CalleeArity {
        name: String,
        expected: usize,
        got: usize,
    },
    /// A called function could not be specialized for its arguments.
    Callee { name: String, reason: String },
    /// A builtin call the JIT does not implement for these arguments.
    UnsupportedBuiltin(String),
    /// `ReturnNull` (or implicit null return) is reachable.
    ReturnsNull,
    /// Returns of different kinds.
    ReturnTypeMismatch { expected: JitType, found: JitType },
    /// No `Return` is reachable.
    NoReturn,
}

/// A structured rejection: what failed and at which instruction.
#[derive(Debug, Clone, PartialEq)]
pub struct Reject {
    pub ip: Option<usize>,
    pub kind: RejectKind,
}

impl Reject {
    fn at(ip: usize, kind: RejectKind) -> Self {
        Reject { ip: Some(ip), kind }
    }
}

impl fmt::Display for RejectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RejectKind::ArityMismatch { expected, got } => {
                write!(f, "signature has {} args, function takes {}", got, expected)
            }
            RejectKind::EmptyFunction => write!(f, "empty function"),
            RejectKind::InvalidOpcode(b) => write!(f, "invalid opcode byte {}", b),
            RejectKind::UnsupportedOpcode(op) => write!(f, "unsupported opcode {:?}", op),
            RejectKind::UnsupportedConstant(k) => write!(f, "unsupported {} constant", k),
            RejectKind::ConstantOutOfRange(i) => write!(f, "constant index {} out of range", i),
            RejectKind::RegisterOutOfRange(r) => write!(f, "register r{} out of range", r),
            RejectKind::JumpOutOfRange(t) => write!(f, "jump target {} out of range", t),
            RejectKind::FallsOffEnd => write!(f, "control falls off the end of the function"),
            RejectKind::OperandType { op, reg, found } => {
                write!(f, "{:?} operand r{} is {}", op, reg, found)
            }
            RejectKind::NonSelfGlobal(name) => write!(f, "reads global `{}`", name),
            RejectKind::NonSelfCall => write!(f, "calls a value the JIT cannot call"),
            RejectKind::SelfCallArity { expected, got } => {
                write!(f, "self-call with {} args, arity is {}", got, expected)
            }
            RejectKind::SelfCallArgType {
                index,
                expected,
                found,
            } => write!(
                f,
                "self-call argument {} is {}, specialization expects {}",
                index, found, expected
            ),
            RejectKind::CalleeArity {
                name,
                expected,
                got,
            } => write!(f, "calls {} with {} args, arity is {}", name, got, expected),
            RejectKind::Callee { name, reason } => {
                write!(f, "callee {} not compilable: {}", name, reason)
            }
            RejectKind::UnsupportedBuiltin(what) => write!(f, "unsupported builtin call {}", what),
            RejectKind::ReturnsNull => write!(f, "may return null"),
            RejectKind::ReturnTypeMismatch { expected, found } => {
                write!(f, "returns both {} and {}", expected, found)
            }
            RejectKind::NoReturn => write!(f, "never returns"),
        }
    }
}

impl fmt::Display for Reject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ip {
            Some(ip) => write!(f, "{} (at ip {})", self.kind, ip),
            None => write!(f, "{}", self.kind),
        }
    }
}

/// What a verified `Call` does.
#[derive(Debug, Clone)]
pub enum CallInfo {
    /// Recursive call of the function itself.
    SelfCall,
    /// Direct native call of another function's specialization.
    Callee {
        id: FnId,
        sig: TypeSig,
        ret: JitType,
    },
    /// A pure builtin computed inline (or through a pure bridge).
    Pure {
        op: PureOp,
        /// Index of the first argument register relative to the call's `A`.
        first_arg: usize,
        args: Vec<JitType>,
        ret: JitType,
    },
}

/// Per-instruction facts the IR builder needs beyond register states.
#[derive(Debug, Clone)]
pub enum OpInfo {
    Call(CallInfo),
    /// `GetField` of a guarded Float constant.
    FloatConst(f64),
}

/// A function proven compilable for `sig`.
#[derive(Debug, Clone)]
pub struct VerifiedFn {
    pub sig: TypeSig,
    /// Kind of every returned value.
    pub ret: JitType,
    /// Number of VM registers in the frame (all < 256).
    pub num_regs: usize,
    /// Register states before each instruction; `None` = unreachable.
    pub states: Vec<Option<Vec<RegState>>>,
    /// Facts about calls and constant loads, by instruction.
    pub ops: Vec<Option<OpInfo>>,
    /// Assumptions about globals made by this function's own code. The
    /// tier adds the self-binding guard (when `has_self_calls`) and the
    /// callees' guards.
    pub guards: Vec<Guard>,
    /// True when the function calls itself (through its global binding).
    pub has_self_calls: bool,
}

/// Prove `chunk` compilable for argument kinds `sig` without access to any
/// globals (only self-calls are resolvable).
#[allow(dead_code)] // unit tests and tooling; the tier uses `verify_in`
pub fn verify(chunk: &Chunk, sig: &TypeSig) -> Result<VerifiedFn, Reject> {
    verify_in(chunk, sig, &mut NoGlobals)
}

/// Prove `chunk` compilable for argument kinds `sig`, resolving globals and
/// callees through `env`.
pub fn verify_in(chunk: &Chunk, sig: &TypeSig, env: &mut dyn Env) -> Result<VerifiedFn, Reject> {
    // The return kind feeds back into self-call results, so it is assumed and
    // then checked against every reachable `Return`. Try each candidate; on
    // total failure report the reason found under the first assumption.
    let mut first = None;
    for ret in [JitType::Int, JitType::Float, JitType::Bool] {
        match verify_with_ret(chunk, sig, ret, env) {
            Ok(v) => return Ok(v),
            Err(e) => {
                if first.is_none() {
                    first = Some(e);
                }
            }
        }
    }
    Err(first.expect("BUG: at least one return kind was tried"))
}

fn reg_in_range(ip: usize, reg: usize, num_regs: usize) -> Result<usize, Reject> {
    if reg < num_regs {
        Ok(reg)
    } else {
        Err(Reject::at(ip, RejectKind::RegisterOutOfRange(reg)))
    }
}

/// Self-reference is only recognized for named, non-lambda functions.
fn self_name(chunk: &Chunk) -> Option<&str> {
    let n = chunk.name.as_str();
    if n.is_empty() || n == "<lambda>" || n == "<main>" {
        None
    } else {
        Some(n)
    }
}

/// Result kind of a binary arithmetic opcode (`semantics::binary`).
fn arith_result(l: JitType, r: JitType) -> JitType {
    if l == JitType::Int && r == JitType::Int {
        JitType::Int
    } else {
        JitType::Float
    }
}

fn verify_with_ret(
    chunk: &Chunk,
    sig: &TypeSig,
    assumed_ret: JitType,
    env: &mut dyn Env,
) -> Result<VerifiedFn, Reject> {
    let arity = chunk.arity as usize;
    if sig.arity() != arity {
        return Err(Reject {
            ip: None,
            kind: RejectKind::ArityMismatch {
                expected: arity,
                got: sig.arity(),
            },
        });
    }
    let len = chunk.code.len();
    if len == 0 {
        return Err(Reject {
            ip: None,
            kind: RejectKind::EmptyFunction,
        });
    }
    // Mirrors the VM frame: `max(max_registers, 1)`, and args occupy 0..arity.
    let num_regs = (chunk.max_registers as usize).max(arity).max(1);

    let mut entry = vec![RegState::Undef; num_regs];
    for (i, t) in sig.0.iter().enumerate() {
        entry[i] = RegState::Val(*t);
    }

    let mut states: Vec<Option<Vec<RegState>>> = vec![None; len];
    let mut ops: Vec<Option<OpInfo>> = vec![None; len];
    let mut guards: Vec<Guard> = Vec::new();
    let mut callees: Vec<(String, Arc<Chunk>)> = Vec::new();
    states[0] = Some(entry);
    let mut worklist: Vec<usize> = vec![0];
    let mut on_list = vec![false; len];
    on_list[0] = true;
    let mut saw_return = false;
    let mut has_self_calls = false;

    while let Some(ip) = worklist.pop() {
        on_list[ip] = false;
        let mut s = states[ip]
            .clone()
            .expect("BUG: worklist entries always have a state");
        let inst = chunk.code[ip];
        let op_byte = decode_op(inst);
        let opcode =
            OpCode::try_from(op_byte).map_err(|b| Reject::at(ip, RejectKind::InvalidOpcode(b)))?;
        let a = decode_a(inst) as usize;
        let b = decode_b(inst) as usize;
        let c = decode_c(inst) as usize;
        let bx = decode_bx(inst) as usize;
        let sbx = decode_sbx(inst) as i64;

        let read = |s: &[RegState], reg: usize| -> Result<RegState, Reject> {
            let reg = reg_in_range(ip, reg, num_regs)?;
            Ok(s[reg])
        };
        let operand_err = |reg: usize, found: RegState| {
            Reject::at(
                ip,
                RejectKind::OperandType {
                    op: opcode,
                    reg,
                    found,
                },
            )
        };
        // Any value kind.
        let value = |s: &[RegState], reg: usize| -> Result<JitType, Reject> {
            match read(s, reg)? {
                RegState::Val(t) => Ok(t),
                found => Err(operand_err(reg, found)),
            }
        };
        let numeric = |s: &[RegState], reg: usize| -> Result<JitType, Reject> {
            match read(s, reg)? {
                RegState::Val(t) if t.is_numeric() => Ok(t),
                found => Err(operand_err(reg, found)),
            }
        };
        let int = |s: &[RegState], reg: usize| -> Result<(), Reject> {
            match read(s, reg)? {
                RegState::Val(JitType::Int) => Ok(()),
                found => Err(operand_err(reg, found)),
            }
        };
        let const_str = |idx: usize| -> Result<&String, Reject> {
            match chunk.constants.get(idx) {
                Some(Constant::Str(n)) => Ok(n),
                Some(_) => Err(Reject::at(
                    ip,
                    RejectKind::NonSelfGlobal("<non-string>".into()),
                )),
                None => Err(Reject::at(ip, RejectKind::ConstantOutOfRange(idx))),
            }
        };
        let jump_target = || -> Result<usize, Reject> {
            let t = ip as i64 + 1 + sbx;
            if t < 0 || t >= len as i64 {
                Err(Reject::at(ip, RejectKind::JumpOutOfRange(t)))
            } else {
                Ok(t as usize)
            }
        };

        // Successor edges with the state each one receives. `None` means
        // "the state after this instruction" (`s`).
        let mut fallthrough = true;
        let mut target: Option<usize> = None;
        let mut extra: Option<(usize, Vec<RegState>)> = None;

        match opcode {
            OpCode::LoadConst => {
                let dst = reg_in_range(ip, a, num_regs)?;
                let k = chunk
                    .constants
                    .get(bx)
                    .ok_or_else(|| Reject::at(ip, RejectKind::ConstantOutOfRange(bx)))?;
                s[dst] = match k {
                    Constant::Int(_) => RegState::Val(JitType::Int),
                    Constant::Bool(_) => RegState::Val(JitType::Bool),
                    Constant::Float(_) => RegState::Val(JitType::Float),
                    Constant::Str(_) => match u16::try_from(bx) {
                        Ok(idx) => RegState::Str(idx),
                        Err(_) => {
                            return Err(Reject::at(ip, RejectKind::UnsupportedConstant("string")))
                        }
                    },
                    Constant::Null => {
                        return Err(Reject::at(ip, RejectKind::UnsupportedConstant("null")))
                    }
                };
            }
            OpCode::LoadTrue | OpCode::LoadFalse => {
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::Move | OpCode::GetLocal | OpCode::SetLocal => {
                let dst = reg_in_range(ip, a, num_regs)?;
                let v = read(&s, b)?;
                match v {
                    RegState::Undef | RegState::Conflict => return Err(operand_err(b, v)),
                    _ => s[dst] = v,
                }
            }
            OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod => {
                let l = numeric(&s, b)?;
                let r = numeric(&s, c)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(arith_result(l, r));
            }
            OpCode::AddLocal => {
                // `R(A) = R(A) + R(B)`; numbers only, so never the in-place
                // string path.
                let l = numeric(&s, a)?;
                let r = numeric(&s, b)?;
                s[a] = RegState::Val(arith_result(l, r));
            }
            OpCode::Neg => {
                let t = numeric(&s, b)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(t);
            }
            OpCode::Lt | OpCode::Gt | OpCode::LtEq | OpCode::GtEq => {
                numeric(&s, b)?;
                numeric(&s, c)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::Eq | OpCode::NotEq => {
                // `Value::equals`: numbers compare numerically across
                // Int/Float, Bool only equals Bool.
                value(&s, b)?;
                value(&s, c)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::Not => {
                value(&s, b)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::And | OpCode::Or => {
                value(&s, b)?;
                value(&s, c)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::Jump | OpCode::Loop => {
                fallthrough = false;
                target = Some(jump_target()?);
            }
            OpCode::JumpIfFalse | OpCode::JumpIfTrue => {
                let t = jump_target()?;
                if read(&s, a)? == RegState::RangeFast {
                    // The counting-loop mode is known to be true.
                    if opcode == OpCode::JumpIfTrue {
                        fallthrough = false;
                        target = Some(t);
                    }
                } else {
                    value(&s, a)?;
                    target = Some(t);
                }
            }
            OpCode::GetGlobal => {
                let dst = reg_in_range(ip, a, num_regs)?;
                let name = const_str(bx)?;
                s[dst] = if self_name(chunk) == Some(name.as_str()) {
                    RegState::SelfFn
                } else if let Some(builtin) = Builtin::from_global(name) {
                    match env.global(name) {
                        GlobalView::Native(n) if n == builtin.name() => {}
                        _ => return Err(Reject::at(ip, RejectKind::NonSelfGlobal(name.clone()))),
                    }
                    push_guard(&mut guards, Guard::Native(builtin));
                    RegState::Ref(Ref::Builtin(builtin))
                } else if name == MATH_MODULE {
                    // Guarded per member at the use site.
                    RegState::Ref(Ref::Math)
                } else {
                    let GlobalView::Closure(callee) = env.global(name) else {
                        return Err(Reject::at(ip, RejectKind::NonSelfGlobal(name.clone())));
                    };
                    push_guard(
                        &mut guards,
                        Guard::Closure {
                            name: name.clone(),
                            chunk: callee.clone(),
                        },
                    );
                    let idx = match callees.iter().position(|(n, _)| n == name) {
                        Some(i) => i,
                        None => {
                            callees.push((name.clone(), callee));
                            callees.len() - 1
                        }
                    };
                    let idx = u16::try_from(idx)
                        .map_err(|_| Reject::at(ip, RejectKind::NonSelfGlobal(name.clone())))?;
                    RegState::Ref(Ref::Callee(idx))
                };
            }
            OpCode::GetField => {
                let dst = reg_in_range(ip, a, num_regs)?;
                if read(&s, b)? != RegState::Ref(Ref::Math) {
                    return Err(Reject::at(ip, RejectKind::UnsupportedOpcode(opcode)));
                }
                let field = const_str(c)?;
                let MemberView::Float(f) = env.member(MATH_MODULE, field) else {
                    return Err(Reject::at(
                        ip,
                        RejectKind::UnsupportedBuiltin(format!("math.{}", field)),
                    ));
                };
                let field: &'static str = match field.as_str() {
                    "pi" => "pi",
                    "e" => "e",
                    "inf" => "inf",
                    other => {
                        return Err(Reject::at(
                            ip,
                            RejectKind::UnsupportedBuiltin(format!("math.{}", other)),
                        ))
                    }
                };
                push_guard(
                    &mut guards,
                    Guard::Member {
                        object: MATH_MODULE,
                        field,
                        expect: MemberExpect::FloatBits(f.to_bits()),
                    },
                );
                ops[ip] = Some(OpInfo::FloatConst(f));
                s[dst] = RegState::Val(JitType::Float);
            }
            OpCode::Call => {
                let dst = reg_in_range(ip, c, num_regs)?;
                let kinds = |s: &[RegState], from: usize, n: usize| {
                    (0..n)
                        .map(|i| value(s, from + i))
                        .collect::<Result<Vec<JitType>, Reject>>()
                };
                let info = match read(&s, a)? {
                    RegState::SelfFn => {
                        if b != arity {
                            return Err(Reject::at(
                                ip,
                                RejectKind::SelfCallArity {
                                    expected: arity,
                                    got: b,
                                },
                            ));
                        }
                        for (i, expected) in sig.0.iter().enumerate() {
                            let found = read(&s, a + 1 + i)?;
                            if found != RegState::Val(*expected) {
                                return Err(Reject::at(
                                    ip,
                                    RejectKind::SelfCallArgType {
                                        index: i,
                                        expected: *expected,
                                        found,
                                    },
                                ));
                            }
                        }
                        has_self_calls = true;
                        CallInfo::SelfCall
                    }
                    RegState::Ref(Ref::Callee(idx)) => {
                        let (name, callee) = callees[idx as usize].clone();
                        if b != callee.arity as usize {
                            return Err(Reject::at(
                                ip,
                                RejectKind::CalleeArity {
                                    name,
                                    expected: callee.arity as usize,
                                    got: b,
                                },
                            ));
                        }
                        let csig = TypeSig(kinds(&s, a + 1, b)?);
                        let ret = env.callee(&callee, &csig).map_err(|reason| {
                            Reject::at(
                                ip,
                                RejectKind::Callee {
                                    name: name.clone(),
                                    reason,
                                },
                            )
                        })?;
                        CallInfo::Callee {
                            id: FnId::of(&callee),
                            sig: csig,
                            ret,
                        }
                    }
                    RegState::Ref(Ref::Builtin(builtin @ (Builtin::Float | Builtin::Int))) => {
                        let args = kinds(&s, a + 1, b)?;
                        let (op, ok) = match builtin {
                            Builtin::Float => (
                                PureOp::ToFloat,
                                args.len() == 1 && args[0].is_numeric(),
                            ),
                            _ => (PureOp::ToInt, args.len() == 1),
                        };
                        if !ok {
                            return Err(Reject::at(
                                ip,
                                RejectKind::UnsupportedBuiltin(format!(
                                    "{}{}",
                                    builtin.name(),
                                    TypeSig(args)
                                )),
                            ));
                        }
                        let ret = if op == PureOp::ToFloat {
                            JitType::Float
                        } else {
                            JitType::Int
                        };
                        CallInfo::Pure {
                            op,
                            first_arg: 1,
                            args,
                            ret,
                        }
                    }
                    RegState::Ref(Ref::Builtin(Builtin::CallMethod)) => {
                        // `__forge_call_method(receiver, "name", args..)`.
                        if b < 2 || read(&s, a + 1)? != RegState::Ref(Ref::Math) {
                            return Err(Reject::at(ip, RejectKind::NonSelfCall));
                        }
                        let RegState::Str(k) = read(&s, a + 2)? else {
                            return Err(Reject::at(ip, RejectKind::NonSelfCall));
                        };
                        let member = const_str(k as usize)?;
                        let args = kinds(&s, a + 3, b - 2)?;
                        let unsupported = || {
                            Reject::at(
                                ip,
                                RejectKind::UnsupportedBuiltin(format!(
                                    "math.{}{}",
                                    member,
                                    TypeSig(args.clone())
                                )),
                            )
                        };
                        let f = MathFn::from_member(member).ok_or_else(unsupported)?;
                        let ret = f.result(&args).ok_or_else(unsupported)?;
                        let qualified = format!("{}.{}", MATH_MODULE, f.member());
                        if env.member(MATH_MODULE, f.member()) != MemberView::Native(qualified) {
                            return Err(unsupported());
                        }
                        push_guard(
                            &mut guards,
                            Guard::Member {
                                object: MATH_MODULE,
                                field: f.member(),
                                expect: MemberExpect::Native,
                            },
                        );
                        CallInfo::Pure {
                            op: PureOp::Math(f),
                            first_arg: 3,
                            args,
                            ret,
                        }
                    }
                    _ => return Err(Reject::at(ip, RejectKind::NonSelfCall)),
                };
                let ret = match &info {
                    CallInfo::SelfCall => assumed_ret,
                    CallInfo::Callee { ret, .. } | CallInfo::Pure { ret, .. } => *ret,
                };
                ops[ip] = Some(OpInfo::Call(info));
                s[dst] = RegState::Val(ret);
            }
            OpCode::ForRangePrep => {
                if read(&s, a)? != RegState::Ref(Ref::Builtin(Builtin::Range)) || !(1..=2).contains(&b)
                {
                    return Err(Reject::at(ip, RejectKind::UnsupportedOpcode(opcode)));
                }
                for i in 0..b {
                    int(&s, a + 1 + i)?;
                }
                reg_in_range(ip, a + 1, num_regs)?;
                let mode = reg_in_range(ip, c, num_regs)?;
                s[a] = RegState::Val(JitType::Int);
                s[a + 1] = RegState::Val(JitType::Int);
                s[mode] = RegState::RangeFast;
            }
            OpCode::ForRangeNext => {
                int(&s, a)?;
                int(&s, a + 1)?;
                let var = reg_in_range(ip, b, num_regs)?;
                // Exhausted: continue with the exit jump at ip + 1 (state
                // unchanged). Otherwise skip it with the variable set.
                if ip + 2 >= len {
                    return Err(Reject::at(ip, RejectKind::FallsOffEnd));
                }
                let mut next = s.clone();
                next[var] = RegState::Val(JitType::Int);
                next[a] = RegState::Val(JitType::Int);
                extra = Some((ip + 2, next));
            }
            OpCode::Return => {
                let t = value(&s, a)?;
                if t != assumed_ret {
                    return Err(Reject::at(
                        ip,
                        RejectKind::ReturnTypeMismatch {
                            expected: assumed_ret,
                            found: t,
                        },
                    ));
                }
                saw_return = true;
                fallthrough = false;
            }
            OpCode::ReturnNull => return Err(Reject::at(ip, RejectKind::ReturnsNull)),
            other => return Err(Reject::at(ip, RejectKind::UnsupportedOpcode(other))),
        }

        let mut succs: Vec<(usize, Vec<RegState>)> = Vec::with_capacity(3);
        if fallthrough {
            if ip + 1 >= len {
                return Err(Reject::at(ip, RejectKind::FallsOffEnd));
            }
            succs.push((ip + 1, s.clone()));
        }
        if let Some(t) = target {
            succs.push((t, s.clone()));
        }
        if let Some(e) = extra {
            succs.push(e);
        }
        for (succ, out) in succs {
            let changed = match &mut states[succ] {
                slot @ None => {
                    *slot = Some(out);
                    true
                }
                Some(existing) => {
                    let mut changed = false;
                    for (e, n) in existing.iter_mut().zip(out.iter()) {
                        let j = e.join(*n);
                        if j != *e {
                            *e = j;
                            changed = true;
                        }
                    }
                    changed
                }
            };
            if changed && !on_list[succ] {
                on_list[succ] = true;
                worklist.push(succ);
            }
        }
    }

    if !saw_return {
        return Err(Reject {
            ip: None,
            kind: RejectKind::NoReturn,
        });
    }

    Ok(VerifiedFn {
        sig: sig.clone(),
        ret: assumed_ret,
        num_regs,
        states,
        ops,
        guards,
        has_self_calls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int_sig(n: usize) -> TypeSig {
        TypeSig(vec![JitType::Int; n])
    }

    fn chunk(name: &str, arity: u8, regs: u8) -> Chunk {
        let mut c = Chunk::new(name);
        c.arity = arity;
        c.max_registers = regs;
        c
    }

    #[test]
    fn accepts_int_arithmetic() {
        let mut c = chunk("add", 2, 3);
        c.emit(encode_abc(OpCode::Add, 2, 0, 1), 1);
        c.emit(encode_abc(OpCode::Return, 2, 0, 0), 1);
        let vf = verify(&c, &int_sig(2)).expect("verifies");
        assert_eq!(vf.ret, JitType::Int);
        assert!(!vf.has_self_calls);
    }

    #[test]
    fn infers_bool_return() {
        let mut c = chunk("lt", 2, 3);
        c.emit(encode_abc(OpCode::Lt, 2, 0, 1), 1);
        c.emit(encode_abc(OpCode::Return, 2, 0, 0), 1);
        assert_eq!(verify(&c, &int_sig(2)).unwrap().ret, JitType::Bool);
    }

    #[test]
    fn rejects_arity_mismatch() {
        let mut c = chunk("f", 2, 3);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        let r = verify(&c, &int_sig(1)).unwrap_err();
        assert_eq!(
            r.kind,
            RejectKind::ArityMismatch {
                expected: 2,
                got: 1
            }
        );
    }

    #[test]
    fn accepts_float_constants_and_rejects_null() {
        let mut c = chunk("f", 0, 1);
        let k = c.add_constant(Constant::Float(1.5));
        c.emit(encode_abx(OpCode::LoadConst, 0, k), 1);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        assert_eq!(verify(&c, &int_sig(0)).unwrap().ret, JitType::Float);
        let mut c = chunk("g", 0, 1);
        c.emit(encode_abc(OpCode::LoadNull, 0, 0, 0), 1);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        assert_eq!(
            verify(&c, &int_sig(0)).unwrap_err().kind,
            RejectKind::UnsupportedOpcode(OpCode::LoadNull)
        );
    }

    #[test]
    fn rejects_bool_arithmetic_operand() {
        let mut c = chunk("f", 1, 2);
        c.emit(encode_abc(OpCode::Add, 1, 0, 0), 1);
        c.emit(encode_abc(OpCode::Return, 1, 0, 0), 1);
        let r = verify(&c, &TypeSig(vec![JitType::Bool])).unwrap_err();
        assert!(matches!(
            r.kind,
            RejectKind::OperandType {
                op: OpCode::Add,
                ..
            }
        ));
    }

    #[test]
    fn rejects_uninitialized_read() {
        let mut c = chunk("f", 0, 2);
        c.emit(encode_abc(OpCode::Return, 1, 0, 0), 1);
        let r = verify(&c, &int_sig(0)).unwrap_err();
        assert_eq!(
            r.kind,
            RejectKind::OperandType {
                op: OpCode::Return,
                reg: 1,
                found: RegState::Undef
            }
        );
    }

    #[test]
    fn rejects_path_dependent_kinds() {
        // r1 is Int on one path and Bool on the other, then returned.
        let mut c = chunk("f", 1, 2);
        let one = c.add_constant(Constant::Int(1));
        c.emit(encode_asbx(OpCode::JumpIfFalse, 0, 2), 1); // 0 -> 3
        c.emit(encode_abx(OpCode::LoadConst, 1, one), 1); // 1
        c.emit(encode_asbx(OpCode::Jump, 0, 1), 1); // 2 -> 4
        c.emit(encode_abc(OpCode::LoadTrue, 1, 0, 0), 1); // 3
        c.emit(encode_abc(OpCode::Return, 1, 0, 0), 1); // 4
                                                        // Rejection may be reported at the join (Conflict) or earlier, when
                                                        // the Bool path's return disagrees with the Int path's.
        let r = verify(&c, &int_sig(1)).unwrap_err();
        assert!(
            matches!(
                r.kind,
                RejectKind::OperandType {
                    found: RegState::Conflict,
                    ..
                } | RejectKind::ReturnTypeMismatch { .. }
            ),
            "{:?}",
            r
        );
    }

    #[test]
    fn rejects_return_null_and_fall_off() {
        let mut c = chunk("f", 0, 1);
        c.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1);
        assert_eq!(
            verify(&c, &int_sig(0)).unwrap_err().kind,
            RejectKind::ReturnsNull
        );
        let mut c = chunk("g", 0, 1);
        c.emit(encode_abc(OpCode::LoadTrue, 0, 0, 0), 1);
        assert_eq!(
            verify(&c, &int_sig(0)).unwrap_err().kind,
            RejectKind::FallsOffEnd
        );
    }

    #[test]
    fn rejects_foreign_globals_and_lambda_self_reference() {
        let mut c = chunk("f", 0, 1);
        let k = c.add_constant(Constant::Str("g".into()));
        c.emit(encode_abx(OpCode::GetGlobal, 0, k), 1);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        assert_eq!(
            verify(&c, &int_sig(0)).unwrap_err().kind,
            RejectKind::NonSelfGlobal("g".into())
        );
        let mut c = chunk("<lambda>", 0, 1);
        let k = c.add_constant(Constant::Str("<lambda>".into()));
        c.emit(encode_abx(OpCode::GetGlobal, 0, k), 1);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        assert!(matches!(
            verify(&c, &int_sig(0)).unwrap_err().kind,
            RejectKind::NonSelfGlobal(_)
        ));
    }

    #[test]
    fn rejects_self_call_with_other_arity_or_kinds() {
        // f(n) { return f(n, n) }
        let mut c = chunk("f", 1, 4);
        let k = c.add_constant(Constant::Str("f".into()));
        c.emit(encode_abx(OpCode::GetGlobal, 1, k), 1);
        c.emit(encode_abc(OpCode::Move, 2, 0, 0), 1);
        c.emit(encode_abc(OpCode::Move, 3, 0, 0), 1);
        c.emit(encode_abc(OpCode::Call, 1, 2, 1), 1);
        c.emit(encode_abc(OpCode::Return, 1, 0, 0), 1);
        assert_eq!(
            verify(&c, &int_sig(1)).unwrap_err().kind,
            RejectKind::SelfCallArity {
                expected: 1,
                got: 2
            }
        );
        // f(n) { return f(true) } under an Int signature
        let mut c = chunk("f", 1, 3);
        let k = c.add_constant(Constant::Str("f".into()));
        c.emit(encode_abx(OpCode::GetGlobal, 1, k), 1);
        c.emit(encode_abc(OpCode::LoadTrue, 2, 0, 0), 1);
        c.emit(encode_abc(OpCode::Call, 1, 1, 1), 1);
        c.emit(encode_abc(OpCode::Return, 1, 0, 0), 1);
        assert!(matches!(
            verify(&c, &int_sig(1)).unwrap_err().kind,
            RejectKind::SelfCallArgType { index: 0, .. }
        ));
    }

    #[test]
    fn rejects_every_unsupported_opcode_family() {
        for op in [
            OpCode::Closure,
            OpCode::GetUpvalue,
            OpCode::SetUpvalue,
            OpCode::SetGlobal,
            OpCode::Concat,
            OpCode::Len,
            OpCode::NewArray,
            OpCode::GetIndex,
            OpCode::Interpolate,
            OpCode::PushHandler,
            OpCode::PushTimeout,
            OpCode::Spawn,
            OpCode::Pop,
        ] {
            let mut c = chunk("f", 1, 2);
            c.emit(encode_abc(op, 1, 0, 0), 1);
            c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
            let r = verify(&c, &int_sig(1)).unwrap_err();
            assert_eq!(r.kind, RejectKind::UnsupportedOpcode(op), "{:?}", op);
            assert_eq!(r.ip, Some(0));
        }
    }

    #[test]
    fn unreachable_code_is_not_checked() {
        let mut c = chunk("f", 1, 2);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        c.emit(encode_abc(OpCode::ReturnNull, 0, 0, 0), 1);
        let vf = verify(&c, &int_sig(1)).expect("dead ReturnNull is fine");
        assert!(vf.states[1].is_none());
    }
}
