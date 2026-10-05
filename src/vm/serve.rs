//! Serving HTTP and WebSocket handlers on the bytecode VM.
//!
//! The server (`runtime::server`) forks one isolated engine instance per
//! request from a read-only template. For the VM that template is a
//! [`VmTemplate`]: a frozen copy of everything the program's top level left
//! behind — globals, method / static-method / struct-default tables and the
//! heap objects reachable from them.
//!
//! # Isolation
//!
//! [`VmTemplate::fork`] builds a fresh [`VM`] whose heap is a private copy of
//! the template's: every array, object, closure and **upvalue cell** is
//! re-created, so a handler that mutates a top-level binding, a collection or
//! state captured by a closure only changes its own copy. Object identity is
//! preserved inside one fork — two closures that captured the same variable
//! still share one cell — which is the VM counterpart of the interpreter's
//! `Environment::deep_clone_isolated`. Channels and task handles are
//! `Arc`-shared, exactly as in the interpreter, so a top-level channel can
//! coordinate requests on purpose.
//!
//! # Cost
//!
//! Freezing runs once at start-up and records the objects in allocation
//! order (children before parents), so a fork is one linear pass with a
//! dense index → `GcRef` table: no graph walk, hashing or cycle checks per
//! request. Cycles can only run through upvalue cells (value semantics make
//! every other object graph acyclic); cells are allocated first and filled
//! last, which breaks them.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::http::StatusCode;
use indexmap::IndexMap;
use serde_json::Value as JsonValue;

use super::gc::Gc;
use super::machine::VM;
use super::profiler::Profiler;
use super::value::*;
use crate::runtime::server::{handler_args, HandlerRequest, HandlerWorker, ServeEngine};

/// A heap object of the template. `Value`s inside refer to template slots
/// (`GcRef(slot)`), not to any live heap.
enum FrozenObj {
    String(String),
    Array(Vec<Value>),
    Tuple(Vec<Value>),
    Set(Vec<Value>),
    Object(IndexMap<String, Value>),
    Map(Vec<(Value, Value)>),
    Function(ObjFunction),
    Closure {
        function: ObjFunction,
        /// Slots of the closure's upvalue cells.
        upvalues: Vec<usize>,
    },
    Native(String),
    ResultOk(Value),
    ResultErr(Value),
    Frozen(Value),
    BoxedInt(i64),
    TaskHandle(Arc<(std::sync::Mutex<Option<SharedValue>>, std::sync::Condvar)>),
    Channel(Arc<VmChannelInner>),
}

/// Per-type method / static-method / struct-default tables, frozen.
type FrozenTables = Vec<(String, IndexMap<String, Value>)>;

/// Read-only snapshot of a VM after its top level ran; the [`ServeEngine`]
/// the HTTP server forks per request. See the module docs.
pub struct VmTemplate {
    /// Number of template slots (cells + objects).
    slots: usize,
    /// Upvalue cells: `(slot, value)`.
    cells: Vec<(usize, Value)>,
    /// Every other object, `(slot, object)`, children before parents.
    objects: Vec<(usize, FrozenObj)>,
    globals: Vec<(String, Value)>,
    method_tables: FrozenTables,
    static_methods: FrozenTables,
    struct_defaults: FrozenTables,
    embedded_fields: HashMap<String, Vec<(String, String)>>,
    /// Parameter names of top-level functions, by function name; the VM's
    /// chunks do not keep them (see `metadata::top_level_fn_params`).
    fn_params: Arc<HashMap<String, Vec<String>>>,
    #[cfg(feature = "jit")]
    jit_mode: super::jit::tier::JitMode,
}

