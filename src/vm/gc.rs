//! Mark-sweep garbage collector for the bytecode VM.
//!
//! # Rooting invariants
//!
//! Collection only happens at VM safe points (between bytecode instructions in
//! `VM::run_until`). At a safe point the roots are: live registers, globals,
//! call frames, method/static/default tables, JIT string constants, and the
//! **pin stack** (`Gc::pinned`).
//!
//! Native (Rust) builtins can hold `Value`s in Rust locals that the register
//! scan cannot see. That is only dangerous when the builtin re-enters Forge
//! code (`call_value`), because that is the only way a safe point can occur
//! while the builtin is on the Rust stack. The VM makes this safe
//! structurally rather than per builtin:
//!
//! * Every native call runs inside a *native scope* (`VM::call_native` wraps
//!   the dispatch in `Gc::enter_native` / `Gc::exit_native`). The call's
//!   arguments are pinned, and while the scope is active **every allocation
//!   is pinned automatically**, so objects a builtin creates can never be
//!   reclaimed under it.
//! * Re-entering bytecode (`VM::call_value`) suspends auto-pinning, so garbage
//!   produced by Forge callbacks is still collected normally; the value the
//!   callback returns is pinned into the enclosing native scope.
//! * When a native scope ends, its pins are released.
//!
//! The one thing a builtin must still do by hand: if it copies `Value`s *out
//! of* a heap object (e.g. snapshots an array's items) and then calls back into
//! Forge code, the callback could mutate that object and drop the last
//! reference. Pin such snapshots with `Gc::pin_values` (or `VM::root_values`).
//!
//! `FORGE_GC_STRESS=1` (or `Gc::set_stress(true)`) collects at every safe
//! point that follows an allocation, which turns any missing root into a
//! deterministic `<freed>` / wrong-value failure in tests.
//!
//! # Memory limit
//!
//! With a limit set ([`Gc::with_memory_limit`], from `--max-memory` /
//! `runtime::limits`), the heap keeps an estimate of its live bytes
//! (`ObjKind::heap_bytes`): every allocation and every in-place growth
//! ([`Gc::note_growth`]) adds to it, and every collection recomputes it from
//! the surviving objects. Crossing the limit forces a collection at the next
//! safe point; only if the heap is *still* over the limit after collecting
//! does [`Gc::memory_exceeded`] report it and the VM fail the run. So that a
//! heap living just under the limit does not collect on every allocation,
//! the next forced collection waits for `limit / MEM_SLACK_DIVISOR` bytes of
//! new allocation (the live heap can therefore exceed the limit by that much
//! before it is detected). Without a limit none of this bookkeeping runs.

use std::collections::HashMap;

use super::value::{GcObject, GcRef, ObjKind, Value};

const INITIAL_GC_THRESHOLD: usize = 8192;
const GC_GROWTH_FACTOR: usize = 2;
/// Once the live heap is close to the memory limit, a forced collection
/// waits for `limit / MEM_SLACK_DIVISOR` bytes of new allocation.
const MEM_SLACK_DIVISOR: usize = 8;
/// Strings longer than this are not interned (avoids bloating the table with
/// large unique strings like HTTP bodies or file contents).
const INTERN_MAX_LEN: usize = 128;

/// Mark-sweep garbage collector.
pub struct Gc {
    objects: Vec<Option<GcObject>>,
    free_list: Vec<usize>,
    pub alloc_count: usize,
    next_gc: usize,
    /// Intern table: maps string content → canonical GcRef.
    interned: HashMap<String, GcRef>,
    /// Pin stack: extra roots owned by active native scopes.
    pinned: Vec<GcRef>,
    /// When true, every allocation is pushed onto `pinned` (native code is
    /// running). Cleared while bytecode runs.
    pinning: bool,
    /// Stress mode: collect at every safe point that follows an allocation.
    stress: bool,
    /// Allocations since the last collection (drives stress mode).
    allocs_since_collect: usize,
    /// Byte limit for the live heap (`None` = unlimited, no accounting).
    mem_limit: Option<usize>,
    /// Estimated live bytes; maintained only when `mem_limit` is set.
    mem_bytes: usize,
    /// Estimated bytes at which the next collection is forced.
    mem_trigger: usize,
    /// Set by a collection that left the heap above `mem_limit`.
    mem_exceeded: bool,
}

