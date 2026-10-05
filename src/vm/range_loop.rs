//! Counting `for` loops: `for v in range(..)` and `repeat n times`.
//!
//! `repeat n times { }` is parsed as `for _ in range(n) { }`, and both used
//! to materialize the whole range as an array and walk it with
//! `IterHas`/`IterGet` (one allocation per element plus two dispatches per
//! iteration). The compiler now emits
//!
//! ```text
//!     R(f) = <range callee>; R(f+1..) = <arguments>
//!     ForRangePrep f, argc, mode
//!     JumpIfTrue   mode -> fast
//!     <generic: call R(f), iterate the result with IterHas/IterGet> -> body
//! fast:
//!     ForRangeNext f, var
//!     Jump         exit
//! body:
//!     ...
//! step:                                  (`continue` lands here)
//!     JumpIfFalse  mode -> generic_step
//!     Loop         fast
//! generic_step:
//!     index += 1; Loop generic head
//! exit:
//! ```
//!
//! so the body is compiled once and the common case costs two dispatches
//! per iteration, like a hand-written `while` loop.
//!
//! # Exactness
//!
//! The counting path is taken only when the callee *at run time* is the
//! builtin `range` (a `NativeFunction` named `range`, which `dispatch_native`
//! always routes to the builtin) called with one or two Int arguments. In
//! that case the builtin would return exactly `start, start+1, .., end-1`
//! (a third argument is accepted by the builtin's arity but ignored, so the
//! compiler sends three-argument calls down the generic path). Any other
//! callee — a user function or local named `range`, a closure, a value with
//! `__call__` — or any non-Int argument goes down the generic path, which is
//! the old code: same call, same errors, same iteration. The loop variable
//! is a copy of the hidden counter, so assigning to it in the body cannot
//! change the iteration, exactly as with the materialized array.

use super::machine::{VMError, VM};
use super::value::{ObjKind, Value};

impl VM {
    /// `ForRangePrep A, B, C`. See `OpCode::ForRangePrep`.
    #[inline]
    pub(super) fn for_range_prep(&mut self, base: usize, a: u8, argc: u8, mode: u8) {
        let f = base + a as usize;
        let fast = self.range_bounds(f, argc);
        if let Some((start, end)) = fast {
            self.registers[f] = Value::int(start, &mut self.gc);
            self.registers[f + 1] = Value::int(end, &mut self.gc);
        }
        self.registers[base + mode as usize] = Value::bool_val(fast.is_some());
    }

    /// `(start, end)` when `R(f)(R(f+1), ..)` is a call of the builtin
    /// `range` with Int arguments.
    fn range_bounds(&self, f: usize, argc: u8) -> Option<(i64, i64)> {
        let callee = self.registers[f].as_obj()?;
        match self.gc.get(callee).map(|o| &o.kind) {
            Some(ObjKind::NativeFunction(nf)) if nf.name == "range" => {}
            _ => return None,
        }
        let arg = |i: usize| self.registers[f + 1 + i].as_int(&self.gc);
        match argc {
            1 => Some((0, arg(0)?)),
            2 => Some((arg(0)?, arg(1)?)),
            _ => None,
        }
    }

    /// `ForRangeNext A, B`: returns true when the loop continues (the
    /// caller then skips the following exit jump).
    #[inline]
    pub(super) fn for_range_next(&mut self, base: usize, a: u8, var: u8) -> Result<bool, VMError> {
        let f = base + a as usize;
        // Both registers were written by `ForRangePrep` (or by this opcode)
        // and are invisible to user code, so they are always Ints.
        let (cur, end) = match (
            self.registers[f].as_inline_int(),
            self.registers[f + 1].as_inline_int(),
        ) {
            (Some(c), Some(e)) => (c, e),
            _ => match (
                self.registers[f].as_int(&self.gc),
                self.registers[f + 1].as_int(&self.gc),
            ) {
                (Some(c), Some(e)) => (c, e),
                _ => return Err(VMError::new("BUG: ForRangeNext on a non-integer counter")),
            },
        };
        if cur >= end {
            return Ok(false);
        }
        let value = Value::int(cur, &mut self.gc);
        self.registers[base + var as usize] = value;
        // `cur < end <= i64::MAX`, so this cannot overflow.
        self.registers[f] = Value::int(cur + 1, &mut self.gc);
        Ok(true)
    }
}
