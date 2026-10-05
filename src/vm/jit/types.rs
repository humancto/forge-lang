//! Identity and type vocabulary shared by the verifier, IR builder and the
//! dispatch tier.

use crate::vm::bytecode::Chunk;
use crate::vm::gc::Gc;
use crate::vm::value::Value;
use std::fmt;

/// Identity of one function prototype.
///
/// This is `Chunk::proto_id`: unique per prototype for the life of the
/// process and shared by every closure instantiated from that prototype
/// (closure creation clones the prototype chunk). Two distinct functions
/// that happen to share a name always get distinct ids. The tier
/// additionally validates that a chunk presented under a known id has the
/// same code as the one it compiled (`tier::same_code`), so an id can never
/// select native code for different bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FnId(pub u64);

impl FnId {
    pub fn of(chunk: &Chunk) -> Self {
        FnId(chunk.proto_id)
    }
}

/// A value kind a specialization may assume for a register or argument.
///
/// Only kinds whose VM semantics are fully implemented by the verifier and
/// IR builder appear here. Future tiers (Str, ...) extend this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JitType {
    /// A signed 64-bit integer (inline or `BoxedInt` in the VM).
    Int,
    /// A boolean, represented natively as 0 / 1.
    Bool,
    /// An IEEE-754 double, represented natively as an `f64` (and as its bit
    /// pattern where the ABI passes `i64`s).
    Float,
}

impl JitType {
    /// Classify a runtime value. `None` means no specialization can accept
    /// it (the entry guard fails and the call runs in the VM).
    pub fn of_value(v: &Value, gc: &Gc) -> Option<JitType> {
        if v.as_bool().is_some() {
            return Some(JitType::Bool);
        }
        if v.as_int(gc).is_some() {
            return Some(JitType::Int);
        }
        if v.as_float().is_some() {
            return Some(JitType::Float);
        }
        None
    }

    /// Int or Float: an operand of arithmetic and ordering.
    pub fn is_numeric(self) -> bool {
        matches!(self, JitType::Int | JitType::Float)
    }

    /// Native (i64) encoding of a value already known to be of this kind.
    pub fn encode(self, v: &Value, gc: &Gc) -> Option<i64> {
        match self {
            JitType::Int => v.as_int(gc),
            JitType::Bool => v.as_bool().map(i64::from),
            JitType::Float => v.as_float().map(|f| f.to_bits() as i64),
        }
    }

    /// Re-box a native result of this kind into a VM value.
    pub fn decode(self, raw: i64, gc: &mut Gc) -> Value {
        match self {
            JitType::Int => Value::int(raw, gc),
            JitType::Bool => Value::bool_val(raw != 0),
            // `Value::float` canonicalizes NaNs exactly as the VM's own
            // arithmetic results are boxed.
            JitType::Float => Value::float(f64::from_bits(raw as u64)),
        }
    }
}

impl fmt::Display for JitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JitType::Int => write!(f, "Int"),
            JitType::Bool => write!(f, "Bool"),
            JitType::Float => write!(f, "Float"),
        }
    }
}

/// The argument-kind signature a specialization is compiled for. Entry guards
/// require the runtime arguments to match it exactly (including arity).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeSig(pub Vec<JitType>);

impl TypeSig {
    /// Signature of a concrete argument list, or `None` if any argument has a
    /// kind no specialization supports.
    pub fn of_args(args: &[Value], gc: &Gc) -> Option<TypeSig> {
        args.iter()
            .map(|v| JitType::of_value(v, gc))
            .collect::<Option<Vec<_>>>()
            .map(TypeSig)
    }

    pub fn arity(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Display for TypeSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        for (i, t) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", t)?;
        }
        write!(f, ")")
    }
}
