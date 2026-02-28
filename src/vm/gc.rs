use super::value::{GcObject, GcRef, ObjKind, Value};

const INITIAL_GC_THRESHOLD: usize = 8192;
const GC_GROWTH_FACTOR: usize = 2;

/// Mark-sweep garbage collector.
pub struct Gc {
    objects: Vec<Option<GcObject>>,
    free_list: Vec<usize>,
    pub alloc_count: usize,
    next_gc: usize,
}

impl Gc {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            free_list: Vec::new(),
            alloc_count: 0,
            next_gc: INITIAL_GC_THRESHOLD,
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

    /// Allocate a string and return its GcRef.
    pub fn alloc_string(&mut self, s: String) -> GcRef {
        self.alloc(ObjKind::String(s))
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
            if let Some(obj) = &mut self.objects[i] {
                if obj.marked {
                    obj.marked = false;
                } else {
                    self.objects[i] = None;
                    self.free_list.push(i);
                    freed += 1;
                }
            }
        }
        self.alloc_count = self.alloc_count.saturating_sub(freed);
    }

    /// Collect all GcRefs from a set of values.
    #[allow(dead_code)]
    pub fn roots_from_values(values: &[Value]) -> Vec<GcRef> {
        values
            .iter()
            .filter_map(|v| {
                if let Value::Obj(r) = v {
                    Some(*r)
                } else {
                    None
                }
            })
            .collect()
    }
}
