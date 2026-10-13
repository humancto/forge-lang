//! Local-variable access and the in-place update opcodes
//! (`AddLocal`, `PushLocal`, `PopLocal`, `UpdateSetLocal`,
//! `UpdateMapLocal`, and the container opcodes `GetIndexLocal` ..
//! `CallPure`).
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
//! * The bit is set only on a string/array/set that one of these opcodes
//!   has just *allocated* and stored into exactly one local-variable
//!   register, and only when that local has no open upvalue cell.
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

    /// Whether local `slot` may hold a uniquely owned object. Every local
    /// can: a captured local's register and its upvalue cell are one owner
    /// (every access goes through `read_local` / `write_local`, and
    /// `GetUpvalue` shares what it copies out of a cell).
    #[inline]
    fn local_can_own(&self, _frame_idx: usize, _slot: u8) -> bool {
        true
    }

    /// `BindUpvalue`: make local `slot` an alias of the closure's captured
    /// variable `uv_idx` (its cell becomes the local's open upvalue).
    pub(super) fn bind_upvalue(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        uv_idx: u8,
    ) -> Result<(), VMError> {
        let closure = self.frames[frame_idx].closure;
        let cell = match self.gc.get(closure).map(|o| &o.kind) {
            Some(ObjKind::Closure(c)) => c.upvalues.get(uv_idx as usize).copied(),
            _ => None,
        }
        .ok_or_else(|| VMError::new("invalid upvalue"))?;
        let value = match self.gc.get(cell).map(|o| &o.kind) {
            Some(ObjKind::Upvalue(uv)) => uv.value,
            _ => return Err(VMError::new("invalid upvalue")),
        };
        self.frames[frame_idx].open_upvalues.insert(slot, cell);
        // The register mirrors the cell (as for a captured local): not a
        // second reference.
        self.registers[base + slot as usize] = value;
        Ok(())
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

    /// `UpdateSetLocal`: statement `local.add(value)` (or `.remove`).
    /// In place when the local uniquely owns a set: the hash index makes
    /// that O(1) on average, where a copy is O(len).
    pub(super) fn update_set_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        value: Value,
        remove: bool,
    ) -> Result<(), VMError> {
        let cur = self.read_local(frame_idx, base, slot)?;
        if let Some(r) = self.owned_by_local(frame_idx, slot, cur) {
            // Look up with shared borrows: equality reads other heap
            // objects (strings, nested collections).
            let hash = value.set_hash(&self.gc);
            let present = match self.gc.get(r).map(|o| &o.kind) {
                Some(ObjKind::Set(items)) => {
                    Some(items.contains(hash, |item| item.set_eq(&value, &self.gc)))
                }
                _ => None,
            };
            match (present, remove) {
                (None, _) => {}
                (Some(false), true) | (Some(true), false) => return Ok(()),
                (Some(false), false) => {
                    // The stored value may be held elsewhere too (owned
                    // containers never hold an element with an owner bit).
                    self.gc.share(value);
                    let mut grown = 0;
                    if let Some(ObjKind::Set(items)) = self.gc.get_mut(r).map(|o| &mut o.kind) {
                        let before = items.heap_bytes();
                        items.push_unchecked(hash, value);
                        grown = items.heap_bytes().saturating_sub(before);
                    }
                    self.gc.note_growth(grown);
                    return Ok(());
                }
                (Some(true), true) => {
                    // Removing compares items too, so take the set out of
                    // the heap while it runs (nothing allocates or collects
                    // meanwhile, and no item refers to the set itself).
                    let Some(ObjKind::Set(items)) = self.gc.get_mut(r).map(|o| &mut o.kind) else {
                        return Err(VMError::new("BUG: owned set changed kind"));
                    };
                    let mut items = std::mem::take(items);
                    items.remove_matching(hash, |item| item.set_eq(&value, &self.gc));
                    if let Some(ObjKind::Set(slot)) = self.gc.get_mut(r).map(|o| &mut o.kind) {
                        *slot = items;
                    }
                    return Ok(());
                }
            }
        }
        let method = if remove { "remove" } else { "add" };
        self.method_mut_local(frame_idx, base, slot, cur, method, vec![value])
            .map(|_| ())
    }

    /// `UpdateMapLocal` set: `local = local.set(key, value)`, where
    /// `snapshot` is the local's value read before the key and value were
    /// evaluated (the generic call's receiver). In place when the local
    /// still holds that map and owns it: the hash index makes that O(1) on
    /// average, where `set` returns an O(len) copy.
    ///
    /// Nothing the key and value expressions can run updates an owned
    /// container of this local in place: the compiler admits no nested
    /// statements there, and other frames reach the local only through its
    /// upvalue cell, which the snapshot shares first when the key or value
    /// may call (`PEEK_SHARE_IF_CAPTURED`), so such an update copies. The
    /// same owned object therefore still has the snapshot's entries.
    pub(super) fn map_set_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        snapshot: Value,
        key: Value,
        value: Value,
    ) -> Result<(), VMError> {
        let cur = self.read_local(frame_idx, base, slot)?;
        if cur.as_obj().is_some() && cur.as_obj() == snapshot.as_obj() {
            if let Some(r) = self.owned_by_local(frame_idx, slot, cur) {
                if matches!(self.gc.get(r).map(|o| &o.kind), Some(ObjKind::Map(_))) {
                    // Stored values may be held elsewhere too (owned
                    // containers never hold an element with an owner bit).
                    self.gc.share(key);
                    self.gc.share(value);
                    let grown = self.with_owned_map(r, |entries, gc| {
                        let before = entries.heap_bytes();
                        entries.insert_in(key, value, gc);
                        entries.heap_bytes().saturating_sub(before)
                    })?;
                    self.gc.note_growth(grown);
                    return Ok(());
                }
            }
        }
        // Generic path: `snapshot.set(key, value)`, stored into the local.
        self.gc.share(snapshot);
        let receiver_is_map = matches!(
            snapshot
                .as_obj()
                .and_then(|r| self.gc.get(r))
                .map(|o| &o.kind),
            Some(ObjKind::Map(_))
        );
        let method = self.alloc_string("set");
        let result = self.call_native("__forge_call_method", vec![snapshot, method, key, value])?;
        self.store_fresh_map(frame_idx, base, slot, receiver_is_map, snapshot, result);
        Ok(())
    }

    /// `UpdateMapLocal` remove: `local = local.remove(key)`. The key was
    /// evaluated before the local is read (as in the generic
    /// `__forge_method_mut` lowering). In place when the local owns a map.
    pub(super) fn map_remove_local(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        key: Value,
    ) -> Result<(), VMError> {
        let cur = self.read_local(frame_idx, base, slot)?;
        if let Some(r) = self.owned_by_local(frame_idx, slot, cur) {
            if matches!(self.gc.get(r).map(|o| &o.kind), Some(ObjKind::Map(_))) {
                self.with_owned_map(r, |entries, gc| entries.remove_in(&key, gc))?;
                return Ok(());
            }
        }
        // Generic path, exactly the lowering of `local = local.remove(k)`:
        // `__forge_method_mut` (whose new receiver is the result for every
        // value that has an in-place `remove`), then the assignment.
        let receiver_is_map = matches!(
            cur.as_obj().and_then(|r| self.gc.get(r)).map(|o| &o.kind),
            Some(ObjKind::Map(_) | ObjKind::Set(_))
        );
        let (_, result) = self.method_mut_pair(cur, "remove", vec![key])?;
        self.store_fresh_map(frame_idx, base, slot, receiver_is_map, cur, result);
        Ok(())
    }

    /// Run `f` on the entries of the owned map `r` with the heap available
    /// for hashing and comparing keys: the entries are taken out of the
    /// heap while it runs (nothing allocates or collects meanwhile, and no
    /// key or value refers to the map itself).
    fn with_owned_map<R>(
        &mut self,
        r: GcRef,
        f: impl FnOnce(&mut super::value::VmMap, &super::gc::Gc) -> R,
    ) -> Result<R, VMError> {
        let Some(ObjKind::Map(slot)) = self.gc.get_mut(r).map(|o| &mut o.kind) else {
            return Err(VMError::new("BUG: owned map changed kind"));
        };
        let mut entries = std::mem::take(slot);
        let out = f(&mut entries, &self.gc);
        if let Some(ObjKind::Map(slot)) = self.gc.get_mut(r).map(|o| &mut o.kind) {
            *slot = entries;
        }
        Ok(out)
    }

    /// Store the result of a copying map update into the local. When the
    /// receiver was a plain map (or set) the result is a map (or set) the
    /// call just allocated, referenced by nothing else, so the local owns
    /// it from now on.
    fn store_fresh_map(
        &mut self,
        frame_idx: usize,
        base: usize,
        slot: u8,
        receiver_is_collection: bool,
        receiver: Value,
        result: Value,
    ) {
        if let Some(r) = result.as_obj() {
            let fresh = receiver_is_collection
                && Some(r) != receiver.as_obj()
                && matches!(
                    self.gc.get(r).map(|o| &o.kind),
                    Some(ObjKind::Map(_) | ObjKind::Set(_))
                );
            if fresh && self.local_can_own(frame_idx, slot) {
                self.gc.set_unique_container(r);
            }
        }
        self.write_local(frame_idx, base, slot, result);
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
        // A different array or set than `cur` was just allocated by the
        // copy; it is referenced only by the (now unreachable) pair and the
        // local. The callers either discard `result` (`push`, `add` and
        // `remove`, whose result is the receiver) or return an element
        // (`pop`), never the receiver.
        if let Some(r) = new_receiver.as_obj() {
            let fresh_array = Some(r) != cur.as_obj()
                && matches!(
                    self.gc.get(r).map(|o| &o.kind),
                    Some(ObjKind::Array(_) | ObjKind::Set(_))
                );
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
