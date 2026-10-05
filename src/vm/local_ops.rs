//! Local-variable access and the in-place update opcodes
//! (`AddLocal`, `PushLocal`, `PopLocal`).
//!
//! # Single-owner updates
//!
//! Strings and collections have value semantics: `s = s + "x"` and
//! `xs.push(v)` must not be observable through any other binding that
//! refers to the old value. Copying the whole value on every update keeps
//! that promise but makes building a string or an array in a loop O(n²).
//!
//! The VM's GC is mark-sweep (no reference counts), so ownership is tracked
//! structurally with `GcObject::unique`:
//!
//! * The bit is set only on a string/array that one of these opcodes has
//!   just *allocated* and stored into exactly one local-variable register,
//!   and only when that local has no open upvalue cell.
//! * Every way a reference can leave a local register clears it:
//!   `GetLocal` and `Move` (the only opcodes that copy a local's register
//!   into another register) and closure capture (`Closure` with a local
//!   upvalue source). The compiler never passes a local's register directly
//!   as an operand to any other opcode.
//! * So while the bit is set, the local's register is the *only* live
//!   reference, and updating the object in place is indistinguishable from
//!   replacing it with an updated copy.
//!
//! When the bit is not set (the value may be shared) these opcodes fall
//! back to exactly the copying behaviour of `GetLocal` + `Add` + `SetLocal`
//! / `__forge_method_mut`, and the fresh copy they store becomes the
//! uniquely owned object for the next update. The first update after any
//! sharing therefore copies once; subsequent updates are amortized O(1).

use super::machine::{VMError, VM};
use super::value::{GcRef, ObjKind, ObjUpvalue, Value};