impl VmTemplate {
    /// Freeze `vm` — which has just run a program's top level — into a
    /// template. `fn_params` maps top-level function names to their
    /// parameter names.
    ///
    /// Fails if a top-level value holds a stream: streams are single-use, so
    /// one shared by every request would be drained by whichever ran first.
    /// (The interpreter rejects the same programs at the first request in
    /// debug builds; the VM reports it before the server starts listening.)
    pub fn new(vm: &VM, fn_params: HashMap<String, Vec<String>>) -> Result<Self, String> {
        let mut freezer = Freezer::new(&vm.gc);
        let globals = freezer.freeze_named(vm.globals.iter())?;
        let method_tables = freezer.freeze_tables(&vm.method_tables)?;
        let static_methods = freezer.freeze_tables(&vm.static_methods)?;
        let struct_defaults = freezer.freeze_tables(&vm.struct_defaults)?;
        Ok(Self {
            slots: freezer.next_slot,
            cells: freezer.cells,
            objects: freezer.objects,
            globals,
            method_tables,
            static_methods,
            struct_defaults,
            embedded_fields: vm.embedded_fields.clone(),
            fn_params: Arc::new(fn_params),
            #[cfg(feature = "jit")]
            jit_mode: vm.jit.mode,
        })
    }

    /// A fresh VM holding a private copy of the template's state, polling
    /// `cancelled` at its safe points.
    pub fn fork(&self, cancelled: Arc<AtomicBool>) -> VM {
        let mut vm = VM::bare(Profiler::new(false));
        let refs = self.materialize(&mut vm.gc);
        let remap = |v: &Value| remap_value(*v, &refs);
        let remap_table = |table: &IndexMap<String, Value>| {
            table
                .iter()
                .map(|(k, v)| (k.clone(), remap(v)))
                .collect::<IndexMap<_, _>>()
        };
        vm.globals = self
            .globals
            .iter()
            .map(|(k, v)| (k.clone(), remap(v)))
            .collect();
        vm.method_tables = self
            .method_tables
            .iter()
            .map(|(k, t)| (k.clone(), remap_table(t)))
            .collect();
        vm.static_methods = self
            .static_methods
            .iter()
            .map(|(k, t)| (k.clone(), remap_table(t)))
            .collect();
        vm.struct_defaults = self
            .struct_defaults
            .iter()
            .map(|(k, t)| (k.clone(), remap_table(t)))
            .collect();
        vm.embedded_fields = self.embedded_fields.clone();
        vm.set_cancel_flag(cancelled);
        #[cfg(feature = "jit")]
        {
            vm.jit.mode = self.jit_mode;
        }
        vm
    }

    /// Allocate the template's objects in `gc`; returns slot → new ref.
    fn materialize(&self, gc: &mut Gc) -> Vec<GcRef> {
        let mut refs = vec![GcRef(usize::MAX); self.slots];
        // 1. Cells first, empty: closures below refer to them.
        for (slot, _) in &self.cells {
            refs[*slot] = gc.alloc(ObjKind::Upvalue(ObjUpvalue {
                value: Value::null(),
            }));
        }
        // 2. Objects in allocation order: every child already exists.
        for (slot, object) in &self.objects {
            refs[*slot] = match object {
                // Short strings are interned, so constants the handler
                // loads later resolve to these objects without allocating.
                FrozenObj::String(s) => gc.alloc_str(s),
                other => gc.alloc(thaw(other, &refs)),
            };
        }
        // 3. Fill the cells now that everything they point to exists.
        for (slot, value) in &self.cells {
            let value = remap_value(*value, &refs);
            if let Some(cell) = gc.get_mut(refs[*slot]) {
                cell.kind = ObjKind::Upvalue(ObjUpvalue { value });
            }
        }
        refs
    }

    fn worker(&self, cancelled: Arc<AtomicBool>) -> VmWorker {
        VmWorker {
            vm: self.fork(cancelled),
            fn_params: Arc::clone(&self.fn_params),
        }
    }
}

