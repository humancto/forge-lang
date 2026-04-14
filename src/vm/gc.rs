use std::collections::HashMap;

use super::value::{GcObject, GcRef, ObjKind, Value};

const INITIAL_GC_THRESHOLD: usize = 8192;
const GC_GROWTH_FACTOR: usize = 2;
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
}

impl Gc {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            free_list: Vec::new(),
            alloc_count: 0,
            next_gc: INITIAL_GC_THRESHOLD,
            interned: HashMap::new(),
        }
    }

    /// Allocate a new object on the GC heap. Returns a GcRef.
    pub fn alloc(&mut self, kind: ObjKind) -> GcRef {
        self.alloc_count += 1;
        let obj = GcObject::new(kind);
        if let Some(idx) = self.free_list.pop() {
            self.objects[idx] = Some(obj);
            GcRef(idx)
        } else {
            let idx = self.objects.len();
            self.objects.push(Some(obj));
            GcRef(idx)
        }
    }

    /// Allocate a string, interning short strings for deduplication.
    /// Strings ≤ INTERN_MAX_LEN bytes are looked up in the intern table first;
    /// if already present, the existing GcRef is returned (no new allocation).
    pub fn alloc_string(&mut self, s: String) -> GcRef {
        if s.len() <= INTERN_MAX_LEN {
            if let Some(&existing) = self.interned.get(&s) {
                return existing;
            }
            let r = self.alloc(ObjKind::String(s.clone()));
            self.interned.insert(s, r);
            r
        } else {
            self.alloc(ObjKind::String(s))
        }
    }

    /// Check if GC should run.
    pub fn should_collect(&self) -> bool {
        self.alloc_count >= self.next_gc
    }

    /// Get an object by ref (immutable).
    pub fn get(&self, r: GcRef) -> Option<&GcObject> {
        self.objects.get(r.0).and_then(|o| o.as_ref())
    }

    /// Get an object by ref (mutable).
    pub fn get_mut(&mut self, r: GcRef) -> Option<&mut GcObject> {
        self.objects.get_mut(r.0).and_then(|o| o.as_mut())
    }

    /// Run a full mark-sweep collection.
    /// `roots` are all GcRefs reachable from the VM (registers, globals, frames, upvalues).
    pub fn collect(&mut self, roots: &[GcRef]) {
        self.mark(roots);
        self.sweep();
        self.next_gc = self.alloc_count * GC_GROWTH_FACTOR;
        if self.next_gc < INITIAL_GC_THRESHOLD {
            self.next_gc = INITIAL_GC_THRESHOLD;
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
                        if s.len() <= INTERN_MAX_LEN {
                            self.interned.remove(s);
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
    fn empty_string_is_interned() {
        let mut gc = Gc::new();
        let r1 = gc.alloc_string(String::new());
        let r2 = gc.alloc_string(String::new());
        assert_eq!(r1, r2);
    }
}
