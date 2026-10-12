//! Local-variable access and the in-place update opcodes
//! (`AddLocal`, `PushLocal`, `PopLocal`, and the container opcodes
//! `GetIndexLocal` .. `CallPure`).
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
//! The same bit makes `local[k] = v` / `local.f = v` (`SetIndexLocal` /
//! `SetFieldLocal`) update an owned array or object in place. Reading an
//! element (`GetIndexLocal` / `GetFieldLocal`) and passing the local to a
//! read-only builtin (`PeekLocal` + `CallPure`: `len`, `get`, `has_key`,
//! `contains`) do not copy the local's reference, so they keep it owned;
//! the element or result they produce is shared instead.
//!
//! # Owned elements (`GcObject::slot_owned`)
//!
//! `local[k].push(v)` / `local.f.push(v)` (`PushIndexLocal` /
//! `PushFieldLocal`) also need to know that the inner array has one
//! reference. `slot_owned` marks an array that one of these opcodes copied
//! and stored into an element of a container owned by a local; it is only
//! trusted through that owned container. The invariants:
//!
//! * a container becomes owned only as a fresh copy, and
//!   `Gc::set_unique_container` clears its elements' `slot_owned` bits
//!   first (the elements are still referenced by the original);
//! * every value an in-place opcode stores into an owned container
//!   (`SetIndexLocal`, `PushLocal`, `AddLocal`, `PushIndexLocal`) is
//!   shared first, so a stale bit on it never becomes trusted;
//! * every element read out of an owned container without sharing the
//!   container (`GetIndexLocal`, `GetFieldLocal`, `CallPure` results,
//!   `PopLocal`) is shared;
//! * any other read of an element first copies the container's reference
//!   out of the local (`GetLocal` etc.), which makes the container
//!   unowned, so its elements' bits are no longer trusted.
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

        let outcome = crate::semantics::binary(
            crate::semantics::BinaryOp::Add,
            Self::semantic_operand(&self.gc, &cur),
            Self::semantic_operand(&self.gc, &rhs),
            self.edition(),
        );
        match outcome {
            Ok(crate::semantics::Outcome::Concat) => {}
            Ok(crate::semantics::Outcome::ArrayConcat) => {
                return self.add_local_array(frame_idx, base, slot, cur, rhs)
            }
            _ => {
                let result = self.arith_op(&cur, &rhs, super::bytecode::OpCode::Add)?;
                self.write_local(frame_idx, base, slot, result);
                return Ok(());
            }
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

    /// `AddLocal` on two arrays: `xs = xs + ys` appends in place when the
    /// local owns its array, otherwise stores a fresh (owned) copy.
    fn add_local_array(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        cur: Value,
        rhs: Value,
    ) -> Result<(), VMError> {
        if let Some(r) = self.owned_by_local(frame_idx, slot, cur) {
            let more = self.concat_operand_items(&rhs).to_vec();
            // The appended elements are now also referenced by `rhs`.
            for v in &more {
                self.gc.share(*v);
            }
            let caps = self.caps;
            let mut grown = None;
            if let Some(obj) = self.gc.get_mut(r) {
                if let ObjKind::Array(items) = &mut obj.kind {
                    caps.check_collection(items.len() + more.len())
                        .map_err(|m| VMError::new(&m))?;
                    let before = items.capacity();
                    items.extend(more);
                    grown = Some((items.capacity() - before) * std::mem::size_of::<Value>());
                }
            }
            if let Some(bytes) = grown {
                self.gc.note_growth(bytes);
                return Ok(());
            }
        }
        let items = self.concat_array_items(&cur, &rhs)?;
        let r = self.gc.alloc(ObjKind::Array(items));
        if self.local_can_own(frame_idx, slot) {
            self.gc.set_unique_container(r);
        }
        self.write_local(frame_idx, base, slot, Value::obj(r));
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
            self.gc.share(value);
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
                    let popped = items.pop().unwrap_or(Value::null());
                    self.gc.share(popped);
                    return Ok(popped);
                }
            }
        }
        let popped = self.method_mut_local(frame_idx, base, slot, cur, "pop", Vec::new())?;
        self.gc.share(popped);
        Ok(popped)
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
        let (new_receiver, result) = self.method_mut_pair(cur, method, rest)?;
        // A different array than `cur` was just allocated by the copy; it is
        // referenced only by the (now unreachable) pair and the local. The
        // callers either discard `result` (`push`, whose result is the
        // receiver) or return an element (`pop`), never the receiver.
        if let Some(r) = new_receiver.as_obj() {
            let fresh_array = Some(r) != cur.as_obj()
                && matches!(self.gc.get(r).map(|o| &o.kind), Some(ObjKind::Array(_)));
            if fresh_array && self.local_can_own(frame_idx, slot) {
                self.gc.set_unique_container(r);
            }
        }
        self.write_local(frame_idx, base, slot, new_receiver);
        Ok(result)
    }

    /// `__forge_method_mut(receiver, method, rest...)`: the updated receiver
    /// (the receiver itself when the method has no in-place form) and the
    /// method's result.
    pub(super) fn method_mut_pair(
        &mut self,
        receiver: Value,
        method: &str,
        rest: Vec<Value>,
    ) -> Result<(Value, Value), VMError> {
        let mut args = Vec::with_capacity(2 + rest.len());
        args.push(receiver);
        args.push(self.alloc_string(method));
        args.extend(rest);
        let pair = self.call_native("__forge_method_mut", args)?;
        match pair.as_obj().and_then(|r| self.gc.get(r)).map(|o| &o.kind) {
            Some(ObjKind::Tuple(items)) if items.len() == 2 => Ok((items[0], items[1])),
            _ => Err(VMError::new("BUG: __forge_method_mut returned a non-pair")),
        }
    }

    /// `GetIndexLocal` / `GetFieldLocal`: read one element of a local's
    /// container without sharing the container. The element is shared: a
    /// register now refers to it too.
    pub(super) fn get_element_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        key: ElemKey<'_>,
    ) -> Result<Value, VMError> {
        let container = self.read_local(frame_idx, base, slot)?;
        let value = self.element_get(container, &key)?;
        self.gc.share(value);
        Ok(value)
    }

    /// `container[index]` / `container.field` with the ordinary read rules.
    fn element_get(&mut self, container: Value, key: &ElemKey<'_>) -> Result<Value, VMError> {
        match key {
            ElemKey::Index(index) => self.index_get(container, *index),
            ElemKey::Field(field) => self.get_field(container, field),
        }
    }

    /// `container[index] = value` / `container.field = value` with the
    /// ordinary rules: an updated copy.
    fn element_set(
        &mut self,
        container: Value,
        key: &ElemKey<'_>,
        value: Value,
    ) -> Result<Value, VMError> {
        match key {
            ElemKey::Index(index) => self.index_set(container, *index, value),
            ElemKey::Field(field) => self.set_field(container, field, value),
        }
    }

    /// Where `key` names an element of the plain array or object `r` (no
    /// frozen wrapper): the only cases the in-place paths handle, all of
    /// which the copying path performs without error. With `insert`, a
    /// missing object key is a position too (assignment adds it).
    fn element_position(&self, r: GcRef, key: &ElemKey<'_>, insert: bool) -> Option<Position> {
        let obj = self.gc.get(r)?;
        match (&obj.kind, key) {
            (ObjKind::Array(items), ElemKey::Index(index)) => {
                let i = index.as_int(&self.gc)?;
                crate::semantics::normalize_index(i, items.len()).map(Position::Element)
            }
            (ObjKind::Object(map), ElemKey::Index(index)) => {
                let key = self.get_str_ref(index)?;
                (insert || map.contains_key(key)).then(|| Position::Key(key.to_string()))
            }
            (ObjKind::Object(map), ElemKey::Field(field)) => {
                (insert || map.contains_key(*field)).then(|| Position::Key((*field).to_string()))
            }
            _ => None,
        }
    }

    /// The value at `pos` in container `r`.
    fn element_at(&self, r: GcRef, pos: &Position) -> Option<Value> {
        match (&self.gc.get(r)?.kind, pos) {
            (ObjKind::Array(items), Position::Element(i)) => items.get(*i).copied(),
            (ObjKind::Object(map), Position::Key(key)) => map.get(key).copied(),
            _ => None,
        }
    }

    /// Store `value` at `pos` inside the owned container `r`, accounting
    /// for growth.
    fn store_at(&mut self, r: GcRef, pos: Position, value: Value) {
        let mut grown = 0;
        if let Some(obj) = self.gc.get_mut(r) {
            match (&mut obj.kind, pos) {
                (ObjKind::Array(items), Position::Element(i)) => {
                    if let Some(slot) = items.get_mut(i) {
                        *slot = value;
                    }
                }
                (ObjKind::Object(map), Position::Key(key)) => {
                    let entry = key.capacity()
                        + std::mem::size_of::<String>()
                        + std::mem::size_of::<Value>()
                        + 2 * std::mem::size_of::<usize>();
                    if map.insert(key, value).is_none() {
                        grown = entry;
                    }
                }
                _ => {}
            }
        }
        if grown > 0 {
            self.gc.note_growth(grown);
        }
    }

    /// Replace the local's container with `updated`, a fresh copy made by
    /// the copying path, which the local owns from now on (when it can).
    fn store_container_copy(&mut self, frame_idx: usize, base: usize, slot: u8, updated: Value) {
        if let Some(r) = updated.as_obj() {
            if self.local_can_own(frame_idx, slot) {
                self.gc.set_unique_container(r);
            }
        }
        self.write_local(frame_idx, base, slot, updated);
    }

    /// `SetIndexLocal` / `SetFieldLocal`: `local[key] = value` on a mutable
    /// local. In place when the local owns its container and `key` names a
    /// position (see `element_position`); otherwise exactly `SetIndex` /
    /// `SetField` followed by `SetLocal`, and the fresh copy becomes owned.
    pub(super) fn set_element_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        key: ElemKey<'_>,
        value: Value,
    ) -> Result<(), VMError> {
        let cur = self.read_local(frame_idx, base, slot)?;
        if let Some(r) = self.owned_by_local(frame_idx, slot, cur) {
            if let Some(pos) = self.element_position(r, &key, true) {
                // The stored value may be held elsewhere too.
                self.gc.share(value);
                self.store_at(r, pos, value);
                return Ok(());
            }
        }
        let updated = self.element_set(cur, &key, value)?;
        self.store_container_copy(frame_idx, base, slot, updated);
        Ok(())
    }

    /// `PushIndexLocal` / `PushFieldLocal`: statement `local[key].push(value)`
    /// on a mutable local.
    ///
    /// In place when the local owns its container, `key` names an existing
    /// array element and the container owns that array
    /// (`GcObject::slot_owned`). When only the container is owned, the
    /// array is copied once with `value` appended, stored in place and owned
    /// by the container from then on. Otherwise this is exactly the generic
    /// lowering: read the element, `__forge_method_mut`, and when the
    /// element changed store it back with `SetIndex` / `SetField` and
    /// `SetLocal` (the fresh container copy becomes owned, and so does the
    /// new array inside it).
    pub(super) fn push_element_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        key: ElemKey<'_>,
        value: Value,
    ) -> Result<(), VMError> {
        let cur = self.read_local(frame_idx, base, slot)?;
        if let Some(container) = self.owned_by_local(frame_idx, slot, cur) {
            if let Some(pos) = self.element_position(container, &key, false) {
                if let Some(element) = self.element_at(container, &pos).and_then(|v| v.as_obj()) {
                    if self.push_owned_array(container, pos, element, value)? {
                        return Ok(());
                    }
                }
            }
        }
        let element = self.element_get(cur, &key)?;
        let (updated, _) = self.method_mut_pair(element, "push", vec![value])?;
        if updated.as_obj().is_none() || updated.as_obj() == element.as_obj() {
            // No in-place form (an object's own `push`, ...): nothing to store.
            return Ok(());
        }
        let updated_container = self.element_set(cur, &key, updated)?;
        let owned = self.local_can_own(frame_idx, slot);
        self.store_container_copy(frame_idx, base, slot, updated_container);
        if owned {
            if let Some(r) = updated.as_obj() {
                if matches!(self.gc.get(r).map(|o| &o.kind), Some(ObjKind::Array(_))) {
                    // A fresh copy: its elements are shared with the
                    // original array.
                    self.gc.set_unique_container(r);
                    if let Some(obj) = self.gc.get_mut(r) {
                        obj.unique = false;
                    }
                    self.gc.set_slot_owned(r);
                }
            }
        }
        Ok(())
    }

    /// The owned-container paths of `push_element_local` for `element`,
    /// stored at `pos` in the owned `container`. Returns `false` (nothing
    /// done) when `element` is not a plain array.
    fn push_owned_array(
        &mut self,
        container: GcRef,
        pos: Position,
        element: GcRef,
        value: Value,
    ) -> Result<bool, VMError> {
        let len = match self.gc.get(element).map(|o| &o.kind) {
            Some(ObjKind::Array(items)) => items.len(),
            _ => return Ok(false),
        };
        self.caps
            .check_collection(len + 1)
            .map_err(|m| VMError::new(&m))?;
        // The pushed value may be held elsewhere too.
        self.gc.share(value);
        if self.gc.is_slot_owned(element) {
            let mut grown = 0;
            if let Some(obj) = self.gc.get_mut(element) {
                if let ObjKind::Array(items) = &mut obj.kind {
                    let before = items.capacity();
                    items.push(value);
                    grown = (items.capacity() - before) * std::mem::size_of::<Value>();
                }
            }
            self.gc.note_growth(grown);
            return Ok(true);
        }
        // The array may be shared: copy it once; the container owns the copy.
        let mut items = match self.gc.get(element).map(|o| &o.kind) {
            Some(ObjKind::Array(items)) => items.clone(),
            _ => return Ok(false),
        };
        items.push(value);
        let copy = self.gc.alloc(ObjKind::Array(items));
        // Its elements are still referenced by the original array.
        self.gc.set_unique_container(copy);
        if let Some(obj) = self.gc.get_mut(copy) {
            // Owned by the container element, not by a local register.
            obj.unique = false;
            obj.slot_owned = true;
        }
        self.store_at(container, pos, Value::obj(copy));
        Ok(true)
    }
}

/// The element a container opcode on a local addresses: `[index]` or
/// `.field`.
pub(super) enum ElemKey<'a> {
    Index(Value),
    Field(&'a str),
}

/// A resolved element position inside a plain array or object.
enum Position {
    Element(usize),
    Key(String),
}