/// The live object for template object `object`, whose children are
/// already allocated (`refs`: slot → live ref).
fn thaw(object: &FrozenObj, refs: &[GcRef]) -> ObjKind {
    let r = |v: &Value| remap_value(*v, refs);
    let r_all = |vs: &[Value]| vs.iter().map(r).collect::<Vec<_>>();
    match object {
        FrozenObj::String(s) => ObjKind::String(s.clone()),
        FrozenObj::Array(items) => ObjKind::Array(r_all(items)),
        FrozenObj::Tuple(items) => ObjKind::Tuple(r_all(items)),
        FrozenObj::Set(items) => ObjKind::Set(r_all(items)),
        FrozenObj::Object(map) => {
            ObjKind::Object(map.iter().map(|(k, v)| (k.clone(), r(v))).collect())
        }
        FrozenObj::Map(pairs) => ObjKind::Map(pairs.iter().map(|(k, v)| (r(k), r(v))).collect()),
        FrozenObj::Function(f) => ObjKind::Function(f.clone()),
        FrozenObj::Closure { function, upvalues } => ObjKind::Closure(ObjClosure {
            function: function.clone(),
            upvalues: upvalues.iter().map(|slot| refs[*slot]).collect(),
        }),
        FrozenObj::Native(name) => ObjKind::NativeFunction(NativeFn { name: name.clone() }),
        FrozenObj::ResultOk(v) => ObjKind::ResultOk(r(v)),
        FrozenObj::ResultErr(v) => ObjKind::ResultErr(r(v)),
        FrozenObj::Frozen(v) => ObjKind::Frozen(r(v)),
        FrozenObj::BoxedInt(n) => ObjKind::BoxedInt(*n),
        FrozenObj::TaskHandle(handle) => ObjKind::TaskHandle(Arc::clone(handle)),
        FrozenObj::Channel(channel) => ObjKind::Channel(Arc::clone(channel)),
    }
}

fn remap_value(v: Value, refs: &[GcRef]) -> Value {
    match v.as_obj() {
        Some(GcRef(slot)) => Value::obj(refs[slot]),
        None => v,
    }
}

impl ServeEngine for VmTemplate {
    fn name(&self) -> &'static str {
        "vm"
    }

    fn fork_request(&self, cancelled: Arc<AtomicBool>) -> Box<dyn HandlerWorker> {
        Box::new(self.worker(cancelled))
    }

    fn fork_connection(&self, cancelled: Arc<AtomicBool>) -> Box<dyn HandlerWorker + Send> {
        let mut worker = self.worker(cancelled);
        // A connection's worker moves between blocking-pool threads, and
        // JIT state (compiled code, the Cranelift module) is not `Send`:
        // the connection VM runs without the JIT. See `ConnectionWorker`.
        #[cfg(feature = "jit")]
        {
            worker.vm.jit.mode = super::jit::tier::JitMode::Off;
        }
        Box::new(ConnectionWorker(worker))
    }
}

/// One forked VM serving one request (or, wrapped in [`ConnectionWorker`],
/// one WebSocket connection).
struct VmWorker {
    vm: VM,
    fn_params: Arc<HashMap<String, Vec<String>>>,
}

impl VmWorker {
    /// Arguments for `handler`, bound like the interpreter binds them: a
    /// named function gets its parameters filled from the request (see
    /// `server::handler_args`); a lambda gets none.
    fn http_args(&mut self, handler: Value, request: &HandlerRequest) -> Vec<Value> {
        let name = handler
            .as_obj()
            .and_then(|r| self.vm.gc.get(r))
            .and_then(|obj| match &obj.kind {
                ObjKind::Closure(c) => Some(c.function.name.clone()),
                ObjKind::Function(f) => Some(f.name.clone()),
                _ => None,
            });
        let Some(params) = name.and_then(|name| self.fn_params.get(&name)) else {
            return Vec::new();
        };
        handler_args(params.iter().map(String::as_str), request)
            .iter()
            .map(|arg| self.vm.convert_interp_value(arg))
            .collect()
    }
}

impl HandlerWorker for VmWorker {
    fn call_http(&mut self, handler: &str, request: &HandlerRequest) -> (StatusCode, JsonValue) {
        let Some(func) = self.vm.globals.get(handler).copied() else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": format!("handler '{}' not found", handler)}),
            );
        };
        let args = self.http_args(func, request);
        match self.vm.call_value(func, args) {
            Ok(value) => (StatusCode::OK, value_to_json(&self.vm.gc, value)),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": e.message}),
            ),
        }
    }

    fn call_ws(&mut self, handler: &str, text: String) -> String {
        let Some(func) = self.vm.globals.get(handler).copied() else {
            return "handler not found".to_string();
        };
        let arg = Value::obj(self.vm.gc.alloc_string(text));
        match self.vm.call_value(func, vec![arg]) {
            Ok(value) => value.display(&self.vm.gc),
            Err(e) => format!("error: {}", e.message),
        }
    }
}