impl VM {
    /// Read local `slot` of the frame at `frame_idx` (`GetLocal`
    /// semantics): a captured local lives in its open upvalue cell, which is
    /// synced back into the register.
    #[inline]
    pub(super) fn read_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
    ) -> Result<Value, VMError> {
        let open = &self.frames[frame_idx].open_upvalues;
        if open.is_empty() {
            return Ok(self.registers[base + slot as usize]);
        }
        let Some(uv_ref) = open.get(&slot).copied() else {
            return Ok(self.registers[base + slot as usize]);
        };
        let value = self
            .gc
            .get(uv_ref)
            .and_then(|uv_obj| match &uv_obj.kind {
                ObjKind::Upvalue(uv) => Some(uv.value),
                _ => None,
            })
            .ok_or_else(|| VMError::new("invalid open upvalue"))?;
        self.registers[base + slot as usize] = value;
        Ok(value)
    }

    /// Write local `slot` (`SetLocal` semantics): the register and, when the
    /// local is captured, its open upvalue cell.
    #[inline]
    pub(super) fn write_local(&mut self, frame_idx: usize, base: usize, slot: u8, val: Value) {
        self.registers[base + slot as usize] = val;
        let open = &self.frames[frame_idx].open_upvalues;
        if open.is_empty() {
            return;
        }
        if let Some(uv_ref) = open.get(&slot).copied() {
            if let Some(uv_obj) = self.gc.get_mut(uv_ref) {
                if let ObjKind::Upvalue(ObjUpvalue { value }) = &mut uv_obj.kind {
                    *value = val;
                }
            }
        }
    }

    /// Whether local `slot` may hold a uniquely owned object: a captured
    /// local's value is also referenced by its upvalue cell.
    #[inline]
    fn local_can_own(&self, frame_idx: usize, slot: u8) -> bool {
        !self.frames[frame_idx].open_upvalues.contains_key(&slot)
    }

    /// The object `value` refers to, when it is uniquely owned by local
    /// `slot`.
    #[inline]
    fn owned_by_local(&self, frame_idx: usize, slot: u8, value: Value) -> Option<GcRef> {
        let r = value.as_obj()?;
        (self.gc.is_unique(r) && self.local_can_own(frame_idx, slot)).then_some(r)
    }

    /// `AddLocal`: `local = local + rhs`.
    pub(super) fn add_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        rhs: Value,
    ) -> Result<(), VMError> {
        let cur = self.read_local(frame_idx, base, slot)?;

        // Inline integers: the common loop-counter case.
        if let (Some(x), Some(y)) = (cur.as_inline_int(), rhs.as_inline_int()) {
            if let Some(v) = x.checked_add(y).and_then(Value::try_inline_int) {
                self.write_local(frame_idx, base, slot, v);
                return Ok(());
            }
        }

        let concat = matches!(
            crate::semantics::binary(
                crate::semantics::BinaryOp::Add,
                Self::semantic_operand(&self.gc, &cur),
                Self::semantic_operand(&self.gc, &rhs),
            ),
            Ok(crate::semantics::Outcome::Concat)
        );
        if !concat {
            let result = self.arith_op(&cur, &rhs, super::bytecode::OpCode::Add)?;
            self.write_local(frame_idx, base, slot, result);
            return Ok(());
        }

        let suffix = rhs.display(&self.gc);
        if let Some(r) = self.owned_by_local(frame_idx, slot, cur) {
            let caps = self.caps;
            let mut grown = None;
            if let Some(obj) = self.gc.get_mut(r) {
                if let ObjKind::String(s) = &mut obj.kind {
                    caps.check_string(s.len() + suffix.len())
                        .map_err(|m| VMError::new(&m))?;
                    let before = s.capacity();
                    s.push_str(&suffix);
                    grown = Some(s.capacity() - before);
                }
            }
            if let Some(bytes) = grown {
                self.gc.note_growth(bytes);
                return Ok(());
            }
        }
        let mut text = cur.display(&self.gc);
        self.caps
            .check_string(text.len() + suffix.len())
            .map_err(|m| VMError::new(&m))?;
        text.push_str(&suffix);
        let result = if self.local_can_own(frame_idx, slot) {
            Value::obj(self.gc.alloc_unique_string(text))
        } else {
            Value::obj(self.gc.alloc_string(text))
        };
        self.write_local(frame_idx, base, slot, result);
        Ok(())
    }

    /// `PushLocal`: statement `local.push(value)`.
    pub(super) fn push_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        value: Value,
    ) -> Result<(), VMError> {
        let cur = self.read_local(frame_idx, base, slot)?;
        if let Some(r) = self.owned_by_local(frame_idx, slot, cur) {
            let caps = self.caps;
            let mut grown = None;
            if let Some(obj) = self.gc.get_mut(r) {
                if let ObjKind::Array(items) = &mut obj.kind {
                    caps.check_collection(items.len() + 1)
                        .map_err(|m| VMError::new(&m))?;
                    let before = items.capacity();
                    items.push(value);
                    grown = Some((items.capacity() - before) * std::mem::size_of::<Value>());
                }
            }
            if let Some(bytes) = grown {
                self.gc.note_growth(bytes);
                return Ok(());
            }
        }
        self.method_mut_local(frame_idx, base, slot, cur, "push", vec![value])
            .map(|_| ())
    }

    /// `PopLocal`: `local.pop()`, returning the popped value.
    pub(super) fn pop_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
    ) -> Result<Value, VMError> {
        let cur = self.read_local(frame_idx, base, slot)?;
        if let Some(r) = self.owned_by_local(frame_idx, slot, cur) {
            if let Some(obj) = self.gc.get_mut(r) {
                if let ObjKind::Array(items) = &mut obj.kind {
                    return Ok(items.pop().unwrap_or(Value::null()));
                }
            }
        }
        self.method_mut_local(frame_idx, base, slot, cur, "pop", Vec::new())
    }

    /// Shared (copying) path: run `__forge_method_mut` exactly as the
    /// generic lowering does, store the new receiver into the local and
    /// return the method's result. A freshly allocated receiver array is
    /// owned by the local from now on.
    fn method_mut_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        cur: Value,
        method: &str,
        rest: Vec<Value>,
    ) -> Result<Value, VMError> {
        let mut args = Vec::with_capacity(2 + rest.len());
        args.push(cur);
        args.push(self.alloc_string(method));
        args.extend(rest);
        let pair = self.call_native("__forge_method_mut", args)?;
        let (new_receiver, result) = match pair.as_obj().and_then(|r| self.gc.get(r)) {
            Some(obj) => match &obj.kind {
                ObjKind::Tuple(items) if items.len() == 2 => (items[0], items[1]),
                _ => return Err(VMError::new("BUG: __forge_method_mut returned a non-pair")),
            },
            None => return Err(VMError::new("BUG: __forge_method_mut returned a non-pair")),
        };
        // A different array than `cur` was just allocated by the copy; it is
        // referenced only by the (now unreachable) pair and the local. The
        // callers either discard `result` (`push`, whose result is the
        // receiver) or return an element (`pop`), never the receiver.
        if let Some(r) = new_receiver.as_obj() {
            let fresh_array = Some(r) != cur.as_obj()
                && matches!(self.gc.get(r).map(|o| &o.kind), Some(ObjKind::Array(_)));
            if fresh_array && self.local_can_own(frame_idx, slot) {
                self.gc.set_unique(r);
            }
        }
        self.write_local(frame_idx, base, slot, new_receiver);
        Ok(result)
    }
}
