//! Cycle collection for interpreter scopes.
//!
//! # Why this exists
//!
//! Closures capture their defining environment by `Arc`, and the scope
//! that defines a function usually also *stores* it:
//!
//! ```text
//! fn fact(n) { ... fact(n - 1) ... }   // scope S binds `fact`,
//!                                       // fact.closure = [.., S]
//! ```
//!
//! `S -> Value::Function -> FunctionValue::closure -> S` is a reference
//! cycle, so plain reference counting never frees it. The same shape
//! appears for lambdas stored in the scope they capture (a lambda bound
//! inside a loop body), for inner functions, for methods, and for every
//! scope a module import or an HTTP fork duplicates. Without collection
//! each interpreter that defines a function leaks its whole global scope
//! (stdlib module objects included), and loops that create closures leak
//! per iteration.
//!
//! Weak references cannot fix this: a closure that escapes its scope
//! (`make_counter()` returning a lambda over a call's local scope) must keep
//! that scope alive, and nothing else owns it.
//!
//! # Design: trial deletion over a registered candidate set
//!
//! Every cycle passes through a scope that some closure captured (values
//! are owned trees; the only shared, mutable edges are scope cells, lambda
//! environments, streams and task slots, and the only way to point back at
//! a scope is to capture it). So each [`ScopeHeap`] *registers* a scope the
//! first time a closure captures it ([`ScopeHeap::track`]); those are the
//! collection candidates. A collection then works like CPython's cycle
//! collector:
//!
//! 1. From the candidates, discover the graph of shared nodes (scopes,
//!    function values, lambda environments, streams, task slots) and count,
//!    for every node, the strong references that come from *inside* that
//!    graph.
//! 2. A node whose `Arc::strong_count` exceeds its internal count is held
//!    from outside: by an interpreter's environment stack, a Rust local, a
//!    host, a channel buffer, another thread. It is live, and so is
//!    everything reachable from it.
//! 3. Every candidate scope not reached in step 2 is garbage: only other
//!    garbage refers to it. Its bindings are taken out, which breaks the
//!    cycle, and ordinary reference counting frees the rest.
//!
//! The analysis is *conservative*: any reference it cannot see (an opaque
//! holder, a scope it could not lock, a channel buffer) counts as external
//! and keeps its target alive. It can only fail to free; it never frees a
//! scope that anything outside the garbage set still references. Values
//! that escape an interpreter (a lambda a host keeps, imported functions
//! moved into the importer) therefore stay intact.
//!
//! # When collections run
//!
//! * **Periodically**, from [`ScopeHeap::track`], once enough new scopes
//!   have been registered since the last collection (amortised O(1) per
//!   registration). The calling interpreter's current environment is
//!   passed as a *known-live* set and is not walked, so the global scope
//!   and its data are not rescanned each time.
//! * **At teardown**, when the last [`Interpreter`](super::Interpreter)
//!   attached to the heap is dropped. The interpreter first releases its
//!   own roots (environment, method tables), so whatever is left reachable
//!   only through cycles is reclaimed deterministically. Per-call hosts
//!   (`Sandbox`, `forge mcp`, the Python binding, one interpreter per HTTP
//!   request) need no explicit call: dropping the interpreter is the
//!   teardown.
//!
//! # Concurrency invariant
//!
//! A heap is shared by every interpreter that can reach the same scopes:
//! the root of a run, its imports, `timeout` bodies, spawned tasks
//! (`child_context`) and `schedule`/`watch` forks. A collection only runs
//! while exactly one interpreter is attached (periodic) or none is
//! (teardown), so no other thread is executing Forge code over these
//! scopes while counts are taken. An HTTP request fork gets its own heap:
//! `deep_clone_isolated` shares no scope with the template. Every lock the
//! collector takes is a `try_lock`; a contended node is treated as live.

use super::{Environment, FunctionValue, Scope, ScopeCell, StreamCell, StreamKind, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};

/// Source of heap ids. Zero is reserved for "not registered anywhere".
static NEXT_HEAP_ID: AtomicU64 = AtomicU64::new(1);

/// Never collect before this many new registrations: small programs pay
/// nothing until teardown.
const MIN_COLLECT_THRESHOLD: usize = 1024;

type TaskSlot = Arc<(Mutex<Option<Value>>, Condvar)>;

/// The set of closure-captured scopes of one interpreter graph, and the
/// interpreters attached to it. See the module docs.
#[derive(Debug)]
pub(crate) struct ScopeHeap {
    id: u64,
    state: Mutex<HeapState>,
    /// Interpreters currently attached (see [`ScopeHeap::attach`]).
    interpreters: AtomicUsize,
}

