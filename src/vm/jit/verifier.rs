//! Compilability proof for a function under a given [`TypeSig`].
//!
//! The verifier is the single gatekeeper of the JIT. It runs a
//! flow-sensitive abstract interpretation over the bytecode CFG, tracking the
//! [`RegState`] of every register before every reachable instruction, and
//! checks each reachable instruction against an explicit allowlist with
//! exact operand-type requirements. Anything not on the allowlist, any read
//! of a register that is not definitely initialized with a supported kind,
//! any non-self call, any control flow leaving the code, and any
//! inconsistent return kind is a [`Reject`] carrying a structured reason.
//!
//! The resulting [`VerifiedFn`] is everything the IR builder needs; the
//! builder has no "unsupported → skip" arms.
//!
//! ## Int/Bool tier semantics (must match `machine.rs` exactly)
//!
//! | opcode | accepted operands | result | deopt when |
//! |---|---|---|---|
//! | `LoadConst` | `Int`/`Bool` constant | same | — |
//! | `LoadTrue`/`LoadFalse` | — | Bool | — |
//! | `Move`/`GetLocal`/`SetLocal` | any defined | copy | — |
//! | `Add`/`Sub`/`Mul` | Int, Int | Int | i64 overflow (VM promotes to Float) |
//! | `Div`/`Mod` | Int, Int | Int | divisor 0 (VM error), `MIN / -1` (VM panic) |
//! | `Neg` | Int | Int | `MIN` (VM promotes to Float) |
//! | `Lt`/`Gt`/`LtEq`/`GtEq` | Int, Int | Bool | — |
//! | `Eq`/`NotEq` | Int,Int or Bool,Bool | Bool | — |
//! | `Not`/`And`/`Or` | Int or Bool (truthiness) | Bool | — |
//! | `Jump`/`Loop`/`JumpIf*` | cond Int or Bool | — | `Loop`: task cancelled |
//! | `GetGlobal` | the function's own name | self ref | — (entry guard checks binding) |
//! | `Call` | self ref, exactly `arity` args matching the signature | return kind | stack depth / cancelled |
//! | `Return` | Int or Bool, same kind on every path | — | — |

use std::fmt;

use crate::vm::bytecode::*;
use crate::vm::jit::types::{JitType, TypeSig};

/// Abstract state of one register at one program point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegState {
    /// Not written on any path yet (the VM would read stale garbage).
    Undef,
    /// Holds a value of this kind on every path.
    Val(JitType),
    /// Holds this function itself (loaded via `GetGlobal` of its own name).
    SelfFn,
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
    /// A constant kind this tier does not implement (float, string, null).
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
    /// `GetGlobal` of something other than the function's own name.
    NonSelfGlobal(String),
    /// `Call` of a value other than the function itself.
    NonSelfCall,
    /// A self-call with a different argument count than the arity.
    SelfCallArity { expected: usize, got: usize },
    /// A self-call whose argument kinds differ from this specialization.
    SelfCallArgType {
        index: usize,
        expected: JitType,
        found: RegState,
    },
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
            RejectKind::NonSelfCall => write!(f, "calls a function other than itself"),
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
    /// True when the function calls itself (needs the self-binding guard).
    pub has_self_calls: bool,
}