/// Saved state returned by `Gc::enter_native`; hand it back to `exit_native`.
#[must_use = "pass the scope to Gc::exit_native, or the pins leak"]
pub struct NativeScope {
    mark: usize,
    prev_pinning: bool,
}

impl Gc {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            free_list: Vec::new(),
            alloc_count: 0,
            next_gc: INITIAL_GC_THRESHOLD,
            interned: HashMap::new(),
            pinned: Vec::new(),
            pinning: false,
            stress: std::env::var("FORGE_GC_STRESS")
                .map(|v| !v.is_empty() && v != "0")
                .unwrap_or(false),
            allocs_since_collect: 0,
            mem_limit: None,
            mem_bytes: 0,
            mem_trigger: usize::MAX,
            mem_exceeded: false,
        }
    }

    /// A heap that fails the run once its live bytes exceed `limit` (see the
    /// module docs). `None` is the same as [`Gc::new`].
    pub fn with_memory_limit(limit: Option<usize>) -> Self {
        Self {
            mem_limit: limit,
            mem_trigger: limit.unwrap_or(usize::MAX),
            ..Self::new()
        }
    }

    /// The byte limit, if any.
    pub fn memory_limit(&self) -> Option<usize> {
        self.mem_limit
    }

    /// Estimated live heap bytes (0 when no limit is set).
    #[cfg(test)]
    pub fn memory_bytes(&self) -> usize {
        self.mem_bytes
    }

    /// True when the last collection left the heap above its limit.
    #[inline]
    pub fn memory_exceeded(&self) -> bool {
        self.mem_exceeded
    }

    /// Account `bytes` of growth of an existing object (an in-place append
    /// to a uniquely owned string or array).
    #[inline]
    pub fn note_growth(&mut self, bytes: usize) {
        if self.mem_limit.is_some() {
            self.charge(bytes);
        }
    }

    #[inline]
    fn charge(&mut self, bytes: usize) {
        self.mem_bytes = self.mem_bytes.saturating_add(bytes);
        if self.mem_bytes > self.mem_trigger {
            // Collect at the next safe point; fail only if that does not
            // bring the heap back under the limit.
            self.next_gc = 0;
        }
    }

    /// Enable/disable stress mode (collect at every safe point after an
    /// allocation). Also settable with `FORGE_GC_STRESS=1`.
    #[allow(dead_code)]
    pub fn set_stress(&mut self, on: bool) {
        self.stress = on;
    }

    /// Open a native scope: auto-pin every allocation until `exit_native`.
    pub fn enter_native(&mut self) -> NativeScope {
        let scope = NativeScope {
            mark: self.pinned.len(),
            prev_pinning: self.pinning,
        };
        self.pinning = true;
        scope
    }

    /// Close a native scope, releasing its pins and restoring the previous
    /// pinning mode.
    pub fn exit_native(&mut self, scope: NativeScope) {
        self.pinned.truncate(scope.mark);
        self.pinning = scope.prev_pinning;
    }

    /// Suspend/resume auto-pinning around bytecode execution. Returns the
    /// previous mode so the caller can restore it.
    pub fn set_pinning(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.pinning, on)
    }

    /// Pin a value (if it is a heap ref) until the current native scope ends.
    /// Outside a native scope this is a no-op (bytecode values live in
    /// registers).
    #[inline]
    pub fn pin_value(&mut self, v: Value) {
        if self.pinning {
            if let Some(r) = v.as_obj() {
                self.pinned.push(r);
            }
        }
    }

    /// Pin every heap ref in `values` until the current native scope ends.
    pub fn pin_values(&mut self, values: &[Value]) {
        if self.pinning {
            self.pinned.extend(values.iter().filter_map(|v| v.as_obj()));
        }
    }

    #[cfg(test)]
    pub fn pinned_len(&self) -> usize {
        self.pinned.len()
    }

    /// Allocate a new object on the GC heap. Returns a GcRef.
    pub fn alloc(&mut self, kind: ObjKind) -> GcRef {
        self.alloc_count += 1;
        self.allocs_since_collect += 1;
        if self.mem_limit.is_some() {
            self.charge(kind.heap_bytes());
        }
        let obj = GcObject::new(kind);
        let r = if let Some(idx) = self.free_list.pop() {
            self.objects[idx] = Some(obj);
            GcRef(idx)
        } else {
            let idx = self.objects.len();
            self.objects.push(Some(obj));
            GcRef(idx)
        };
        if self.pinning {
            self.pinned.push(r);
        }
        r
    }

    /// Allocate a string, interning short strings for deduplication.
    /// Strings ≤ INTERN_MAX_LEN bytes are looked up in the intern table first;
    /// if already present, the existing GcRef is returned (no new allocation).
    pub fn alloc_string(&mut self, s: String) -> GcRef {
        if s.len() <= INTERN_MAX_LEN {
            if let Some(&existing) = self.interned.get(&s) {
                // The interned object may be otherwise unreachable; native
                // code now holds it, so pin it like a fresh allocation.
                if self.pinning {
                    self.pinned.push(existing);
                }
                return existing;
            }
            let r = self.alloc(ObjKind::String(s.clone()));
            self.interned.insert(s, r);
            r
        } else {
            self.alloc(ObjKind::String(s))
        }
    }

    /// Like [`Gc::alloc_string`] for a borrowed string: an intern-table hit
    /// allocates nothing (no `String` is built just to be looked up), which
    /// keeps `LoadConst` of a string constant allocation-free.
    pub fn alloc_str(&mut self, s: &str) -> GcRef {
        if s.len() <= INTERN_MAX_LEN {
            if let Some(&existing) = self.interned.get(s) {
                if self.pinning {
                    self.pinned.push(existing);
                }
                return existing;
            }
        }
        self.alloc_string(s.to_string())
    }

    /// Allocate a string that is *not* interned, marked as uniquely owned
    /// (see `GcObject::unique`). Used for strings a local variable will be
    /// extended in place; interned strings are shared and never mutated.
    pub fn alloc_unique_string(&mut self, s: String) -> GcRef {
        let r = self.alloc(ObjKind::String(s));
        self.set_unique(r);
        r
    }

    /// Mark `r` as uniquely owned by the local register it is stored in.
    #[inline]
    pub fn set_unique(&mut self, r: GcRef) {
        if let Some(obj) = self.get_mut(r) {
            obj.unique = true;
        }
    }

    /// `v` is being copied out of a local register: if it is a uniquely
    /// owned object it now has a second reference, so clear the bit.
    #[inline]
    pub fn share(&mut self, v: Value) {
        if let Some(r) = v.as_obj() {
            if let Some(obj) = self.get_mut(r) {
                obj.unique = false;
            }
        }
    }

    /// Whether `r` is uniquely owned by the local register holding it.
    #[inline]
    pub fn is_unique(&self, r: GcRef) -> bool {
        self.get(r).is_some_and(|o| o.unique)
    }

    /// Check if GC should run.
    #[inline]
    pub fn should_collect(&self) -> bool {
        self.alloc_count >= self.next_gc || (self.stress && self.allocs_since_collect > 0)
    }

    /// Get an object by ref (immutable).
    #[inline]
    pub fn get(&self, r: GcRef) -> Option<&GcObject> {
        self.objects.get(r.0).and_then(|o| o.as_ref())
    }

    /// Get an object by ref (mutable).
    #[inline]
    pub fn get_mut(&mut self, r: GcRef) -> Option<&mut GcObject> {
        self.objects.get_mut(r.0).and_then(|o| o.as_mut())
    }

    /// Run a full mark-sweep collection.
    /// `roots` are all GcRefs reachable from the VM (registers, globals, frames, upvalues).
    pub fn collect(&mut self, roots: &[GcRef]) {
        let pinned = std::mem::take(&mut self.pinned);
        self.mark(roots);
        self.mark(&pinned);
        self.pinned = pinned;
        self.sweep();
        self.allocs_since_collect = 0;
        self.next_gc = self.alloc_count * GC_GROWTH_FACTOR;
        if self.next_gc < INITIAL_GC_THRESHOLD {
            self.next_gc = INITIAL_GC_THRESHOLD;
        }
        if let Some(limit) = self.mem_limit {
            self.mem_bytes = self
                .objects
                .iter()
                .flatten()
                .map(|o| o.kind.heap_bytes())
                .sum();
            self.mem_exceeded = self.mem_bytes > limit;
            self.mem_trigger = limit.max(self.mem_bytes + limit / MEM_SLACK_DIVISOR);
        }
    }

    fn mark(&mut self, roots: &[GcRef]) {
        let mut worklist: Vec<GcRef> = roots.to_vec();

        while let Some(r) = worklist.pop() {
            if let Some(obj) = self.objects.get_mut(r.0).and_then(|o| o.as_mut()) {
                if obj.marked {
                    continue;
                }
                obj.marked = true;
                obj.trace(&mut worklist);
            }
        }
    }

    fn sweep(&mut self) {
        let mut freed = 0;
        for i in 0..self.objects.len() {
            let should_free = match &self.objects[i] {
                Some(obj) => !obj.marked,
                None => false,
            };
            if should_free {
                // Extract string content before destroying, to clean intern table
                if let Some(obj) = &self.objects[i] {
                    if let ObjKind::String(ref s) = obj.kind {
                        // Only drop the entry if it names *this* object: a
                        // short string allocated outside the table (e.g. a
                        // string built in place) must not evict the live
                        // canonical copy of the same text.
                        if s.len() <= INTERN_MAX_LEN
                            && self.interned.get(s.as_str()) == Some(&GcRef(i))
                        {
                            self.interned.remove(s.as_str());
                        }
                    }
                }
                self.objects[i] = None;
                self.free_list.push(i);
                freed += 1;
            } else if let Some(obj) = &mut self.objects[i] {
                obj.marked = false;
            }
        }
        self.alloc_count = self.alloc_count.saturating_sub(freed);
    }

    /// Collect all GcRefs from a set of values.
    #[allow(dead_code)]
    pub fn roots_from_values(values: &[Value]) -> Vec<GcRef> {
        values.iter().filter_map(|v| v.as_obj()).collect()
    }

    /// Number of entries in the intern table (for testing).
    #[cfg(test)]
    pub fn intern_count(&self) -> usize {
        self.interned.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interned_strings_share_gcref() {
        let mut gc = Gc::new();
        let r1 = gc.alloc_string("hello".to_string());
        let r2 = gc.alloc_string("hello".to_string());
        assert_eq!(r1, r2, "same string should return same GcRef");
    }

    #[test]
    fn different_strings_get_different_refs() {
        let mut gc = Gc::new();
        let r1 = gc.alloc_string("hello".to_string());
        let r2 = gc.alloc_string("world".to_string());
        assert_ne!(r1, r2);
    }

    #[test]
    fn long_strings_not_interned() {
        let mut gc = Gc::new();
        let long = "x".repeat(INTERN_MAX_LEN + 1);
        let r1 = gc.alloc_string(long.clone());
        let r2 = gc.alloc_string(long);
        assert_ne!(r1, r2, "long strings should not be interned");
    }

    #[test]
    fn short_strings_at_boundary_are_interned() {
        let mut gc = Gc::new();
        let at_limit = "x".repeat(INTERN_MAX_LEN);
        let r1 = gc.alloc_string(at_limit.clone());
        let r2 = gc.alloc_string(at_limit);
        assert_eq!(r1, r2, "string at exact limit should be interned");
    }

    #[test]
    fn sweep_removes_unreachable_interned_strings() {
        let mut gc = Gc::new();
        let _r1 = gc.alloc_string("ephemeral".to_string());
        assert_eq!(gc.intern_count(), 1);

        // Collect with no roots — everything is unreachable
        gc.collect(&[]);
        assert_eq!(
            gc.intern_count(),
            0,
            "intern table should be cleaned on sweep"
        );
    }

    #[test]
    fn sweep_keeps_reachable_interned_strings() {
        let mut gc = Gc::new();
        let r1 = gc.alloc_string("keep".to_string());
        let _r2 = gc.alloc_string("discard".to_string());
        assert_eq!(gc.intern_count(), 2);

        // Only r1 is a root
        gc.collect(&[r1]);
        assert_eq!(gc.intern_count(), 1);

        // The kept ref is still valid and interned
        let r3 = gc.alloc_string("keep".to_string());
        assert_eq!(r1, r3, "surviving interned string should be reused");
    }

    #[test]
    fn re_interning_after_collection() {
        let mut gc = Gc::new();
        let r1 = gc.alloc_string("temp".to_string());
        gc.collect(&[]); // sweep removes it
        assert_eq!(gc.intern_count(), 0);

        // Re-allocating the same string should work (new slot)
        let r2 = gc.alloc_string("temp".to_string());
        assert_eq!(gc.intern_count(), 1);
        // May or may not reuse the same index (free list), but ref should be valid
        assert!(gc.get(r2).is_some());
        // r1 should be invalid (freed)
        let _ = r1; // just to suppress unused warning
    }

    #[test]
    fn native_scope_pins_allocations_until_exit() {
        let mut gc = Gc::new();
        let scope = gc.enter_native();
        let a = gc.alloc_string("pinned-in-native".to_string());
        let b = gc.alloc(ObjKind::Array(vec![]));
        gc.collect(&[]);
        assert!(gc.get(a).is_some(), "native-scope string must survive GC");
        assert!(gc.get(b).is_some(), "native-scope array must survive GC");
        gc.exit_native(scope);
        assert_eq!(gc.pinned_len(), 0);
        gc.collect(&[]);
        assert!(gc.get(a).is_none(), "pins are released when the scope ends");
        assert!(gc.get(b).is_none());
    }

    #[test]
    fn bytecode_inside_native_scope_is_not_auto_pinned() {
        let mut gc = Gc::new();
        let scope = gc.enter_native();
        let prev = gc.set_pinning(false);
        let garbage = gc.alloc(ObjKind::Array(vec![]));
        let ret = gc.alloc_string("callback-result".to_string());
        gc.set_pinning(prev);
        gc.pin_value(Value::obj(ret));
        gc.collect(&[]);
        assert!(gc.get(garbage).is_none(), "callback garbage is collectable");
        assert!(gc.get(ret).is_some(), "callback result is pinned");
        gc.exit_native(scope);
    }

    #[test]
    fn interned_hit_is_pinned_in_native_scope() {
        let mut gc = Gc::new();
        let r1 = gc.alloc_string("shared".to_string());
        let scope = gc.enter_native();
        let r2 = gc.alloc_string("shared".to_string());
        assert_eq!(r1, r2);
        gc.collect(&[]);
        assert!(gc.get(r2).is_some());
        gc.exit_native(scope);
    }

    #[test]
    fn stress_mode_collects_after_every_allocation() {
        let mut gc = Gc::new();
        gc.set_stress(true);
        assert!(!gc.should_collect());
        let _ = gc.alloc(ObjKind::Array(vec![]));
        assert!(gc.should_collect());
        gc.collect(&[]);
        assert!(!gc.should_collect());
    }

    #[test]
    fn empty_string_is_interned() {
        let mut gc = Gc::new();
        let r1 = gc.alloc_string(String::new());
        let r2 = gc.alloc_string(String::new());
        assert_eq!(r1, r2);
    }
}