#[derive(Debug, Default)]
struct HeapState {
    tracked: Vec<Weak<Mutex<Scope>>>,
    /// Registrations since the last collection.
    since_collect: usize,
    /// Tracked scopes that survived the last collection.
    survivors: usize,
}

impl ScopeHeap {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            id: NEXT_HEAP_ID.fetch_add(1, Ordering::Relaxed),
            state: Mutex::new(HeapState::default()),
            interpreters: AtomicUsize::new(0),
        })
    }

    /// An interpreter starts using this heap.
    pub(crate) fn attach(&self) {
        self.interpreters.fetch_add(1, Ordering::AcqRel);
    }

    /// An interpreter stops using this heap. Returns true when it was the
    /// last one: nothing can run Forge code on these scopes any more, so
    /// the caller runs the teardown collection.
    pub(crate) fn detach(&self) -> bool {
        self.interpreters.fetch_sub(1, Ordering::AcqRel) == 1
    }

    fn state(&self) -> std::sync::MutexGuard<'_, HeapState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Register every scope of `env` that a closure is about to capture,
    /// then collect if enough has accumulated and `env`'s interpreter is
    /// the only one attached. `env` is that interpreter's current
    /// environment, so it is also the known-live set for the collection.
    ///
    /// Must not be called while the caller holds a scope lock.
    pub(crate) fn track(&self, env: &Environment) {
        let added = self.register(env.scopes.iter());
        if added == 0 {
            return;
        }
        let due = {
            let mut st = self.state();
            st.since_collect += added;
            st.since_collect > MIN_COLLECT_THRESHOLD.max(st.survivors)
        };
        if due && self.interpreters.load(Ordering::Acquire) == 1 {
            self.collect(&env.scopes);
        }
    }

    /// Register freshly created scopes that closures already capture (the
    /// copies `deep_clone_isolated` makes for an HTTP fork).
    pub(crate) fn track_cells<'a>(&self, cells: impl Iterator<Item = &'a ScopeCell>) {
        let added = self.register(cells);
        self.state().since_collect += added;
    }

    fn register<'a>(&self, cells: impl Iterator<Item = &'a ScopeCell>) -> usize {
        let mut fresh = Vec::new();
        for cell in cells {
            let mut scope = super::lock_scope(cell);
            if scope.tracked_by != self.id {
                scope.tracked_by = self.id;
                fresh.push(Arc::downgrade(cell));
            }
        }
        let added = fresh.len();
        if added > 0 {
            self.state().tracked.extend(fresh);
        }
        added
    }

    /// Run one collection. `live` are scopes known to be reachable (the
    /// collecting interpreter's environment); they are not walked. Returns
    /// the number of scopes reclaimed.
    pub(crate) fn collect(&self, live: &[ScopeCell]) -> usize {
        let mut st = self.state();
        let live: HashSet<usize> = live.iter().map(key).collect();

        let mut graph = Graph::new(&live);
        let mut candidates = Vec::with_capacity(st.tracked.len());
        for weak in &st.tracked {
            if let Some(cell) = weak.upgrade() {
                candidates.push(graph.intern_scope(&cell));
            }
        }
        graph.discover();
        let garbage = graph.garbage();

        // Survivors: registered scopes that are still referenced.
        let garbage_set: HashSet<usize> = garbage.iter().copied().collect();
        let tracked: Vec<Weak<Mutex<Scope>>> = candidates
            .iter()
            .filter(|i| !garbage_set.contains(i))
            .filter_map(|&i| match &graph.nodes[i].handle {
                Handle::Scope(cell) => Some(Arc::downgrade(cell)),
                _ => None,
            })
            .collect();
        st.survivors = tracked.len();
        st.tracked = tracked;
        st.since_collect = 0;

        // Break the cycles: take every garbage scope's bindings out.
        let mut graveyard = Vec::with_capacity(garbage.len());
        for &i in &garbage {
            if let Handle::Scope(cell) = &graph.nodes[i].handle {
                if let Ok(mut scope) = cell.try_lock() {
                    graveyard.push(std::mem::take(&mut scope.bindings));
                    scope.index = None;
                }
            }
        }
        drop(st);
        let freed = graveyard.len();
        // Drop the values (which releases the cycle edges) and then our own
        // handles; reference counting frees the rest.
        drop(graveyard);
        drop(graph);
        freed
    }
}

fn key<T: ?Sized>(arc: &Arc<T>) -> usize {
    Arc::as_ptr(arc) as *const () as usize
}

/// One shared node of the object graph. The collector holds exactly one
/// strong handle per node while it runs.
enum Handle {
    Scope(ScopeCell),
    Function(Arc<FunctionValue>),
    Lambda(Arc<Mutex<Environment>>),
    Stream(Arc<Mutex<StreamCell>>),
    Task(TaskSlot),
}