/// A WebSocket connection's worker. The server keeps it behind a mutex and
/// runs each message on whichever blocking-pool thread is free, so it must
/// be `Send`.
struct ConnectionWorker(VmWorker);

// SAFETY: everything a `VM` owns is either owned data or `Arc`-shared
// thread-safe state, except JIT state (compiled code handles and the
// Cranelift module), which is why `VM` is not `Send` with the `jit` feature.
// `fork_connection` turns the JIT off before the worker is created, so that
// state stays empty for the worker's whole life; `call_*` assert it (in
// release builds too, as `SendableVM` does) before the worker can be moved
// again.
unsafe impl Send for ConnectionWorker {}

impl ConnectionWorker {
    fn assert_sendable(&self) {
        #[cfg(feature = "jit")]
        assert!(
            self.0.vm.jit.is_empty(),
            "BUG: a WebSocket connection VM must not hold JIT state"
        );
    }
}

impl HandlerWorker for ConnectionWorker {
    fn call_http(&mut self, handler: &str, request: &HandlerRequest) -> (StatusCode, JsonValue) {
        let result = self.0.call_http(handler, request);
        self.assert_sendable();
        result
    }

    fn call_ws(&mut self, handler: &str, text: String) -> String {
        let result = self.0.call_ws(handler, text);
        self.assert_sendable();
        result
    }
}

/// JSON encoding of a handler's return value. Mirrors
/// `server::forge_to_json` for the interpreter value each VM value
/// corresponds to, so both engines send the same body.
pub(crate) fn value_to_json(gc: &Gc, value: Value) -> JsonValue {
    let opaque = |type_name: &str| JsonValue::String(format!("<{}>", type_name));
    let r = match value.classify(gc) {
        ValueKind::Int(n) => return JsonValue::Number(n.into()),
        ValueKind::Float(n) => {
            return serde_json::Number::from_f64(n)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null)
        }
        ValueKind::Bool(b) => return JsonValue::Bool(b),
        ValueKind::Null => return JsonValue::Null,
        ValueKind::Obj(r) => r,
    };
    let Some(obj) = gc.get(r) else {
        return JsonValue::Null;
    };
    match &obj.kind {
        ObjKind::String(s) => JsonValue::String(s.clone()),
        ObjKind::BoxedInt(n) => JsonValue::Number((*n).into()),
        ObjKind::Array(items) => {
            JsonValue::Array(items.iter().map(|v| value_to_json(gc, *v)).collect())
        }
        ObjKind::Object(map) if is_option(gc, map) => opaque("Option"),
        ObjKind::Object(map) => JsonValue::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), value_to_json(gc, *v)))
                .collect(),
        ),
        ObjKind::ResultOk(v) => serde_json::json!({ "Ok": value_to_json(gc, *v) }),
        ObjKind::ResultErr(v) => serde_json::json!({ "Err": value_to_json(gc, *v) }),
        ObjKind::Frozen(inner) => opaque(&interpreter_type_name(gc, *inner)),
        _ => opaque(&interpreter_type_name(gc, value)),
    }
}

/// Chunk name the compiler gives anonymous functions.
const LAMBDA_NAME: &str = "<lambda>";

/// `Some(x)` / `None` are tagged objects on the VM, `Value::Some` /
/// `Value::None` in the interpreter.
fn is_option(gc: &Gc, map: &IndexMap<String, Value>) -> bool {
    map.get("__type__")
        .and_then(|v| v.as_obj())
        .and_then(|r| gc.get(r))
        .is_some_and(|o| matches!(&o.kind, ObjKind::String(s) if s == "Option"))
}

