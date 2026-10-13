//! The fast path of annotation checks (`TypeCheck`, V3 finding 21).
//!
//! Edition 2027 (and `--strict`) instrument annotated functions with calls
//! `__types.check(value, "<type>", "<context>")` / `__types.widen(value,
//! "<type>")` (`typechecker::enforce`). As a call each check costs a method
//! lookup, two native calls and a type-string lookup: about a microsecond,
//! which made small annotated functions ~3x slower. For the common types
//! the compiler emits, in front of that call:
//!
//! ```text
//!     <value> -> R(A)
//!     TypeCheck A, code, name      ; fits unchanged: continue; else skip 1
//!     Jump done
//!     <the original __types.check call, R(A) as its value> -> R(A)
//! done:
//! ```
//!
//! `TypeCheck` decides only "the value certainly fits and the check returns
//! it unchanged" (no Int → Float widening). Anything else, including every
//! violation, runs the original call, so errors and the rule itself stay
//! `types_check` / `semantics::types`. A code the VM decides here must
//! therefore never accept a value `fits_vm` rejects or `widen_vm` changes;
//! the tests in `type_check_tests` compare the two on many values.
//!
//! The code byte (`B`): a base kind in the low bits, plus [`ARRAY`] (an
//! array whose elements all have the base kind) and [`OPTION`] (or null).
//! `NAMED` reads the type name from string constant `C`.

use super::bytecode::Constant;
use super::gc::Gc;
use super::value::{ObjKind, Value};
use crate::semantics::types::RuntimeType;

pub(crate) const ANY: u8 = 0;
pub(crate) const INT: u8 = 1;
/// A Float value (an Int is widened, so it takes the slow path).
pub(crate) const FLOAT: u8 = 2;
pub(crate) const STRING: u8 = 3;
pub(crate) const BOOL: u8 = 4;
pub(crate) const NULL: u8 = 5;
/// An object that is not an `Option` value.
pub(crate) const OBJECT: u8 = 6;
/// An object tagged with the struct/ADT name in constant `C`.
pub(crate) const NAMED: u8 = 7;
pub(crate) const BASE_MASK: u8 = 0x0f;
pub(crate) const ARRAY: u8 = 0x40;
pub(crate) const OPTION: u8 = 0x80;

/// Is `code` a code [`encode`] can produce?
pub(crate) fn valid_code(code: u8) -> bool {
    let base = code & BASE_MASK;
    let flags = code & !BASE_MASK;
    if base > NAMED || flags & !(ARRAY | OPTION) != 0 {
        return false;
    }
    // Arrays only of scalars (`[Any]` included).
    flags & ARRAY == 0 || matches!(base, ANY | INT | FLOAT | STRING | BOOL)
}

/// The fast-path code of a canonical annotation type, with the struct name
/// for `NAMED`; `None` when the type always takes the call.
pub(crate) fn encode(ty: &RuntimeType) -> Option<(u8, Option<&str>)> {
    fn scalar(t: &RuntimeType) -> Option<u8> {
        Some(match t {
            RuntimeType::Int => INT,
            RuntimeType::Float => FLOAT,
            RuntimeType::String => STRING,
            RuntimeType::Bool => BOOL,
            _ => return None,
        })
    }
    fn plain(t: &RuntimeType) -> Option<(u8, Option<&str>)> {
        match t {
            RuntimeType::Null => Some((NULL, None)),
            RuntimeType::Object => Some((OBJECT, None)),
            // `Option` values are `Option`-tagged objects: never a struct.
            RuntimeType::Named(n) if n != "Option" => Some((NAMED, Some(n.as_str()))),
            RuntimeType::Array(elem) => match elem.as_ref() {
                RuntimeType::Any => Some((ARRAY | ANY, None)),
                e => scalar(e).map(|c| (ARRAY | c, None)),
            },
            t => scalar(t).map(|c| (c, None)),
        }
    }
    match ty {
        RuntimeType::Option(inner) => match inner.as_ref() {
            // `??T` and `?Null` are rare; keep them on the call.
            RuntimeType::Option(_) | RuntimeType::Null => None,
            t => plain(t).map(|(c, n)| (c | OPTION, n)),
        },
        t => plain(t),
    }
}

/// The `__type__` tag of an object value.
fn tag<'a>(gc: &'a Gc, map: &'a indexmap::IndexMap<String, Value>) -> Option<&'a str> {
    // Struct and ADT values are built with the tag first: no hashing.
    let v = match map.first() {
        Some((k, v)) if k == "__type__" => v,
        _ => map.get("__type__")?,
    };
    let r = v.as_obj()?;
    match &gc.get(r)?.kind {
        ObjKind::String(s) => Some(s.as_str()),
        _ => None,
    }
}

fn scalar_fits(gc: &Gc, value: Value, base: u8) -> bool {
    match base {
        ANY => true,
        INT => value.is_int(gc),
        FLOAT => value.as_float().is_some(),
        BOOL => value.as_bool().is_some(),
        NULL => value.is_null(),
        STRING => matches!(
            value.as_obj().and_then(|r| gc.get(r)).map(|o| &o.kind),
            Some(ObjKind::String(_))
        ),
        _ => false,
    }
}

/// Does `value` certainly fit the type `code` stands for, with nothing to
/// widen? `name` is constant `C` (read only for `NAMED`).
pub(crate) fn fits_unchanged(gc: &Gc, value: Value, code: u8, name: Option<&Constant>) -> bool {
    if code & OPTION != 0 && value.is_null() {
        return true;
    }
    let base = code & BASE_MASK;
    if code & ARRAY != 0 {
        return match value.as_obj().and_then(|r| gc.get(r)).map(|o| &o.kind) {
            Some(ObjKind::Array(items)) => {
                base == ANY || items.iter().all(|v| scalar_fits(gc, *v, base))
            }
            _ => false,
        };
    }
    match base {
        OBJECT | NAMED => match value.as_obj().and_then(|r| gc.get(r)).map(|o| &o.kind) {
            Some(ObjKind::Object(map)) => {
                let t = tag(gc, map);
                if base == OBJECT {
                    t != Some("Option")
                } else {
                    matches!(name, Some(Constant::Str(n)) if t == Some(n.as_str()))
                }
            }
            _ => false,
        },
        b => scalar_fits(gc, value, b),
    }
}

/// Scalar kinds as the JIT sees them (`jit::types::JitType`, without the
/// dependency on the `jit` feature).
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scalar {
    Int,
    Float,
    Bool,
}

/// Does [`fits_unchanged`] hold for *every* value of kind `kind`? (It is
/// false for every value of that kind otherwise: the JIT takes one path.)
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) fn accepts_kind(code: u8, kind: Scalar) -> bool {
    if code & ARRAY != 0 {
        return false;
    }
    matches!(
        (code & BASE_MASK, kind),
        (ANY, _) | (INT, Scalar::Int) | (FLOAT, Scalar::Float) | (BOOL, Scalar::Bool)
    )
}