/// Prove `chunk` compilable for argument kinds `sig`.
pub fn verify(chunk: &Chunk, sig: &TypeSig) -> Result<VerifiedFn, Reject> {
    // The return kind feeds back into self-call results, so it is assumed and
    // then checked against every reachable `Return`. Try each candidate; on
    // total failure report the reason found under the first assumption.
    match verify_with_ret(chunk, sig, JitType::Int) {
        Ok(v) => Ok(v),
        Err(first) => verify_with_ret(chunk, sig, JitType::Bool).map_err(|_| first),
    }
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

fn verify_with_ret(
    chunk: &Chunk,
    sig: &TypeSig,
    assumed_ret: JitType,
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
        let want = |s: &[RegState], reg: usize, ok: &[JitType]| -> Result<JitType, Reject> {
            match read(s, reg)? {
                RegState::Val(t) if ok.contains(&t) => Ok(t),
                found => Err(Reject::at(
                    ip,
                    RejectKind::OperandType {
                        op: opcode,
                        reg,
                        found,
                    },
                )),
            }
        };
        const INT: &[JitType] = &[JitType::Int];
        const ANY: &[JitType] = &[JitType::Int, JitType::Bool];

        // Successors: fallthrough and/or jump target.
        let mut fallthrough = true;
        let mut target: Option<usize> = None;
        let jump_target = || -> Result<usize, Reject> {
            let t = ip as i64 + 1 + sbx;
            if t < 0 || t >= len as i64 {
                Err(Reject::at(ip, RejectKind::JumpOutOfRange(t)))
            } else {
                Ok(t as usize)
            }
        };

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
                    Constant::Float(_) => {
                        return Err(Reject::at(ip, RejectKind::UnsupportedConstant("float")))
                    }
                    Constant::Str(_) => {
                        return Err(Reject::at(ip, RejectKind::UnsupportedConstant("string")))
                    }
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
                    RegState::Val(_) | RegState::SelfFn => s[dst] = v,
                    found => {
                        return Err(Reject::at(
                            ip,
                            RejectKind::OperandType {
                                op: opcode,
                                reg: b,
                                found,
                            },
                        ))
                    }
                }
            }
            OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div | OpCode::Mod => {
                want(&s, b, INT)?;
                want(&s, c, INT)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Int);
            }
            OpCode::AddLocal => {
                // `R(A) = R(A) + R(B)`; in a verified (int-only) function
                // there are no strings, so no in-place path.
                want(&s, a, INT)?;
                want(&s, b, INT)?;
                s[a] = RegState::Val(JitType::Int);
            }
            OpCode::Neg => {
                want(&s, b, INT)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Int);
            }
            OpCode::Lt | OpCode::Gt | OpCode::LtEq | OpCode::GtEq => {
                want(&s, b, INT)?;
                want(&s, c, INT)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::Eq | OpCode::NotEq => {
                let lt = want(&s, b, ANY)?;
                // Both sides must share a kind: Int==Bool is not modelled.
                want(&s, c, &[lt])?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::Not => {
                want(&s, b, ANY)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::And | OpCode::Or => {
                want(&s, b, ANY)?;
                want(&s, c, ANY)?;
                let dst = reg_in_range(ip, a, num_regs)?;
                s[dst] = RegState::Val(JitType::Bool);
            }
            OpCode::Jump | OpCode::Loop => {
                fallthrough = false;
                target = Some(jump_target()?);
            }
            OpCode::JumpIfFalse | OpCode::JumpIfTrue => {
                want(&s, a, ANY)?;
                target = Some(jump_target()?);
            }
            OpCode::GetGlobal => {
                let dst = reg_in_range(ip, a, num_regs)?;
                let name = match chunk.constants.get(bx) {
                    Some(Constant::Str(n)) => n,
                    Some(_) => {
                        return Err(Reject::at(
                            ip,
                            RejectKind::NonSelfGlobal("<non-string>".into()),
                        ))
                    }
                    None => return Err(Reject::at(ip, RejectKind::ConstantOutOfRange(bx))),
                };
                if self_name(chunk) != Some(name.as_str()) {
                    return Err(Reject::at(ip, RejectKind::NonSelfGlobal(name.clone())));
                }
                s[dst] = RegState::SelfFn;
            }
            OpCode::Call => {
                if read(&s, a)? != RegState::SelfFn {
                    return Err(Reject::at(ip, RejectKind::NonSelfCall));
                }
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
                let dst = reg_in_range(ip, c, num_regs)?;
                s[dst] = RegState::Val(assumed_ret);
                has_self_calls = true;
            }
            OpCode::Return => {
                let t = want(&s, a, ANY)?;
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

        let mut succs: Vec<usize> = Vec::with_capacity(2);
        if fallthrough {
            if ip + 1 >= len {
                return Err(Reject::at(ip, RejectKind::FallsOffEnd));
            }
            succs.push(ip + 1);
        }
        if let Some(t) = target {
            succs.push(t);
        }
        for succ in succs {
            let changed = match &mut states[succ] {
                slot @ None => {
                    *slot = Some(s.clone());
                    true
                }
                Some(existing) => {
                    let mut changed = false;
                    for (e, n) in existing.iter_mut().zip(s.iter()) {
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
    fn rejects_float_and_null() {
        let mut c = chunk("f", 0, 1);
        let k = c.add_constant(Constant::Float(1.5));
        c.emit(encode_abx(OpCode::LoadConst, 0, k), 1);
        c.emit(encode_abc(OpCode::Return, 0, 0, 0), 1);
        assert_eq!(
            verify(&c, &int_sig(0)).unwrap_err().kind,
            RejectKind::UnsupportedConstant("float")
        );
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