impl Handle {
    fn strong_count(&self) -> usize {
        match self {
            Handle::Scope(a) => Arc::strong_count(a),
            Handle::Function(a) => Arc::strong_count(a),
            Handle::Lambda(a) => Arc::strong_count(a),
            Handle::Stream(a) => Arc::strong_count(a),
            Handle::Task(a) => Arc::strong_count(a),
        }
    }
}

struct Node {
    handle: Handle,
    /// References to this node from other discovered nodes.
    internal: usize,
    /// Nodes this one references (with multiplicity).
    edges: Vec<usize>,
    /// Not walked (known live, or its lock was busy): treated as live.
    opaque: bool,
}

struct Graph<'a> {
    live: &'a HashSet<usize>,
    nodes: Vec<Node>,
    index: HashMap<usize, usize>,
    pending: Vec<usize>,
}

impl<'a> Graph<'a> {
    fn new(live: &'a HashSet<usize>) -> Self {
        Self {
            live,
            nodes: Vec::new(),
            index: HashMap::new(),
            pending: Vec::new(),
        }
    }

    fn intern(&mut self, k: usize, make: impl FnOnce() -> Handle) -> usize {
        if let Some(&i) = self.index.get(&k) {
            return i;
        }
        let i = self.nodes.len();
        self.nodes.push(Node {
            handle: make(),
            internal: 0,
            edges: Vec::new(),
            opaque: false,
        });
        self.index.insert(k, i);
        self.pending.push(i);
        i
    }

    fn intern_scope(&mut self, cell: &ScopeCell) -> usize {
        self.intern(key(cell), || Handle::Scope(cell.clone()))
    }

    fn edge(&mut self, from: usize, to: usize) {
        self.nodes[to].internal += 1;
        self.nodes[from].edges.push(to);
    }

    fn edges_to_env(&mut self, from: usize, env: &Environment) {
        for cell in &env.scopes {
            let to = self.intern_scope(cell);
            self.edge(from, to);
        }
    }

    /// Record the shared nodes `value` references (directly or inside
    /// owned containers) as edges from `from`.
    fn walk_value(&mut self, from: usize, value: &Value) {
        let mut stack = vec![value];
        while let Some(v) = stack.pop() {
            match v {
                Value::Function(f) => {
                    let to = self.intern(key(f), || Handle::Function(f.clone()));
                    self.edge(from, to);
                }
                Value::Lambda { closure, .. } => {
                    let to = self.intern(key(closure), || Handle::Lambda(closure.clone()));
                    self.edge(from, to);
                }
                Value::Stream(s) => {
                    let to = self.intern(key(s), || Handle::Stream(s.clone()));
                    self.edge(from, to);
                }
                Value::TaskHandle(t) => {
                    let to = self.intern(key(t), || Handle::Task(t.clone()));
                    self.edge(from, to);
                }
                Value::Array(items) | Value::Tuple(items) | Value::Set(items) => {
                    stack.extend(items.iter())
                }
                Value::Map(pairs) => {
                    for (k, val) in pairs {
                        stack.push(k);
                        stack.push(val);
                    }
                }
                Value::Object(fields) => stack.extend(fields.values()),
                Value::ResultOk(b) | Value::ResultErr(b) | Value::Some(b) | Value::Frozen(b) => {
                    stack.push(b)
                }
                // A channel's buffered values are invisible here; whatever
                // they reference counts as externally held (conservative).
                Value::Channel(_)
                | Value::Int(_)
                | Value::Float(_)
                | Value::String(_)
                | Value::Bool(_)
                | Value::None
                | Value::Null
                | Value::BuiltIn(_) => {}
            }
        }
    }

    /// Walk every pending node until the reachable graph is complete.
    fn discover(&mut self) {
        while let Some(i) = self.pending.pop() {
            // Clone the handle (a refcount bump) so walking can borrow
            // `self` mutably; it is dropped before any count is read.
            let handle = match &self.nodes[i].handle {
                Handle::Scope(a) => Handle::Scope(a.clone()),
                Handle::Function(a) => Handle::Function(a.clone()),
                Handle::Lambda(a) => Handle::Lambda(a.clone()),
                Handle::Stream(a) => Handle::Stream(a.clone()),
                Handle::Task(a) => Handle::Task(a.clone()),
            };
            match &handle {
                Handle::Scope(cell) => {
                    if self.live.contains(&key(cell)) {
                        self.nodes[i].opaque = true;
                        continue;
                    }
                    match cell.try_lock() {
                        Ok(scope) => {
                            for binding in &scope.bindings {
                                self.walk_value(i, &binding.value);
                            }
                        }
                        Err(_) => self.nodes[i].opaque = true,
                    }
                }
                Handle::Function(f) => self.edges_to_env(i, &f.closure),
                Handle::Lambda(env) => match env.try_lock() {
                    Ok(env) => self.edges_to_env(i, &env),
                    Err(_) => self.nodes[i].opaque = true,
                },
                Handle::Stream(cell) => match cell.try_lock() {
                    Ok(cell) => self.walk_stream(i, &cell.kind),
                    Err(_) => self.nodes[i].opaque = true,
                },
                Handle::Task(slot) => match slot.0.try_lock() {
                    Ok(result) => {
                        if let Some(v) = result.as_ref() {
                            self.walk_value(i, v);
                        }
                    }
                    Err(_) => self.nodes[i].opaque = true,
                },
            }
        }
    }