/// The interpreter's `Value::type_name` for the value `value` converts to.
fn interpreter_type_name(gc: &Gc, value: Value) -> String {
    let name = match value.classify(gc) {
        ValueKind::Int(_) => "Int",
        ValueKind::Float(_) => "Float",
        ValueKind::Bool(_) => "Bool",
        ValueKind::Null => "Null",
        ValueKind::Obj(r) => match gc.get(r).map(|o| &o.kind) {
            Some(ObjKind::String(_)) => "String",
            Some(ObjKind::BoxedInt(_)) => "Int",
            Some(ObjKind::Array(_)) => "Array",
            Some(ObjKind::Object(map)) if is_option(gc, map) => "Option",
            Some(ObjKind::Object(_)) => "Object",
            Some(ObjKind::ResultOk(_) | ObjKind::ResultErr(_)) => "Result",
            Some(ObjKind::Frozen(inner)) => return interpreter_type_name(gc, *inner),
            Some(ObjKind::Tuple(_)) => "Tuple",
            Some(ObjKind::Set(_)) => "Set",
            Some(ObjKind::Map(_)) => "Map",
            Some(ObjKind::Stream(_)) => "Stream",
            // Named `fn`s are `Value::Function`, anonymous ones
            // `Value::Lambda` in the interpreter.
            Some(ObjKind::Closure(c)) if c.function.name == LAMBDA_NAME => "Lambda",
            Some(ObjKind::Function(f)) if f.name == LAMBDA_NAME => "Lambda",
            Some(ObjKind::Function(_) | ObjKind::Closure(_)) => "Function",
            Some(ObjKind::NativeFunction(_)) => "BuiltIn",
            Some(ObjKind::TaskHandle(_)) => "TaskHandle",
            Some(ObjKind::Channel(_)) => "Channel",
            Some(ObjKind::Upvalue(_)) | None => "Null",
        },
    };
    name.to_string()
}

/// Builds a [`VmTemplate`]'s object list from a live heap.
struct Freezer<'a> {
    gc: &'a Gc,
    /// Live ref → template slot, for objects already frozen.
    slot_of: HashMap<usize, usize>,
    /// Objects whose children are still being frozen (cycle detection).
    in_progress: std::collections::HashSet<usize>,
    next_slot: usize,
    cells: Vec<(usize, Value)>,
    objects: Vec<(usize, FrozenObj)>,
    /// Cells allocated but not yet filled: `(slot, live value)`.
    pending_cells: Vec<(usize, Value)>,
}

impl<'a> Freezer<'a> {
    fn new(gc: &'a Gc) -> Self {
        Self {
            gc,
            slot_of: HashMap::new(),
            in_progress: std::collections::HashSet::new(),
            next_slot: 0,
            cells: Vec::new(),
            objects: Vec::new(),
            pending_cells: Vec::new(),
        }
    }

