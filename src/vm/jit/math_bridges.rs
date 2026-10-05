//! Pure numeric bridges called from JIT code.
//!
//! These implement float operations that Cranelift has no instruction for
//! (or whose instruction has different semantics, e.g. `fmin`/`fmax`
//! propagate NaN while Rust's `f64::min`/`max` return the other operand).
//! Each one is *the same Rust expression* the VM evaluates for that
//! operation (`semantics::float_op`, `stdlib::math::call_vm`), so results
//! are bit-identical, including NaN, infinities and signed zeros.
//!
//! They touch no VM state and cannot fail, so they are not "runtime
//! bridges" in the sense of `vm::jit::runtime` (no `record_jit_bridge_error`
//! duty). [`rt_int_pow`] is the one exception that can *deopt*: it writes
//! the context status itself, exactly like native code does.

use crate::vm::jit::tier::JitCtx;

/// Float `%` (`semantics::float_op`, `BinaryOp::Mod`).
pub extern "C" fn rt_fmod(a: f64, b: f64) -> f64 {
    a % b
}

/// `math.round` before the Int conversion (half away from zero).
pub extern "C" fn rt_round(x: f64) -> f64 {
    x.round()
}

pub extern "C" fn rt_sin(x: f64) -> f64 {
    x.sin()
}

pub extern "C" fn rt_cos(x: f64) -> f64 {
    x.cos()
}

pub extern "C" fn rt_tan(x: f64) -> f64 {
    x.tan()
}

/// `math.log` (natural logarithm).
pub extern "C" fn rt_ln(x: f64) -> f64 {
    x.ln()
}

pub extern "C" fn rt_powf(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

pub extern "C" fn rt_fmin(a: f64, b: f64) -> f64 {
    a.min(b)
}

pub extern "C" fn rt_fmax(a: f64, b: f64) -> f64 {
    a.max(b)
}

/// `math.pow(Int, Int)`: the Int result of `stdlib::math::int_pow`, or a
/// deopt (status 1) whenever that function would produce a Float
/// (negative exponent or overflow).
///
/// # Safety
/// `ctx` must point to the live `JitCtx` of the current native call.
pub unsafe extern "C" fn rt_int_pow(ctx: *mut JitCtx, base: i64, exp: i64) -> i64 {
    match crate::stdlib::math::int_pow(base, exp) {
        crate::stdlib::math::Num::Int(n) => n,
        crate::stdlib::math::Num::Float(_) => {
            // SAFETY: guaranteed by the caller (native code passes its own
            // context pointer).
            unsafe { (*ctx).status = 1 };
            0
        }
    }
}

/// Every bridge with its symbol name, for registration with the JIT module.
pub fn symbols() -> [(&'static str, *const u8); 10] {
    [
        ("forge_rt_fmod", rt_fmod as *const u8),
        ("forge_rt_round", rt_round as *const u8),
        ("forge_rt_sin", rt_sin as *const u8),
        ("forge_rt_cos", rt_cos as *const u8),
        ("forge_rt_tan", rt_tan as *const u8),
        ("forge_rt_ln", rt_ln as *const u8),
        ("forge_rt_powf", rt_powf as *const u8),
        ("forge_rt_fmin", rt_fmin as *const u8),
        ("forge_rt_fmax", rt_fmax as *const u8),
        ("forge_rt_int_pow", rt_int_pow as *const u8),
    ]
}