    fn walk_stream(&mut self, from: usize, kind: &StreamKind) {
        let upstream = |g: &mut Self, s: &Arc<Mutex<StreamCell>>| {
            let to = g.intern(key(s), || Handle::Stream(s.clone()));
            g.edge(from, to);
        };
        match kind {
            StreamKind::ArrayIter { items, .. }
            | StreamKind::TupleIter { items, .. }
            | StreamKind::SetIter { items, .. } => {
                for v in items {
                    self.walk_value(from, v);
                }
            }
            StreamKind::MapIter { pairs, .. } => {
                for (k, v) in pairs {
                    self.walk_value(from, k);
                    self.walk_value(from, v);
                }
            }
            StreamKind::StringIter { .. } => {}
            StreamKind::Filter {
                upstream: up,
                pred: f,
            }
            | StreamKind::Map {
                upstream: up,
                fn_val: f,
            } => {
                upstream(self, up);
                self.walk_value(from, f);
            }
            StreamKind::Take { upstream: up, .. }
            | StreamKind::Skip { upstream: up, .. }
            | StreamKind::Enumerate { upstream: up, .. } => upstream(self, up),
            StreamKind::Chain { first, second, .. } => {
                upstream(self, first);
                upstream(self, second);
            }
            StreamKind::Zip { left, right } => {
                upstream(self, left);
                upstream(self, right);
            }
        }
    }

    /// Indices of garbage scope nodes, or none if the counts are
    /// inconsistent (which only a concurrent mutation could cause).
    fn garbage(&self) -> Vec<usize> {
        // Our one handle per node is not a reference from the program.
        let strong: Vec<usize> = self
            .nodes
            .iter()
            .map(|n| n.handle.strong_count().saturating_sub(1))
            .collect();
        let mut live = vec![false; self.nodes.len()];
        let mut work = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if node.internal > strong[i] {
                return Vec::new();
            }
            if node.opaque || strong[i] > node.internal {
                live[i] = true;
                work.push(i);
            }
        }
        while let Some(i) = work.pop() {
            for &j in &self.nodes[i].edges {
                if !live[j] {
                    live[j] = true;
                    work.push(j);
                }
            }
        }
        // Re-validate: a count that moved while we looked means another
        // thread touched the graph after all; collect nothing this time.
        let mut garbage = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if live[i] {
                continue;
            }
            if node.handle.strong_count().saturating_sub(1) != strong[i] {
                return Vec::new();
            }
            if matches!(node.handle, Handle::Scope(_)) {
                garbage.push(i);
            }
        }
        garbage
    }
}

/// Test-only accounting of live scopes, for leak tests. A test installs a
/// counter for its thread; every scope created on that thread while it is
/// installed increments it and decrements it when dropped (on any thread).
#[cfg(test)]
pub(crate) mod probe {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::Arc;

    thread_local! {
        static COUNTER: RefCell<Option<Arc<AtomicIsize>>> = const { RefCell::new(None) };
    }

    #[derive(Debug)]
    pub(crate) struct ScopeProbe(Option<Arc<AtomicIsize>>);

    impl Default for ScopeProbe {
        fn default() -> Self {
            let counter = COUNTER.with(|c| c.borrow().clone());
            if let Some(c) = &counter {
                c.fetch_add(1, Ordering::SeqCst);
            }
            ScopeProbe(counter)
        }
    }

    impl Drop for ScopeProbe {
        fn drop(&mut self) {
            if let Some(c) = &self.0 {
                c.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    /// Count scopes created on this thread until the guard is dropped.
    pub(crate) fn install() -> Arc<AtomicIsize> {
        let counter = Arc::new(AtomicIsize::new(0));
        COUNTER.with(|c| *c.borrow_mut() = Some(counter.clone()));
        counter
    }

    pub(crate) fn uninstall() {
        COUNTER.with(|c| *c.borrow_mut() = None);
    }
}