    fn freeze_named<'v>(
        &mut self,
        entries: impl Iterator<Item = (&'v String, &'v Value)>,
    ) -> Result<Vec<(String, Value)>, String> {
        entries
            .map(|(k, v)| Ok((k.clone(), self.freeze(*v)?)))
            .collect()
    }

    fn freeze_tables(
        &mut self,
        tables: &HashMap<String, IndexMap<String, Value>>,
    ) -> Result<FrozenTables, String> {
        tables
            .iter()
            .map(|(name, table)| {
                let frozen = table
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), self.freeze(*v)?)))
                    .collect::<Result<IndexMap<_, _>, String>>()?;
                Ok((name.clone(), frozen))
            })
            .collect()
    }

    /// Freeze `value` and everything reachable from it; returns the value
    /// re-pointed at template slots.
    fn freeze(&mut self, value: Value) -> Result<Value, String> {
        let frozen = self.freeze_shallow(value)?;
        while let Some((slot, live)) = self.pending_cells.pop() {
            let value = self.freeze_shallow(live)?;
            self.cells.push((slot, value));
        }
        Ok(frozen)
    }

    fn alloc_slot(&mut self) -> usize {
        let slot = self.next_slot;
        self.next_slot += 1;
        slot
    }

    /// Freeze `value`'s object graph, stopping at upvalue cells: those get
    /// a slot immediately and their contents are queued in `pending_cells`.
    /// Iterative post-order DFS, so deep data cannot overflow the stack.
    fn freeze_shallow(&mut self, value: Value) -> Result<Value, String> {
        let Some(root) = value.as_obj() else {
            return Ok(value);
        };
        let mut stack = vec![(root, false)];
        while let Some((r, children_done)) = stack.pop() {
            if self.slot_of.contains_key(&r.0) {
                continue;
            }
            let obj = self
                .gc
                .get(r)
                .ok_or_else(|| "BUG: dangling reference in the server template".to_string())?;
            if let ObjKind::Upvalue(cell) = &obj.kind {
                let slot = self.alloc_slot();
                self.slot_of.insert(r.0, slot);
                self.pending_cells.push((slot, cell.value));
                continue;
            }
            if !children_done {
                if !self.in_progress.insert(r.0) {
                    return Err(
                        "BUG: reference cycle outside an upvalue in the server template"
                            .to_string(),
                    );
                }
                stack.push((r, true));
                let mut children = Vec::new();
                obj.trace(&mut children);
                for child in children {
                    if !self.slot_of.contains_key(&child.0) {
                        stack.push((child, false));
                    }
                }
                continue;
            }
            self.in_progress.remove(&r.0);
            let frozen = self.freeze_object(&obj.kind)?;
            let slot = self.alloc_slot();
            self.slot_of.insert(r.0, slot);
            self.objects.push((slot, frozen));
        }
        Ok(self.mapped(value))
    }

    /// Template slot of live object `r`, which must already be frozen
    /// (post-order guarantees it for every child of the object being
    /// frozen).
    fn slot(&self, r: GcRef) -> usize {
        *self
            .slot_of
            .get(&r.0)
            .expect("BUG: server template child frozen after its parent")
    }

    fn mapped(&self, value: Value) -> Value {
        match value.as_obj() {
            Some(r) => Value::obj(GcRef(self.slot(r))),
            None => value,
        }
    }

    /// Copy one object whose children are all frozen.
    fn freeze_object(&self, kind: &ObjKind) -> Result<FrozenObj, String> {
        let m = |v: &Value| self.mapped(*v);
        let m_all = |vs: &[Value]| vs.iter().map(m).collect::<Vec<_>>();
        Ok(match kind {
            ObjKind::String(s) => FrozenObj::String(s.clone()),
            ObjKind::Array(items) => FrozenObj::Array(m_all(items)),
            ObjKind::Tuple(items) => FrozenObj::Tuple(m_all(items)),
            ObjKind::Set(items) => FrozenObj::Set(m_all(items)),
            ObjKind::Object(map) => {
                FrozenObj::Object(map.iter().map(|(k, v)| (k.clone(), m(v))).collect())
            }
            ObjKind::Map(pairs) => {
                FrozenObj::Map(pairs.iter().map(|(k, v)| (m(k), m(v))).collect())
            }
            ObjKind::Function(f) => FrozenObj::Function(f.clone()),
            ObjKind::Closure(c) => FrozenObj::Closure {
                function: c.function.clone(),
                upvalues: c.upvalues.iter().map(|uv| self.slot(*uv)).collect(),
            },
            ObjKind::NativeFunction(nf) => FrozenObj::Native(nf.name.clone()),
            ObjKind::ResultOk(v) => FrozenObj::ResultOk(m(v)),
            ObjKind::ResultErr(v) => FrozenObj::ResultErr(m(v)),
            ObjKind::Frozen(v) => FrozenObj::Frozen(m(v)),
            ObjKind::BoxedInt(n) => FrozenObj::BoxedInt(*n),
            ObjKind::TaskHandle(handle) => FrozenObj::TaskHandle(Arc::clone(handle)),
            ObjKind::Channel(channel) => FrozenObj::Channel(Arc::clone(channel)),
            ObjKind::Stream(_) => {
                return Err(
                    "a top-level value holds a stream; streams are single-use and \
                     cannot be shared by every request. Construct streams inside handlers, \
                     not at the top level."
                        .to_string(),
                )
            }
            ObjKind::Upvalue(_) => {
                return Err("BUG: upvalue cell reached freeze_object".to_string())
            }
        })
    }
}
