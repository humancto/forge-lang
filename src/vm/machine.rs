use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::bytecode::*;
use super::frame::*;
use super::gc::Gc;
use super::profiler::Profiler;
use super::value::*;

/// Wrapper for sending a VM to another thread.
/// SAFETY: fork_for_spawn() asserts the JIT state is empty (no raw
/// pointers cross threads). All other VM fields are owned or Arc-wrapped.
/// The assert runs in release builds to prevent UB if the invariant breaks.
struct SendableVM(VM);
unsafe impl Send for SendableVM {}

/// Run a spawned closure on a forked VM in a new OS thread. `task` is the
/// run's task slot (`runtime::limits`), held until the task finishes.
fn spawn_thread(
    sendable: SendableVM,
    closure: Value,
    slot: Arc<(Mutex<Option<SharedValue>>, Condvar)>,
    task: crate::runtime::limits::Slot,
) {
    crate::permissions::spawn(move || {
        sendable.run(closure, slot, task);
    });
}

/// Run a schedule closure in a loop on a forked VM in a new OS thread.
fn spawn_schedule_thread(sendable: SendableVM, closure: Value, interval: Duration) {
    crate::permissions::spawn(move || {
        sendable.run_loop(closure, interval);
    });
}

/// Run a watch closure on a forked VM, polling a file path for mtime changes.
fn spawn_watch_thread(sendable: SendableVM, closure: Value, path: String) {
    crate::permissions::spawn(move || {
        sendable.run_watch(closure, path);
    });
}

impl SendableVM {
    fn run(
        mut self,
        closure: Value,
        slot: Arc<(Mutex<Option<SharedValue>>, Condvar)>,
        task: crate::runtime::limits::Slot,
    ) {
        let vm = &mut self.0;
        let val = match vm.call_value(closure, vec![]) {
            Ok(v) => {
                let sv = value_to_shared(&vm.gc, &v);
                if vm.check_stream_boundary().is_err() {
                    SharedValue::ResultErr(Box::new(SharedValue::String(
                        "Stream cannot cross the VM/interpreter boundary; call .collect() first to materialize".to_string(),
                    )))
                } else {
                    SharedValue::ResultOk(Box::new(sv))
                }
            }
            Err(e) => SharedValue::ResultErr(Box::new(SharedValue::String(e.message.clone()))),
        };
        // Free the task slot before anyone can observe the result, so a
        // caller that awaits and immediately spawns again is not refused.
        drop(task);
        if let Ok(mut guard) = slot.0.lock() {
            *guard = Some(val);
            slot.1.notify_all();
        }
    }

    fn run_loop(mut self, closure: Value, interval: Duration) {
        let vm = &mut self.0;
        // Root the closure in register 0 so GC can't collect it between calls
        if vm.registers.is_empty() {
            vm.registers.push(closure);
        } else {
            vm.registers[0] = closure;
        }
        loop {
            std::thread::sleep(interval);
            let _ = vm.call_value(closure, vec![]);
            // Re-root after call (call_value may have modified registers)
            if vm.registers.is_empty() {
                vm.registers.push(closure);
            } else {
                vm.registers[0] = closure;
            }
        }
    }

    fn run_watch(mut self, closure: Value, path: String) {
        let vm = &mut self.0;
        // Root the closure in register 0 so GC can't collect it between calls
        if vm.registers.is_empty() {
            vm.registers.push(closure);
        } else {
            vm.registers[0] = closure;
        }
        let mut last_modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let current = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if current != last_modified {
                last_modified = current;
                let _ = vm.call_value(closure, vec![]);
                // Re-root after call
                if vm.registers.is_empty() {
                    vm.registers.push(closure);
                } else {
                    vm.registers[0] = closure;
                }
            }
        }
    }
}

/// Instructions executed between two polls of `timeout` deadlines. Polling
/// reads the clock and walks every frame, so it must not run per
/// instruction; 1024 simple instructions take a few microseconds, far below
/// the one-second resolution of `timeout` scopes. Cancellation (squads,
/// HTTP cancel-on-drop) is a single atomic load and is still checked on
/// every backward jump and call.
pub(super) const SAFEPOINT_INTERVAL: u32 = 1024;

/// Backward jumps after which a frame's function is offered to the JIT
/// (see `VM::try_jit_loop_restart`). Low enough that a loop-heavy function
/// called once still tiers up early, high enough that short loops never
/// pay for compilation.
#[cfg(feature = "jit")]
pub(super) const LOOP_HOT_THRESHOLD: u32 = 1000;

/// Compiler intrinsics: natives the bytecode compiler emits calls to. They
/// are not user-visible builtins (those live in `crate::builtins_registry`).
const COMPILER_INTRINSICS: &[&str] = &[
    "__forge_register_struct",
    "__forge_new_struct",
    "__forge_register_interface",
    "__forge_register_method",
    "__forge_validate_impl",
    "__forge_call_method",
    "__forge_binding_matches",
    "__forge_retry_count",
    "__forge_retry_wait",
    "__forge_retry_failed",
    "__forge_where_filter",
    "__forge_pipe_sort",
    "__forge_pipe_take",
    "__forge_register_prompt",
    "__forge_register_agent",
    "__forge_raise_error",
    "__forge_import_module",
    "__forge_import_native",
    "__forge_get_field",
    "__forge_set_field",
    "__forge_destructure",
    "__forge_array_spread",
    "__forge_check",
    "__forge_when_matches",
    "__forge_method_mut",
];

pub struct VM {
    pub registers: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub globals: HashMap<String, Value>,
    pub method_tables: HashMap<String, IndexMap<String, Value>>,
    pub static_methods: HashMap<String, IndexMap<String, Value>>,
    pub embedded_fields: HashMap<String, Vec<(String, String)>>,
    pub struct_defaults: HashMap<String, IndexMap<String, Value>>,
    pub gc: Gc,
    pub output: Vec<String>,
    /// JIT tier state: specialization cache keyed by (prototype id, type
    /// signature), hotness and deopt accounting. See `vm::jit`.
    #[cfg(feature = "jit")]
    pub jit: super::jit::tier::JitState,
    /// Error raised inside a runtime bridge (`vm::jit::runtime`) while
    /// native code was running. Bridges are `extern "C"` and cannot return
    /// a `Result`, so they record the error here and return a placeholder;
    /// `try_jit_call` turns it into the call's error. Never silently
    /// dropped.
    #[cfg(feature = "jit")]
    pub(crate) jit_bridge_error: Option<VMError>,
    pub profiler: Profiler,
    /// Instructions left before the next safe-point poll of `timeout`
    /// deadlines (see [`SAFEPOINT_INTERVAL`]). Reading the clock and walking
    /// every frame's timeout stack on each instruction dominated the cost of
    /// simple loops, so deadlines are polled every `SAFEPOINT_INTERVAL`
    /// instructions instead. `PushTimeout` zeroes it so a scope that is
    /// already expired (`timeout 0 seconds`) fires before its body runs, and
    /// a fired timeout refills it so the catch path's `PopTimeout` runs
    /// before the next poll.
    safepoint_countdown: u32,
    /// Instructions in the current safe-point window (the countdown it
    /// started with + 1). `fuel_window - safepoint_countdown` is the number
    /// executed so far, which the next safe point charges to the fuel budget
    /// (see `runtime::limits::Meter`).
    fuel_window: u64,
    /// Fuel / fatal-limit accounting against the run's resource budget.
    meter: crate::runtime::limits::Meter,
    /// Size caps for strings and collections the VM builds.
    pub(super) caps: crate::runtime::limits::Caps,
    /// Set by the Stream arms of `convert_to_interp_val` / `convert_interp_value`
    /// / `value_to_shared` when a Stream is encountered at the VM↔interpreter
    /// boundary. Callers of those conversions must check this flag after each
    /// call and surface a `VMError` via `check_stream_boundary`. Streams are
    /// single-use and cannot cross engine boundaries — callers must
    /// `.collect()` first to materialize. (M9.4 bug #6.)
    pub(super) stream_boundary_error: std::cell::Cell<bool>,
    /// Squad handle collector stack: when non-empty, Spawn registers handles here.
    /// Each entry is (dst_register, cancel_flag, handles, saved_outer_cancelled).
    /// Values received by `IterHas` from a channel being iterated by a
    /// `for` loop, consumed by the `IterGet` that immediately follows.
    iter_prefetch: Vec<SharedValue>,
    squad_stack: Vec<(
        u8,
        Arc<std::sync::atomic::AtomicBool>,
        Vec<Arc<(Mutex<Option<SharedValue>>, Condvar)>>,
        Arc<std::sync::atomic::AtomicBool>,
    )>,
    /// Cooperative cancellation flag — shared with squad parent, checked at safe points.
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorControl {
    Runtime,
    UnwoundToHandler,
    /// A fatal resource limit (fuel, memory): never caught by `try`/`safe`.
    Fatal,
}

#[derive(Debug)]
pub struct VMError {
    pub message: String,
    pub stack_trace: Vec<StackFrame>,
    control: ErrorControl,
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function: String,
    pub line: usize,
    pub col: usize,
}

impl VMError {
    pub fn new(msg: &str) -> Self {
        Self {
            message: msg.to_string(),
            stack_trace: Vec::new(),
            control: ErrorControl::Runtime,
        }
    }

    #[allow(dead_code)]
    pub fn with_trace(msg: &str, trace: Vec<StackFrame>) -> Self {
        Self {
            message: msg.to_string(),
            stack_trace: trace,
            control: ErrorControl::Runtime,
        }
    }

    pub fn unwound_to_handler() -> Self {
        Self {
            message: "internal control transfer to catch handler".to_string(),
            stack_trace: Vec::new(),
            control: ErrorControl::UnwoundToHandler,
        }
    }

    pub fn is_unwound_to_handler(&self) -> bool {
        self.control == ErrorControl::UnwoundToHandler
    }

    /// A fatal resource-limit error (`runtime::limits`): it unwinds past
    /// every handler.
    pub fn fatal(msg: &str) -> Self {
        Self {
            message: msg.to_string(),
            stack_trace: Vec::new(),
            control: ErrorControl::Fatal,
        }
    }

    pub fn is_fatal(&self) -> bool {
        self.control == ErrorControl::Fatal
    }
}

impl std::fmt::Display for VMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        // Runs of identical frames (deep recursion) are collapsed after
        // `SHOWN_REPEATS` copies so a depth-limit error stays readable.
        const SHOWN_REPEATS: usize = 3;
        let mut i = 0;
        while i < self.stack_trace.len() {
            let frame = &self.stack_trace[i];
            let run = self.stack_trace[i..]
                .iter()
                .take_while(|g| {
                    g.function == frame.function && g.line == frame.line && g.col == frame.col
                })
                .count();
            for _ in 0..run.min(SHOWN_REPEATS) {
                if frame.col > 0 {
                    write!(
                        f,
                        "\n  at {} (line {}, col {})",
                        frame.function, frame.line, frame.col
                    )?;
                } else {
                    write!(f, "\n  at {} (line {})", frame.function, frame.line)?;
                }
            }
            if run > SHOWN_REPEATS {
                write!(
                    f,
                    "\n  ... previous frame repeated {} more times",
                    run - SHOWN_REPEATS
                )?;
            }
            i += run;
        }
        Ok(())
    }
}

impl VM {
    pub fn new() -> Self {
        let (meter, caps, gc) = Self::limits_state();
        let mut vm = Self {
            registers: vec![Value::null(); 256],
            frames: Vec::with_capacity(INITIAL_FRAME_CAPACITY),
            globals: HashMap::new(),
            method_tables: HashMap::new(),
            static_methods: HashMap::new(),
            embedded_fields: HashMap::new(),
            struct_defaults: HashMap::new(),
            gc,
            output: Vec::new(),
            #[cfg(feature = "jit")]
            jit: super::jit::tier::JitState::default(),
            #[cfg(feature = "jit")]
            jit_bridge_error: None,
            profiler: Profiler::new(false),
            safepoint_countdown: 0,
            fuel_window: 0,
            meter,
            caps,
            stream_boundary_error: std::cell::Cell::new(false),
            squad_stack: Vec::new(),
            iter_prefetch: Vec::new(),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        vm.register_builtins();
        vm
    }

    pub fn with_profiling() -> Self {
        let (meter, caps, gc) = Self::limits_state();
        let mut vm = Self {
            registers: vec![Value::null(); 256],
            frames: Vec::with_capacity(INITIAL_FRAME_CAPACITY),
            globals: HashMap::new(),
            method_tables: HashMap::new(),
            static_methods: HashMap::new(),
            embedded_fields: HashMap::new(),
            struct_defaults: HashMap::new(),
            gc,
            output: Vec::new(),
            #[cfg(feature = "jit")]
            jit: super::jit::tier::JitState::default(),
            #[cfg(feature = "jit")]
            jit_bridge_error: None,
            profiler: Profiler::new(true),
            safepoint_countdown: 0,
            fuel_window: 0,
            meter,
            caps,
            stream_boundary_error: std::cell::Cell::new(false),
            squad_stack: Vec::new(),
            iter_prefetch: Vec::new(),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        vm.register_builtins();
        vm
    }

    /// Resource-limit state for a new VM: the budget active on this thread
    /// (`runtime::limits`) decides the fuel meter, the size caps and the GC
    /// heap's memory limit.
    fn limits_state() -> (
        crate::runtime::limits::Meter,
        crate::runtime::limits::Caps,
        Gc,
    ) {
        let meter = crate::runtime::limits::Meter::current();
        let limits = meter
            .budget()
            .map(|b| b.limits().clone())
            .unwrap_or_default();
        let gc = Gc::with_memory_limit(limits.max_memory);
        (meter, limits.caps(), gc)
    }

    /// Fail the run with a fatal resource-limit error: it skips every
    /// `try`/`safe` handler and unwinds this `run_until` invocation.
    fn raise_fatal(&mut self, message: &str, boundary_frame_idx: usize) -> VMError {
        // The countdown stays at 0, so every later instruction goes back
        // through the safe point and fails again (the trip is sticky).
        self.safepoint_countdown = 0;
        self.fuel_window = 0;
        match self.handle_runtime_error(VMError::fatal(message), boundary_frame_idx) {
            Err(e) => e,
            Ok(_) => VMError::fatal(message),
        }
    }

    fn register_builtins(&mut self) {
        // User-visible globals come from the shared registry, so the VM and
        // the interpreter expose exactly the same builtins.
        let globals = crate::builtins_registry::GLOBALS.iter().map(|b| b.name);
        for name in globals.chain(COMPILER_INTRINSICS.iter().copied()) {
            let name_ref = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: name.to_string(),
            }));
            self.globals.insert(name.to_string(), Value::obj(name_ref));
        }

        self.globals.insert("null".to_string(), Value::null());

        // Register stdlib modules
        self.register_stdlib();
    }

    /// Build every stdlib module object from the shared registry. Members
    /// are `NativeFunction`s named `module.member`; `dispatch_native`
    /// routes them to the shared implementation (`builtins_registry::
    /// call_module`), so a module available on the interpreter is available
    /// here with the same members.
    fn register_stdlib(&mut self) {
        use crate::interpreter::Value as IV;
        for module in crate::builtins_registry::modules() {
            let IV::Object(members) = (module.create)() else {
                continue;
            };
            let mut map = IndexMap::new();
            for (key, member) in members {
                let value = match &member {
                    IV::BuiltIn(name) => self.alloc_builtin(name),
                    other => self.convert_interp_value(other),
                };
                map.insert(key, value);
            }
            if module.name == "time" {
                // `time()` called as a function returns the current datetime.
                let time_call = self.alloc_builtin("time");
                map.insert("__call__".to_string(), time_call);
            }
            let module_ref = self.gc.alloc(ObjKind::Object(map));
            self.globals
                .insert(module.name.to_string(), Value::obj(module_ref));
        }

        // Option prelude
        let mut none_obj = IndexMap::new();
        none_obj.insert("__type__".to_string(), self.alloc_string("Option"));
        none_obj.insert("__variant__".to_string(), self.alloc_string("None"));
        let none_ref = self.gc.alloc(ObjKind::Object(none_obj));
        self.globals
            .insert("None".to_string(), Value::obj(none_ref));
    }

    pub(super) fn alloc_string(&mut self, s: &str) -> Value {
        let r = self.gc.alloc_str(s);
        Value::obj(r)
    }

    pub(super) fn alloc_builtin(&mut self, name: &str) -> Value {
        let native = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
            name: name.to_string(),
        }));
        Value::obj(native)
    }

    #[inline]
    fn constant_to_value(&mut self, constant: &Constant) -> Value {
        match constant {
            Constant::Int(n) => Value::int(*n, &mut self.gc),
            Constant::Float(n) => Value::float(*n),
            Constant::Bool(b) => Value::bool_val(*b),
            Constant::Null => Value::null(),
            Constant::Str(s) => Value::obj(self.gc.alloc_str(s)),
        }
    }

    pub(super) fn get_string(&self, val: &Value) -> Option<String> {
        if let Some(r) = val.as_obj() {
            if let Some(obj) = self.gc.get(r) {
                if let ObjKind::String(s) = &obj.kind {
                    return Some(s.clone());
                }
            }
        }
        None
    }

    /// Create a new VM for a spawn thread with copies of this VM's state.
    /// Calls VM::new() for fresh builtins + empty JIT state, then copies
    /// non-function globals and struct metadata from the parent.
    fn fork_for_spawn(&self) -> SendableVM {
        let mut child = VM::new();

        // Copy non-function globals. Skip globals where value_to_shared returns
        // Null but the original wasn't Null (i.e., functions/closures/natives) —
        // these would overwrite the child's freshly-registered builtins.
        for (name, val) in &self.globals {
            let shared = value_to_shared(&self.gc, val);
            if matches!(shared, SharedValue::Null) && !val.is_null() {
                continue;
            }
            let child_val = shared_to_value(&mut child.gc, &shared);
            child.globals.insert(name.clone(), child_val);
        }

        for (name, methods) in &self.method_tables {
            let mut child_methods = IndexMap::new();
            for (k, v) in methods {
                let shared = value_to_shared(&self.gc, v);
                if matches!(shared, SharedValue::Null) && !v.is_null() {
                    continue;
                }
                child_methods.insert(k.clone(), shared_to_value(&mut child.gc, &shared));
            }
            child.method_tables.insert(name.clone(), child_methods);
        }
        for (name, methods) in &self.static_methods {
            let mut child_methods = IndexMap::new();
            for (k, v) in methods {
                let shared = value_to_shared(&self.gc, v);
                if matches!(shared, SharedValue::Null) && !v.is_null() {
                    continue;
                }
                child_methods.insert(k.clone(), shared_to_value(&mut child.gc, &shared));
            }
            child.static_methods.insert(name.clone(), child_methods);
        }

        child.embedded_fields = self.embedded_fields.clone();

        for (name, defaults) in &self.struct_defaults {
            let mut child_defaults = IndexMap::new();
            for (k, v) in defaults {
                let shared = value_to_shared(&self.gc, v);
                if matches!(shared, SharedValue::Null) && !v.is_null() {
                    continue;
                }
                child_defaults.insert(k.clone(), shared_to_value(&mut child.gc, &shared));
            }
            child.struct_defaults.insert(name.clone(), child_defaults);
        }

        // Propagate cancellation flag so squad can cancel spawned tasks
        child.cancelled = self.cancelled.clone();

        #[cfg(feature = "jit")]
        assert!(
            child.jit.is_empty(),
            "BUG: SendableVM must have empty JIT state to be safely Send"
        );
        #[cfg(feature = "jit")]
        {
            child.jit.mode = self.jit.mode;
        }
        SendableVM(child)
    }

    /// Silently drain both stream-boundary flags. Used by spawn-family
    /// opcodes after `fork_for_spawn` + `transfer_closure` have run, so a
    /// captured-stream upvalue cannot leak the flag into a subsequent
    /// unrelated builtin call on the parent thread. Spawn already silently
    /// coerces non-transferable values (functions, closures, channels) so
    /// silently dropping captured streams is consistent; what we must not
    /// allow is the flag surviving past the spawn opcode.
    #[inline]
    pub(super) fn drain_stream_boundary_flags(&self) {
        self.stream_boundary_error.set(false);
        let _ = super::value::take_stream_boundary_error();
    }

    /// Re-create a closure from parent GC in a child VM's GC.
    /// The Arc<Chunk> is shared; upvalue values are copied via SharedValue.
    fn transfer_closure(&self, closure_ref: GcRef, child: &mut VM) -> Value {
        let obj = self
            .gc
            .get(closure_ref)
            .expect("BUG: closure ref invalid in transfer_closure");
        match &obj.kind {
            ObjKind::Closure(c) => {
                let function = ObjFunction {
                    name: c.function.name.clone(),
                    chunk: std::sync::Arc::clone(&c.function.chunk),
                };
                let mut child_upvalues = Vec::new();
                for uv_ref in &c.upvalues {
                    let uv_val = self
                        .gc
                        .get(*uv_ref)
                        .and_then(|o| match &o.kind {
                            ObjKind::Upvalue(uv) => Some(&uv.value),
                            _ => None,
                        })
                        .cloned()
                        .unwrap_or(Value::null());
                    let shared = value_to_shared(&self.gc, &uv_val);
                    let child_val = shared_to_value(&mut child.gc, &shared);
                    let child_uv = child
                        .gc
                        .alloc(ObjKind::Upvalue(ObjUpvalue { value: child_val }));
                    child_upvalues.push(child_uv);
                }
                let closure = ObjClosure {
                    function,
                    upvalues: child_upvalues,
                };
                let r = child.gc.alloc(ObjKind::Closure(closure));
                Value::obj(r)
            }
            ObjKind::Function(f) => {
                let function = ObjFunction {
                    name: f.name.clone(),
                    chunk: std::sync::Arc::clone(&f.chunk),
                };
                let r = child.gc.alloc(ObjKind::Function(function));
                Value::obj(r)
            }
            _ => Value::null(),
        }
    }

    pub fn execute(&mut self, chunk: &Chunk) -> Result<Value, VMError> {
        let func = ObjFunction {
            name: "<main>".to_string(),
            chunk: std::sync::Arc::new(chunk.clone()),
        };
        let closure = ObjClosure {
            function: func,
            upvalues: Vec::new(),
        };
        let closure_ref = self.gc.alloc(ObjKind::Closure(closure));

        let frame_size = (chunk.max_registers as usize).max(1);
        self.ensure_registers(frame_size);
        self.frames.push(CallFrame::new(closure_ref, 0, frame_size));
        self.run_until(0)
    }

    pub(super) fn execute_module(&mut self, chunk: &Chunk) -> Result<Value, VMError> {
        let func = ObjFunction {
            name: "<module>".to_string(),
            chunk: std::sync::Arc::new(chunk.clone()),
        };
        let closure = ObjClosure {
            function: func,
            upvalues: Vec::new(),
        };
        let closure_ref = self.gc.alloc(ObjKind::Closure(closure));
        let new_base = self.frames.last().map(|f| f.base + f.size).unwrap_or(0);
        let frame_size = (chunk.max_registers as usize).max(1);
        // Shared depth limit + native stack guard (runtime/recursion.rs).
        crate::runtime::recursion::check_call_depth(self.frames.len())
            .map_err(|m| VMError::new(&m))?;
        self.ensure_registers(new_base + frame_size);
        self.frames
            .push(CallFrame::new(closure_ref, new_base, frame_size));
        let boundary = self.frames.len() - 1;
        // Module bytecode is ordinary bytecode: don't auto-pin its
        // allocations even when an `import` builtin is the caller.
        let was_pinning = self.gc.set_pinning(false);
        let result = self.run_until(boundary);
        self.gc.set_pinning(was_pinning);
        result
    }

    fn ensure_registers(&mut self, needed: usize) {
        if needed > self.registers.len() {
            self.registers.resize(needed, Value::null());
        }
    }

    fn earliest_expired_timeout(&self) -> Option<(usize, TimeoutGuard)> {
        let now = Instant::now();
        self.frames
            .iter()
            .enumerate()
            .flat_map(|(frame_idx, frame)| {
                frame
                    .timeouts
                    .iter()
                    .copied()
                    .map(move |guard| (frame_idx, guard))
            })
            .filter(|(_, guard)| now >= guard.deadline)
            .min_by_key(|(_, guard)| guard.deadline)
    }

    pub(super) fn sleep_with_timeout_checks(&self, duration: Duration) -> Result<(), VMError> {
        let total_ms = duration.as_millis() as u64;
        let mut elapsed = 0u64;
        while elapsed < total_ms {
            if let Some((_, guard)) = self.earliest_expired_timeout() {
                return Err(VMError::new(&format!(
                    "timeout: operation exceeded {} second limit",
                    guard.seconds
                )));
            }
            let chunk = std::cmp::min(50, total_ms - elapsed);
            std::thread::sleep(Duration::from_millis(chunk));
            elapsed += chunk;
        }
        if let Some((_, guard)) = self.earliest_expired_timeout() {
            return Err(VMError::new(&format!(
                "timeout: operation exceeded {} second limit",
                guard.seconds
            )));
        }
        Ok(())
    }

    fn handle_timeout_expiry(&mut self) -> Result<usize, VMError> {
        let (frame_idx, guard) = self
            .earliest_expired_timeout()
            .ok_or_else(|| VMError::new("internal: no expired timeout"))?;

        while self.frames.len() > frame_idx + 1 {
            self.profiler.exit_function();
            self.frames.pop();
        }

        let err = VMError::new(&format!(
            "timeout: operation exceeded {} second limit",
            guard.seconds
        ));
        let err_value = self.runtime_error_value(&err);
        let base = self.frames[frame_idx].base;
        self.registers[base + guard.error_register as usize] = err_value;

        let frame = &mut self.frames[frame_idx];
        frame.handlers.truncate(guard.handler_base);
        frame.ip = guard.catch_ip;
        // The safe point that called us has already refilled the countdown,
        // so the catch path's `PopTimeout` runs before the next poll.
        Ok(frame_idx)
    }

    fn run_until(&mut self, boundary_frame_idx: usize) -> Result<Value, VMError> {
        let mut cached_closure: Option<(GcRef, Arc<Chunk>)> = None;

        loop {
            if self.frames.is_empty() {
                return Ok(Value::null());
            }

            let frame_idx = self.frames.len() - 1;
            let current_closure = self.frames[frame_idx].closure;
            let need_fetch = match cached_closure {
                Some((ref r, _)) => *r != current_closure,
                None => true,
            };
            if need_fetch {
                let closure_obj = self
                    .gc
                    .get(current_closure)
                    .ok_or_else(|| VMError::new("invalid closure"))?;
                let c = if let ObjKind::Closure(c) = &closure_obj.kind {
                    c.function.chunk.clone()
                } else {
                    return Err(VMError::new("expected closure"));
                };
                cached_closure = Some((current_closure, c));
            }
            // Borrowed, not cloned: an `Arc` clone/drop pair per instruction
            // showed up in the dispatch cost. The cache is only refreshed
            // when the top frame's closure changes.
            let chunk: &Arc<Chunk> = &cached_closure
                .as_ref()
                .expect("BUG: cached_closure is None after need_fetch guard always fills it")
                .1;

            if self.frames[frame_idx].ip >= chunk.code.len() {
                self.frames.pop();
                continue;
            }

            // Safe point: settle fuel and poll `timeout` deadlines every
            // SAFEPOINT_INTERVAL instructions (see `safepoint_countdown`).
            if self.safepoint_countdown == 0 {
                match self.meter.safepoint(self.fuel_window, SAFEPOINT_INTERVAL) {
                    Ok(next) => {
                        self.safepoint_countdown = next;
                        self.fuel_window = u64::from(next) + 1;
                    }
                    Err(message) => return Err(self.raise_fatal(&message, boundary_frame_idx)),
                }
                if self.earliest_expired_timeout().is_some() {
                    let handler_frame_idx = self.handle_timeout_expiry()?;
                    if handler_frame_idx < boundary_frame_idx {
                        return Err(VMError::unwound_to_handler());
                    }
                    continue;
                }
            } else {
                self.safepoint_countdown -= 1;
            }

            let frame = &mut self.frames[frame_idx];
            let inst = chunk.code[frame.ip];
            frame.ip += 1;
            let base = frame.base;

            let op = decode_op(inst);
            let a = decode_a(inst);
            let b = decode_b(inst);
            let c = decode_c(inst);
            let bx = decode_bx(inst);
            let sbx = decode_sbx(inst);
            let opcode: OpCode = OpCode::try_from(op)
                .map_err(|bad| VMError::new(&format!("invalid opcode: {bad}")))?;

            let step_result = (|| -> Result<Option<Value>, VMError> {
                match opcode {
                    OpCode::LoadConst => {
                        let val = self.constant_to_value(&chunk.constants[bx as usize]);
                        self.registers[base + a as usize] = val;
                    }
                    OpCode::LoadNull => {
                        self.registers[base + a as usize] = Value::null();
                    }
                    OpCode::LoadTrue => {
                        self.registers[base + a as usize] = Value::bool_val(true);
                    }
                    OpCode::LoadFalse => {
                        self.registers[base + a as usize] = Value::bool_val(false);
                    }
                    OpCode::Move => {
                        let v = self.registers[base + b as usize];
                        // The source may be a local register: the copy is a
                        // second reference (see `GcObject::unique`).
                        self.gc.share(v);
                        self.registers[base + a as usize] = v;
                    }
                    OpCode::Add => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.arith_op(&left, &right, OpCode::Add)?;
                    }
                    OpCode::Sub => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.arith_op(&left, &right, OpCode::Sub)?;
                    }
                    OpCode::Mul => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.arith_op(&left, &right, OpCode::Mul)?;
                    }
                    OpCode::Div => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.arith_op(&left, &right, OpCode::Div)?;
                    }
                    OpCode::Mod => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.arith_op(&left, &right, OpCode::Mod)?;
                    }
                    OpCode::Neg => {
                        let src = self.registers[base + b as usize];
                        self.registers[base + a as usize] = match src.classify(&self.gc) {
                            ValueKind::Int(n) => match n.checked_neg() {
                                Some(neg) => Value::int(neg, &mut self.gc),
                                None => Value::float(-(n as f64)),
                            },
                            ValueKind::Float(n) => Value::float(-n),
                            _ => return Err(VMError::new("cannot negate non-number")),
                        };
                    }
                    OpCode::Eq => {
                        let left = &self.registers[base + b as usize];
                        let right = &self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            Value::bool_val(left.equals(right, &self.gc));
                    }
                    OpCode::NotEq => {
                        let left = &self.registers[base + b as usize];
                        let right = &self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            Value::bool_val(!left.equals(right, &self.gc));
                    }
                    OpCode::Lt => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.compare_op(&left, &right, OpCode::Lt)?;
                    }
                    OpCode::Gt => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.compare_op(&left, &right, OpCode::Gt)?;
                    }
                    OpCode::LtEq => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.compare_op(&left, &right, OpCode::LtEq)?;
                    }
                    OpCode::GtEq => {
                        let left = self.registers[base + b as usize];
                        let right = self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.compare_op(&left, &right, OpCode::GtEq)?;
                    }
                    OpCode::And => {
                        let left = self.registers[base + b as usize].is_truthy(&self.gc);
                        let right = self.registers[base + c as usize].is_truthy(&self.gc);
                        self.registers[base + a as usize] = Value::bool_val(left && right);
                    }
                    OpCode::Or => {
                        let left = self.registers[base + b as usize].is_truthy(&self.gc);
                        let right = self.registers[base + c as usize].is_truthy(&self.gc);
                        self.registers[base + a as usize] = Value::bool_val(left || right);
                    }
                    OpCode::Not => {
                        let val = self.registers[base + b as usize].is_truthy(&self.gc);
                        self.registers[base + a as usize] = Value::bool_val(!val);
                    }
                    OpCode::GetGlobal => {
                        let name_const = &chunk.constants[bx as usize];
                        if let Constant::Str(name) = name_const {
                            let val = self.globals.get(name).cloned().ok_or_else(|| {
                                VMError::new(&crate::semantics::undefined_variable(name, None))
                            })?;
                            self.registers[base + a as usize] = val;
                        }
                    }
                    OpCode::SetGlobal => {
                        let name_const = &chunk.constants[bx as usize];
                        if let Constant::Str(name) = name_const {
                            let val = self.registers[base + a as usize];
                            self.globals.insert(name.clone(), val);
                        }
                    }
                    OpCode::GetLocal => {
                        let value = self.read_local(frame_idx, base, b)?;
                        // Copying a reference out of a local: it is no
                        // longer uniquely owned (see `GcObject::unique`).
                        self.gc.share(value);
                        self.registers[base + a as usize] = value;
                    }
                    OpCode::SetLocal => {
                        let val = self.registers[base + b as usize];
                        self.write_local(frame_idx, base, a, val);
                    }
                    OpCode::AddLocal => {
                        let rhs = self.registers[base + b as usize];
                        self.add_local(frame_idx, base, a, rhs)?;
                    }
                    OpCode::PushLocal => {
                        let value = self.registers[base + b as usize];
                        self.push_local(frame_idx, base, a, value)?;
                    }
                    OpCode::PopLocal => {
                        let popped = self.pop_local(frame_idx, base, a)?;
                        self.registers[base + b as usize] = popped;
                    }
                    OpCode::Jump => {
                        let frame = &mut self.frames[frame_idx];
                        frame.ip = (frame.ip as i64 + sbx as i64) as usize;
                    }
                    OpCode::JumpIfFalse => {
                        let val = &self.registers[base + a as usize];
                        if !val.is_truthy(&self.gc) {
                            let frame = &mut self.frames[frame_idx];
                            frame.ip = (frame.ip as i64 + sbx as i64) as usize;
                        }
                    }
                    OpCode::JumpIfTrue => {
                        let val = &self.registers[base + a as usize];
                        if val.is_truthy(&self.gc) {
                            let frame = &mut self.frames[frame_idx];
                            frame.ip = (frame.ip as i64 + sbx as i64) as usize;
                        }
                    }
                    OpCode::Loop => {
                        // Cooperative cancellation check at backward jump
                        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
                            return Err(VMError::new("task cancelled"));
                        }
                        let frame = &mut self.frames[frame_idx];
                        frame.ip = (frame.ip as i64 + sbx as i64) as usize;
                        frame.back_edges = frame.back_edges.saturating_add(1);
                        #[cfg(feature = "jit")]
                        if frame.back_edges == LOOP_HOT_THRESHOLD {
                            if let Some(value) = self.try_jit_loop_restart(frame_idx, chunk)? {
                                // Exactly what `Return` does with the value.
                                self.profiler.exit_function();
                                self.frames.pop();
                                return Ok(Some(value));
                            }
                        }
                    }
                    OpCode::Call => {
                        // Cooperative cancellation check at function call
                        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
                            return Err(VMError::new("task cancelled"));
                        }
                        let func_val = self.registers[base + a as usize];
                        let arg_count = b as usize;
                        let dst_reg = base + c as usize;

                        let mut args = Vec::with_capacity(arg_count);
                        for i in 0..arg_count {
                            args.push(self.registers[base + a as usize + 1 + i]);
                        }

                        // Direct calls of user functions follow the shared
                        // arity rule; callbacks invoked by builtins (which
                        // also use `call_value`) stay lenient.
                        self.check_direct_call_arity(func_val, args.len())?;
                        let result = self.call_value(func_val, args)?;
                        self.registers[dst_reg] = result;
                    }
                    OpCode::Return => {
                        let val = self.registers[base + a as usize];
                        self.profiler.exit_function();
                        self.frames.pop();
                        return Ok(Some(val));
                    }
                    OpCode::ReturnNull => {
                        self.profiler.exit_function();
                        self.frames.pop();
                        return Ok(Some(Value::null()));
                    }
                    OpCode::Closure => {
                        let proto = chunk.prototypes[bx as usize].clone();
                        let parent_upvalues = {
                            let frame = &self.frames[frame_idx];
                            let closure_obj = self
                                .gc
                                .get(frame.closure)
                                .ok_or_else(|| VMError::new("invalid closure"))?;
                            if let ObjKind::Closure(closure) = &closure_obj.kind {
                                closure.upvalues.clone()
                            } else {
                                return Err(VMError::new("expected closure"));
                            }
                        };

                        let mut upvalue_refs = Vec::new();
                        for source in &proto.upvalue_sources {
                            let uv_ref = match source {
                                UpvalueSource::Local(src_reg) => {
                                    if let Some(existing) =
                                        self.frames[frame_idx].open_upvalues.get(src_reg).copied()
                                    {
                                        existing
                                    } else {
                                        let val = self.registers[base + *src_reg as usize];
                                        // The upvalue cell is a second
                                        // reference to the local's value.
                                        self.gc.share(val);
                                        let uv_ref = self
                                            .gc
                                            .alloc(ObjKind::Upvalue(ObjUpvalue { value: val }));
                                        self.frames[frame_idx]
                                            .open_upvalues
                                            .insert(*src_reg, uv_ref);
                                        uv_ref
                                    }
                                }
                                UpvalueSource::Upvalue(parent_idx) => parent_upvalues
                                    .get(*parent_idx as usize)
                                    .copied()
                                    .ok_or_else(|| VMError::new("invalid upvalue source"))?,
                            };
                            upvalue_refs.push(uv_ref);
                        }

                        let func = ObjFunction {
                            name: proto.name.clone(),
                            chunk: std::sync::Arc::new(proto),
                        };
                        let closure = ObjClosure {
                            function: func,
                            upvalues: upvalue_refs,
                        };
                        let r = self.gc.alloc(ObjKind::Closure(closure));
                        self.registers[base + a as usize] = Value::obj(r);
                    }
                    OpCode::GetUpvalue => {
                        let uv_idx = b as usize;
                        if let Some(frame) = self.frames.last() {
                            if let Some(obj) = self.gc.get(frame.closure) {
                                if let ObjKind::Closure(closure) = &obj.kind {
                                    if uv_idx < closure.upvalues.len() {
                                        let uv_ref = closure.upvalues[uv_idx];
                                        if let Some(uv_obj) = self.gc.get(uv_ref) {
                                            if let ObjKind::Upvalue(uv) = &uv_obj.kind {
                                                self.registers[base + a as usize] = uv.value;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    OpCode::SetUpvalue => {
                        let uv_idx = a as usize;
                        let val = self.registers[base + b as usize];
                        if let Some(frame) = self.frames.last() {
                            let closure_ref = frame.closure;
                            if let Some(obj) = self.gc.get(closure_ref) {
                                if let ObjKind::Closure(closure) = &obj.kind {
                                    if uv_idx < closure.upvalues.len() {
                                        let uv_ref = closure.upvalues[uv_idx];
                                        if let Some(uv_obj) = self.gc.get_mut(uv_ref) {
                                            if let ObjKind::Upvalue(uv) = &mut uv_obj.kind {
                                                uv.value = val;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    OpCode::NewArray => {
                        let start = base + b as usize;
                        let count = c as usize;
                        let mut items = Vec::with_capacity(count);
                        for i in 0..count {
                            items.push(self.registers[start + i]);
                        }
                        let r = self.gc.alloc(ObjKind::Array(items));
                        self.registers[base + a as usize] = Value::obj(r);
                    }
                    OpCode::NewTuple => {
                        let start = base + b as usize;
                        let count = c as usize;
                        let mut items = Vec::with_capacity(count);
                        for i in 0..count {
                            items.push(self.registers[start + i]);
                        }
                        let r = self.gc.alloc(ObjKind::Tuple(items));
                        self.registers[base + a as usize] = Value::obj(r);
                    }
                    OpCode::NewObject => {
                        let start = base + b as usize;
                        let pair_count = c as usize;
                        let mut map = IndexMap::new();
                        for i in 0..pair_count {
                            let key_val = &self.registers[start + i * 2];
                            let val = self.registers[start + i * 2 + 1];
                            if let Some(key) = self.get_string(key_val) {
                                map.insert(key, val);
                            }
                        }
                        let r = self.gc.alloc(ObjKind::Object(map));
                        self.registers[base + a as usize] = Value::obj(r);
                    }
                    OpCode::GetField => {
                        let obj_val = self.registers[base + b as usize];
                        let Constant::Str(field) = &chunk.constants[c as usize] else {
                            return Err(VMError::new("BUG: GetField constant is not a string"));
                        };
                        self.registers[base + a as usize] = self.get_field(obj_val, field)?;
                    }
                    OpCode::SetField => {
                        let target = self.registers[base + a as usize];
                        let val = self.registers[base + c as usize];
                        let Constant::Str(field) = &chunk.constants[b as usize] else {
                            return Err(VMError::new("BUG: SetField constant is not a string"));
                        };
                        self.registers[base + a as usize] = self.set_field(target, field, val)?;
                    }
                    OpCode::GetIndex => {
                        let obj = self.registers[base + b as usize];
                        let idx = self.registers[base + c as usize];
                        self.registers[base + a as usize] = self.index_get(obj, idx)?;
                    }
                    OpCode::IterGet => {
                        let obj = self.registers[base + b as usize];
                        let idx = self.registers[base + c as usize];
                        let is_channel = matches!(
                            obj.as_obj().and_then(|r| self.gc.get(r)).map(|o| &o.kind),
                            Some(ObjKind::Channel(_))
                        );
                        if is_channel {
                            let shared = self.iter_prefetch.pop().ok_or_else(|| {
                                VMError::new("BUG: channel IterGet without a prefetched value")
                            })?;
                            self.registers[base + a as usize] =
                                shared_to_value(&mut self.gc, &shared);
                            return Ok(None);
                        }
                        let result = if let Some(r) = obj.as_obj() {
                            if let Some(i) = idx.as_int(&self.gc) {
                                // Classify the source; clone out any pair so
                                // we can drop the gc borrow before allocating.
                                enum IterSrc {
                                    Item(Value),
                                    Pair(Value, Value),
                                    ObjPair(String, Value),
                                }
                                let src = if let Some(o) = self.gc.get(r) {
                                    match &o.kind {
                                        ObjKind::Array(items)
                                        | ObjKind::Tuple(items)
                                        | ObjKind::Set(items) => {
                                            items.get(i as usize).copied().map(IterSrc::Item)
                                        }
                                        ObjKind::Map(pairs) => pairs
                                            .get(i as usize)
                                            .map(|(k, v)| IterSrc::Pair(*k, *v)),
                                        ObjKind::Object(map) => map
                                            .iter()
                                            .nth(i as usize)
                                            .map(|(k, v)| IterSrc::ObjPair(k.clone(), *v)),
                                        _ => {
                                            return Err(VMError::new(
                                                "cannot iterate non-collection",
                                            ));
                                        }
                                    }
                                } else {
                                    None
                                };
                                match src {
                                    Some(IterSrc::Item(v)) => v,
                                    Some(IterSrc::Pair(k, v)) => {
                                        let tr = self.gc.alloc(ObjKind::Tuple(vec![k, v]));
                                        Value::obj(tr)
                                    }
                                    Some(IterSrc::ObjPair(k, v)) => {
                                        let ks = self.gc.alloc_string(k);
                                        let tr =
                                            self.gc.alloc(ObjKind::Tuple(vec![Value::obj(ks), v]));
                                        Value::obj(tr)
                                    }
                                    None => {
                                        return Err(VMError::new("index out of bounds"));
                                    }
                                }
                            } else {
                                return Err(VMError::new("iterator index must be int"));
                            }
                        } else {
                            return Err(VMError::new("invalid iterator operation"));
                        };
                        self.registers[base + a as usize] = result;
                    }
                    OpCode::SetIndex => {
                        let target = self.registers[base + a as usize];
                        let idx = self.registers[base + b as usize];
                        let val = self.registers[base + c as usize];
                        self.registers[base + a as usize] = self.index_set(target, idx, val)?;
                    }
                    OpCode::JumpIfArg => {
                        if self.frames[frame_idx].argc > a as usize {
                            self.frames[frame_idx].ip =
                                (self.frames[frame_idx].ip as isize + sbx as isize) as usize;
                        }
                    }
                    OpCode::IterHas => {
                        // `for` loop condition: is there an element at index C?
                        // Channels are iterated lazily: receive the next value
                        // (blocking) and hand it to the following `IterGet`;
                        // a closed, drained channel ends the loop.
                        let src = self.registers[base + b as usize];
                        let channel = src.as_obj().and_then(|r| match self.gc.get(r) {
                            Some(obj) => match &obj.kind {
                                ObjKind::Channel(ch) => Some(ch.clone()),
                                _ => None,
                            },
                            None => None,
                        });
                        let has = if let Some(ch) = channel {
                            let guard = ch.receiver.lock().unwrap_or_else(|e| e.into_inner());
                            match guard.as_ref().map(|rx| rx.recv()) {
                                Some(Ok(shared)) => {
                                    self.iter_prefetch.push(shared);
                                    true
                                }
                                _ => false,
                            }
                        } else {
                            let idx = self.registers[base + c as usize].as_int(&self.gc);
                            idx.is_some_and(|i| i < self.collection_len(src))
                        };
                        self.registers[base + a as usize] = Value::bool_val(has);
                    }
                    OpCode::Len => {
                        let src = self.registers[base + b as usize];
                        let len = self.collection_len(src);
                        self.registers[base + a as usize] = Value::int(len, &mut self.gc);
                    }
                    OpCode::Concat => {
                        let left = self.registers[base + b as usize].display(&self.gc);
                        let right = self.registers[base + c as usize].display(&self.gc);
                        let r = self.gc.alloc_string(format!("{}{}", left, right));
                        self.registers[base + a as usize] = Value::obj(r);
                    }
                    OpCode::Interpolate => {
                        let start = base + b as usize;
                        let count = c as usize;
                        let mut result = String::new();
                        for i in 0..count {
                            result.push_str(&self.registers[start + i].display(&self.gc));
                        }
                        let r = self.gc.alloc_string(result);
                        self.registers[base + a as usize] = Value::obj(r);
                    }
                    OpCode::ExtractField => {
                        let obj = &self.registers[base + b as usize];
                        let field_name = format!("_{}", c);
                        let extracted =
                            match obj.as_obj().and_then(|r| self.gc.get(r)).map(|o| &o.kind) {
                                Some(ObjKind::Object(map)) => {
                                    map.get(&field_name).cloned().unwrap_or(Value::null())
                                }
                                Some(ObjKind::ResultOk(v) | ObjKind::ResultErr(v)) if c == 0 => *v,
                                _ => Value::null(),
                            };
                        self.registers[base + a as usize] = extracted;
                    }
                    OpCode::Try => {
                        let src = self.registers[base + b as usize];
                        if let Some(r) = src.as_obj() {
                            if let Some(obj) = self.gc.get(r) {
                                match &obj.kind {
                                    ObjKind::ResultOk(v) => {
                                        self.registers[base + a as usize] = *v;
                                    }
                                    ObjKind::ResultErr(_) => {
                                        let val = self.registers[base + b as usize];
                                        self.frames.pop();
                                        return Ok(Some(val));
                                    }
                                    _ => {
                                        return Err(VMError::new(
                                            "? operator requires Result value",
                                        ))
                                    }
                                }
                            }
                        } else {
                            return Err(VMError::new("? operator requires Result value"));
                        }
                    }
                    OpCode::Spawn => {
                        let task = crate::runtime::limits::acquire(
                            crate::runtime::limits::Resource::Tasks,
                        )
                        .map_err(|m| VMError::new(&m))?;
                        let closure_val = self.registers[base + a as usize];
                        let result_slot: Arc<(Mutex<Option<SharedValue>>, Condvar)> =
                            Arc::new((Mutex::new(None), Condvar::new()));
                        let slot_clone = result_slot.clone();

                        let mut sendable = self.fork_for_spawn();
                        let child_closure = if let Some(r) = closure_val.as_obj() {
                            self.transfer_closure(r, &mut sendable.0)
                        } else {
                            Value::null()
                        };

                        spawn_thread(sendable, child_closure, slot_clone, task);
                        self.drain_stream_boundary_flags();

                        // Register handle with squad if active
                        if let Some(squad) = self.squad_stack.last_mut() {
                            squad.2.push(result_slot.clone());
                        }

                        let handle = self.gc.alloc(ObjKind::TaskHandle(result_slot));
                        self.registers[base + a as usize] = Value::obj(handle);
                    }
                    OpCode::SquadBegin => {
                        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let saved = self.cancelled.clone();
                        self.cancelled = cancel_flag.clone();
                        self.squad_stack.push((a, cancel_flag, Vec::new(), saved));
                    }
                    OpCode::CloseUpvalues => {
                        let frame = &mut self.frames[frame_idx];
                        if !frame.open_upvalues.is_empty() {
                            for reg in a..=b {
                                frame.open_upvalues.remove(&reg);
                            }
                        }
                    }
                    OpCode::SquadEnd => {
                        let (dst_reg, cancel_flag, handles, saved_cancelled) =
                            self.squad_stack.pop().unwrap_or_else(|| {
                                let dummy = Arc::new(std::sync::atomic::AtomicBool::new(false));
                                (a, dummy.clone(), Vec::new(), dummy)
                            });
                        // Restore outer cancellation flag
                        self.cancelled = saved_cancelled;

                        let mut results = Vec::with_capacity(handles.len());
                        let mut first_error: Option<String> = None;

                        for slot in &handles {
                            let (lock, cvar) = &**slot;
                            let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                            while guard.is_none() {
                                guard = cvar.wait(guard).unwrap_or_else(|e| e.into_inner());
                            }
                            if let Some(ref shared) = *guard {
                                match shared {
                                    SharedValue::ResultOk(inner) => {
                                        results.push(shared_to_value(&mut self.gc, inner));
                                    }
                                    SharedValue::ResultErr(inner) => {
                                        if first_error.is_none() {
                                            let msg = match inner.as_ref() {
                                                SharedValue::String(s) => s.clone(),
                                                _ => "task error".to_string(),
                                            };
                                            first_error = Some(msg);
                                            cancel_flag
                                                .store(true, std::sync::atomic::Ordering::Release);
                                        }
                                    }
                                    other => {
                                        results.push(shared_to_value(&mut self.gc, other));
                                    }
                                }
                            }
                        }

                        if let Some(msg) = first_error {
                            return Err(VMError::new(&format!("squad task error: {}", msg)));
                        }

                        // Build result array
                        let arr_items: Vec<Value> = results;
                        let arr = self.gc.alloc(ObjKind::Array(arr_items));
                        self.registers[base + dst_reg as usize] = Value::obj(arr);
                    }
                    OpCode::Await => {
                        let src = self.registers[base + b as usize];
                        // Extract the Arc first, releasing the GC borrow
                        let maybe_slot = if let Some(r) = src.as_obj() {
                            self.gc.get(r).and_then(|obj| {
                                if let ObjKind::TaskHandle(slot) = &obj.kind {
                                    Some(slot.clone())
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        };
                        // GC borrow released — safe to call shared_to_value
                        let result = if let Some(slot) = maybe_slot {
                            let (lock, cvar) = &*slot;
                            let mut guard = lock
                                .lock()
                                .map_err(|_| VMError::new("await: spawned task panicked"))?;
                            while guard.is_none() {
                                guard = cvar
                                    .wait(guard)
                                    .map_err(|_| VMError::new("await: wait interrupted"))?;
                            }
                            let shared = guard.as_ref().cloned().unwrap_or(SharedValue::Null);
                            let val = shared_to_value(&mut self.gc, &shared);
                            // Unwrap ResultOk, propagate ResultErr
                            match val.classify(&self.gc) {
                                ValueKind::Obj(r) => {
                                    if let Some(obj) = self.gc.get(r) {
                                        if let ObjKind::ResultOk(v) = &obj.kind {
                                            *v
                                        } else if let ObjKind::ResultErr(e) = &obj.kind {
                                            let msg = e.display(&self.gc);
                                            return Err(VMError::new(&format!(
                                                "task error: {}",
                                                msg
                                            )));
                                        } else {
                                            val
                                        }
                                    } else {
                                        val
                                    }
                                }
                                _ => val,
                            }
                        } else {
                            src
                        };
                        self.registers[base + a as usize] = result;
                    }
                    OpCode::PushHandler => {
                        let catch_ip = {
                            let frame = &self.frames[frame_idx];
                            (frame.ip as i64 + sbx as i64) as usize
                        };
                        let frame = &mut self.frames[frame_idx];
                        frame.handlers.push(ExceptionHandler {
                            catch_ip,
                            error_register: a,
                        });
                    }
                    OpCode::PopHandler => {
                        self.frames[frame_idx].handlers.pop();
                    }
                    OpCode::PushTimeout => {
                        let timeout_val = self.registers[base + a as usize];
                        let seconds = if let Some(n) = timeout_val.as_int(&self.gc) {
                            n.max(0) as u64
                        } else if let Some(n) = timeout_val.as_float() {
                            n.max(0.0) as u64
                        } else {
                            5
                        };
                        let catch_ip = {
                            let frame = &self.frames[frame_idx];
                            (frame.ip as i64 + sbx as i64) as usize
                        };
                        let handler_base = self.frames[frame_idx].handlers.len().saturating_sub(1);
                        self.frames[frame_idx].timeouts.push(TimeoutGuard {
                            deadline: Instant::now() + Duration::from_secs(seconds),
                            seconds,
                            catch_ip,
                            error_register: a,
                            handler_base,
                        });
                        // Poll at the next instruction, so an already
                        // expired scope fires before its body runs. The
                        // window shrinks to what actually ran, keeping the
                        // fuel count exact.
                        self.fuel_window -= u64::from(self.safepoint_countdown);
                        self.safepoint_countdown = 0;
                    }
                    OpCode::PopTimeout => {
                        self.frames[frame_idx].timeouts.pop();
                    }
                    OpCode::Schedule => {
                        let closure_val = self.registers[base + a as usize];
                        let interval_val = self.registers[base + b as usize];
                        let secs = if let Some(n) = interval_val.as_int(&self.gc) {
                            if n > 0 {
                                // Read unit string from register C
                                let unit_val = self.registers[base + c as usize];
                                let unit_str = if let Some(r) = unit_val.as_obj() {
                                    self.gc
                                        .get(r)
                                        .and_then(|o| match &o.kind {
                                            ObjKind::String(s) => Some(s.clone()),
                                            _ => None,
                                        })
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                };
                                match unit_str.as_str() {
                                    "minutes" => n as u64 * 60,
                                    "hours" => n as u64 * 3600,
                                    _ => n as u64, // "seconds" or default
                                }
                            } else {
                                return Err(VMError::new(
                                    "schedule interval must be a positive integer",
                                ));
                            }
                        } else {
                            60 // Non-integer defaults to 60s (matches interpreter)
                        };

                        let mut sendable = self.fork_for_spawn();
                        let child_closure = if let Some(r) = closure_val.as_obj() {
                            self.transfer_closure(r, &mut sendable.0)
                        } else {
                            Value::null()
                        };

                        spawn_schedule_thread(sendable, child_closure, Duration::from_secs(secs));
                        self.drain_stream_boundary_flags();
                    }
                    OpCode::Watch => {
                        let closure_val = self.registers[base + a as usize];
                        let path_val = self.registers[base + b as usize];
                        let path = if let Some(r) = path_val.as_obj() {
                            self.gc.get(r).and_then(|o| match &o.kind {
                                ObjKind::String(s) => Some(s.clone()),
                                _ => None,
                            })
                        } else {
                            None
                        };
                        let path =
                            path.ok_or_else(|| VMError::new("watch requires a string path"))?;

                        let mut sendable = self.fork_for_spawn();
                        let child_closure = if let Some(r) = closure_val.as_obj() {
                            self.transfer_closure(r, &mut sendable.0)
                        } else {
                            Value::null()
                        };

                        spawn_watch_thread(sendable, child_closure, path);
                        self.drain_stream_boundary_flags();
                    }
                    OpCode::Must => {
                        let src = self.registers[base + b as usize];
                        let result = if src.is_null() {
                            return Err(VMError::new("must failed: got null"));
                        } else if let Some(r) = src.as_obj() {
                            match self.gc.get(r).map(|o| &o.kind) {
                                Some(ObjKind::ResultErr(v)) => {
                                    let msg = v.display(&self.gc);
                                    return Err(VMError::new(&format!("must failed: {}", msg)));
                                }
                                Some(ObjKind::ResultOk(v)) => *v,
                                _ => src,
                            }
                        } else {
                            src
                        };
                        self.registers[base + a as usize] = result;
                    }
                    OpCode::Ask => {
                        crate::permissions::require(crate::permissions::Capability::Ai, "ask")
                            .map_err(|e| VMError::new(&e.to_string()))?;
                        let prompt_val = &self.registers[base + b as usize];
                        let prompt_str = prompt_val.display(&self.gc);

                        let api_key = std::env::var("FORGE_AI_KEY")
                            .or_else(|_| std::env::var("OPENAI_API_KEY"))
                            .unwrap_or_default();
                        if api_key.is_empty() {
                            return Err(VMError::new(
                                "ask requires FORGE_AI_KEY or OPENAI_API_KEY environment variable",
                            ));
                        }

                        let model = std::env::var("FORGE_AI_MODEL")
                            .unwrap_or_else(|_| "gpt-4o-mini".to_string());
                        let url = std::env::var("FORGE_AI_URL").unwrap_or_else(|_| {
                            "https://api.openai.com/v1/chat/completions".to_string()
                        });
                        let body = serde_json::json!({
                            "model": model,
                            "messages": [{"role": "user", "content": prompt_str}],
                            "max_tokens": 1000
                        })
                        .to_string();
                        let mut headers = std::collections::HashMap::new();
                        headers.insert("Authorization".to_string(), format!("Bearer {}", api_key));
                        headers.insert("Content-Type".to_string(), "application/json".to_string());

                        match crate::runtime::client::fetch_blocking(
                            &url,
                            "POST",
                            Some(body),
                            Some(&headers),
                            None,
                            None,
                            None,
                        ) {
                            Ok(crate::interpreter::Value::Object(resp)) => {
                                let content = resp
                                    .get("json")
                                    .and_then(|j| {
                                        if let crate::interpreter::Value::Object(json) = j {
                                            json.get("choices")
                                        } else {
                                            None
                                        }
                                    })
                                    .and_then(|c| {
                                        if let crate::interpreter::Value::Array(choices) = c {
                                            choices.first()
                                        } else {
                                            None
                                        }
                                    })
                                    .and_then(|c| {
                                        if let crate::interpreter::Value::Object(choice) = c {
                                            choice.get("message")
                                        } else {
                                            None
                                        }
                                    })
                                    .and_then(|m| {
                                        if let crate::interpreter::Value::Object(msg) = m {
                                            msg.get("content")
                                        } else {
                                            None
                                        }
                                    })
                                    .and_then(|c| {
                                        if let crate::interpreter::Value::String(s) = c {
                                            Some(s.clone())
                                        } else {
                                            None
                                        }
                                    });

                                if let Some(text) = content {
                                    self.registers[base + a as usize] = self.alloc_string(&text);
                                } else {
                                    self.registers[base + a as usize] = Value::null();
                                }
                            }
                            Ok(_) => {
                                self.registers[base + a as usize] = Value::null();
                            }
                            Err(e) => {
                                return Err(VMError::new(&format!("ask error: {}", e)));
                            }
                        }
                    }
                    OpCode::Freeze => {
                        let src = self.registers[base + b as usize];
                        let frozen_ref = self.gc.alloc(ObjKind::Frozen(src));
                        self.registers[base + a as usize] = Value::obj(frozen_ref);
                    }
                    _ => {
                        return Err(VMError::new(&format!("unknown opcode: {}", op)));
                    }
                }
                Ok(None)
            })();

            match step_result {
                Ok(Some(value)) => return Ok(value),
                Ok(None) => {}
                Err(err) if err.is_unwound_to_handler() => {
                    if self.frames.len() <= boundary_frame_idx {
                        return Err(err);
                    }
                    continue;
                }
                Err(err) => match self.handle_runtime_error(err, boundary_frame_idx) {
                    Ok(_handler_frame_idx) => continue,
                    Err(err) => return Err(err),
                },
            }

            // GC check
            if self.gc.should_collect() {
                let max_reg = self.frames.last().map(|f| f.base + f.size).unwrap_or(0);
                let scan_limit = max_reg.min(self.registers.len());
                let mut roots = Vec::with_capacity(scan_limit / 4);
                for r in &self.registers[..scan_limit] {
                    if let Some(gr) = r.as_obj() {
                        roots.push(gr);
                    }
                }
                for v in self.globals.values() {
                    if let Some(gr) = v.as_obj() {
                        roots.push(gr);
                    }
                }
                for frame in &self.frames {
                    roots.push(frame.closure);
                    roots.extend(frame.entry_args.iter().filter_map(|v| v.as_obj()));
                    for gr in frame.open_upvalues.values() {
                        roots.push(*gr);
                    }
                }
                for methods in self.method_tables.values() {
                    for v in methods.values() {
                        if let Some(gr) = v.as_obj() {
                            roots.push(gr);
                        }
                    }
                }
                for methods in self.static_methods.values() {
                    for v in methods.values() {
                        if let Some(gr) = v.as_obj() {
                            roots.push(gr);
                        }
                    }
                }
                for defaults in self.struct_defaults.values() {
                    for v in defaults.values() {
                        if let Some(gr) = v.as_obj() {
                            roots.push(gr);
                        }
                    }
                }
                self.gc.collect(&roots);
                if self.gc.memory_exceeded() {
                    let limit = self.gc.memory_limit().unwrap_or(0);
                    let message = match self.meter.budget() {
                        Some(b) => b.trip(crate::runtime::limits::Trip::Memory),
                        None => crate::runtime::limits::memory_exceeded_message(limit),
                    };
                    return Err(self.raise_fatal(&message, boundary_frame_idx));
                }
            }
        }
    }

    /// Select the JIT mode (`Off`, `Auto` = tier up hot functions, `Eager`
    /// = compile on first call as with `forge --jit`).
    #[cfg(feature = "jit")]
    pub fn set_jit_mode(&mut self, mode: super::jit::tier::JitMode) {
        self.jit.mode = mode;
    }

    /// Run `chunk` natively if a specialization exists (or can be compiled)
    /// for these arguments and every entry guard passes. `None` means the
    /// caller must execute the call in the VM — including after a deopt,
    /// which is safe because every compiled function is pure.
    #[cfg(feature = "jit")]
    fn try_jit_call(
        &mut self,
        chunk: &Arc<Chunk>,
        args: &[Value],
    ) -> Result<Option<Value>, VMError> {
        self.try_jit_native(chunk, args, self.frames.len(), false)
    }

    /// Loop tier-up ("restart in native code"). Called when the frame at
    /// `frame_idx` takes its [`LOOP_HOT_THRESHOLD`]th backward jump: if its
    /// function has (or can now get) a specialization for the arguments the
    /// frame was entered with, the *whole call* is re-run natively from the
    /// start and its result is the frame's result.
    ///
    /// This is sound for the same reason deoptimization is: the verifier
    /// only accepts pure functions (they read only their arguments, write
    /// only their own registers and call only themselves), so the work the
    /// VM has done in this frame so far has no observable effect and
    /// repeating it natively is indistinguishable from finishing it in the
    /// VM. The repeated work is bounded by the threshold. On a guard
    /// failure, rejection or deopt the frame simply continues in the VM.
    ///
    /// True on-stack replacement (entering native code at the loop header
    /// with the frame's live registers) is a possible follow-up; restarting
    /// needs no new entry points or state mapping in the JIT.
    #[cfg(feature = "jit")]
    fn try_jit_loop_restart(
        &mut self,
        frame_idx: usize,
        chunk: &Arc<Chunk>,
    ) -> Result<Option<Value>, VMError> {
        if self.jit.mode == super::jit::tier::JitMode::Off
            || chunk.name == "<main>"
            || chunk.name == "<module>"
        {
            return Ok(None);
        }
        let frame = &self.frames[frame_idx];
        if frame.entry_args.len() != chunk.arity as usize {
            return Ok(None);
        }
        let args = frame.entry_args.clone();
        // The native call replaces this frame, so it starts at its depth.
        self.try_jit_native(chunk, &args, frame_idx, true)
    }

    /// Shared native-call path. `depth_below` is the number of VM frames
    /// beneath the call; `force_hot` skips the call-count threshold (the
    /// caller has its own hotness evidence).
    #[cfg(feature = "jit")]
    fn try_jit_native(
        &mut self,
        chunk: &Arc<Chunk>,
        args: &[Value],
        depth_below: usize,
        force_hot: bool,
    ) -> Result<Option<Value>, VMError> {
        use super::jit::tier::{invoke, Invoke};

        let Some(sel) = self.jit.select(chunk, args, &self.gc, force_hot) else {
            return Ok(None);
        };

        // VM-state guards: the VM itself would refuse the call (stack
        // overflow), a `timeout` is active (the VM checks deadlines between
        // instructions; native code does not), or the global the code calls
        // itself through no longer names this function.
        let depth_limit = crate::runtime::recursion::max_depth();
        // Native code has no fuel counter: with a fuel budget every call
        // stays in the VM, so exhaustion is exact and deterministic.
        let guards_ok = depth_below < depth_limit
            && !self.meter.fuel_limited()
            && self.frames.iter().all(|f| f.timeouts.is_empty())
            && (!sel.needs_self_binding || self.jit_self_binding_matches(chunk));
        if !guards_ok {
            self.jit.record_guard_failure(&sel);
            return Ok(None);
        }

        let mut raw: Vec<i64> = Vec::with_capacity(args.len());
        for v in args {
            // `select` already checked every argument's kind.
            let encoded = super::jit::types::JitType::of_value(v, &self.gc)
                .and_then(|kind| kind.encode(v, &self.gc));
            let Some(encoded) = encoded else {
                return Ok(None);
            };
            raw.push(encoded);
        }
        let max_depth = (depth_limit - 1 - depth_below) as i64;
        // SAFETY: `sel.entry` was produced by the JIT compiler owned by
        // `self.jit`, which outlives this call; `raw` has exactly the
        // specialization's arity (checked by `select`); `self.cancelled` is
        // alive for the duration of the call.
        let outcome = unsafe { invoke(sel.entry, &raw, max_depth, Arc::as_ptr(&self.cancelled)) };
        // An error raised by a runtime bridge is the call's outcome. It is
        // checked before the return/deopt result: a deopt would re-run the
        // call in the VM and repeat the side effects that preceded the
        // error, and a normal return would swallow it.
        if let Some(err) = self.jit_bridge_error.take() {
            self.jit.record_run(&sel);
            return Err(err);
        }
        match outcome {
            Invoke::Returned(r) => {
                self.jit.record_run(&sel);
                Ok(Some(sel.ret.decode(r, &mut self.gc)))
            }
            Invoke::Deopt => {
                self.jit.record_deopt(&sel);
                Ok(None)
            }
        }
    }

    /// Record an error raised by a JIT runtime bridge (see
    /// `jit_bridge_error`). The first error wins.
    #[cfg(feature = "jit")]
    pub(crate) fn record_jit_bridge_error(&mut self, err: VMError) {
        if self.jit_bridge_error.is_none() {
            self.jit_bridge_error = Some(err);
        }
    }

    /// Take the pending bridge error, if any.
    #[cfg(all(test, feature = "jit"))]
    pub(crate) fn take_jit_bridge_error(&mut self) -> Option<VMError> {
        self.jit_bridge_error.take()
    }

    /// True when the global named like `chunk` is a closure over the same
    /// prototype code, so native self-calls behave like the VM's
    /// `GetGlobal` + `Call`.
    #[cfg(feature = "jit")]
    fn jit_self_binding_matches(&self, chunk: &Arc<Chunk>) -> bool {
        let Some(r) = self.globals.get(&chunk.name).and_then(|v| v.as_obj()) else {
            return false;
        };
        match self.gc.get(r).map(|o| &o.kind) {
            Some(ObjKind::Closure(c)) => {
                Arc::ptr_eq(&c.function.chunk, chunk)
                    || super::jit::tier::same_code(&c.function.chunk, chunk)
            }
            _ => false,
        }
    }

    /// Call any callable value (closure, function, native, `__call__`
    /// object).
    ///
    /// GC rooting (see `vm/gc.rs`): bytecode run by the callee must not be
    /// auto-pinned, so pinning is suspended for the duration of the call; if
    /// the caller is a native builtin (pinning was on), the returned value is
    /// pinned into the caller's native scope so it survives later callbacks.
    pub fn call_value(&mut self, func: Value, args: Vec<Value>) -> Result<Value, VMError> {
        let was_pinning = self.gc.set_pinning(false);
        let result = self.call_value_inner(func, args);
        self.gc.set_pinning(was_pinning);
        if let Ok(v) = &result {
            self.gc.pin_value(*v);
        }
        result
    }

    fn call_value_inner(&mut self, func: Value, args: Vec<Value>) -> Result<Value, VMError> {
        if let Some(r) = func.as_obj() {
            let obj = self
                .gc
                .get(r)
                .ok_or_else(|| VMError::new("null function"))?;
            {
                match &obj.kind {
                    ObjKind::Closure(closure) => {
                        let chunk = closure.function.chunk.clone();
                        let func_name = closure.function.name.clone();

                        // JIT dispatch: a verified, type-specialized native
                        // version runs when its entry guards pass; otherwise
                        // (or on deopt) the call runs in the VM below.
                        #[cfg(feature = "jit")]
                        if let Some(result) = self.try_jit_call(&chunk, &args)? {
                            if self.profiler.is_enabled()
                                && !func_name.is_empty()
                                && func_name != "<lambda>"
                            {
                                self.profiler.enter_function(&func_name);
                                self.profiler.exit_function();
                            }
                            return Ok(result);
                        }

                        // Count calls for profiling. Anonymous lambdas all
                        // share the name "<lambda>" and are not profiled.
                        if !func_name.is_empty() && func_name != "<lambda>" {
                            self.profiler.enter_function(&func_name);
                        }

                        let arity = chunk.arity as usize;
                        let frame_size = (chunk.max_registers as usize).max(1);
                        let new_base = self.frames.last().map(|f| f.base + f.size).unwrap_or(0);
                        // Shared depth limit + native stack guard
                        // (runtime/recursion.rs): runaway recursion is a
                        // catchable error, never a process abort.
                        crate::runtime::recursion::check_call_depth(self.frames.len())
                            .map_err(|m| VMError::new(&m))?;
                        self.ensure_registers(new_base + frame_size);

                        for (i, arg) in args.iter().enumerate() {
                            if i < arity {
                                self.registers[new_base + i] = *arg;
                            }
                        }
                        for i in args.len()..arity {
                            self.registers[new_base + i] = Value::null();
                        }

                        let mut frame = CallFrame::new(r, new_base, frame_size);
                        frame.argc = args.len();
                        frame.entry_args = args;
                        self.frames.push(frame);
                        let boundary = self.frames.len() - 1;
                        self.run_until(boundary)
                    }
                    ObjKind::NativeFunction(nf) => {
                        let name = nf.name.clone();
                        self.call_native(&name, args)
                    }
                    ObjKind::Object(map) => {
                        // Module-as-function: if the object has a __call__ field, call it
                        if let Some(call_fn) = map.get("__call__").copied() {
                            self.call_value(call_fn, args)
                        } else {
                            Err(VMError::new("cannot call non-function"))
                        }
                    }
                    _ => Err(VMError::new("cannot call non-function")),
                }
            }
        } else {
            Err(VMError::new("cannot call non-function"))
        }
    }

    // call_native() is in src/vm/builtins.rs (extracted for readability)

    pub(super) fn get_string_arg(&self, args: &[Value], idx: usize) -> Result<String, VMError> {
        match args.get(idx) {
            Some(v) => self
                .get_string(v)
                .ok_or_else(|| VMError::new("expected string argument")),
            None => Err(VMError::new("missing argument")),
        }
    }

    pub(super) fn args_to_interp(
        &self,
        args: &[Value],
    ) -> Result<Vec<crate::interpreter::Value>, VMError> {
        let out: Vec<crate::interpreter::Value> =
            args.iter().map(|v| self.convert_to_interp_val(v)).collect();
        self.check_stream_boundary()?;
        Ok(out)
    }

    #[allow(dead_code)]
    fn collect_stack_trace(&self) -> Vec<StackFrame> {
        let mut trace = Vec::new();
        for frame in self.frames.iter().rev() {
            if let Some(obj) = self.gc.get(frame.closure) {
                if let ObjKind::Closure(c) = &obj.kind {
                    let line = if frame.ip > 0 && frame.ip - 1 < c.function.chunk.lines.len() {
                        c.function.chunk.lines[frame.ip - 1]
                    } else {
                        0
                    };
                    let col = if frame.ip > 0 && frame.ip - 1 < c.function.chunk.cols.len() {
                        c.function.chunk.cols[frame.ip - 1]
                    } else {
                        0
                    };
                    trace.push(StackFrame {
                        function: c.function.name.clone(),
                        line,
                        col,
                    });
                }
            }
        }
        trace
    }

    fn classify_error_type(message: &str) -> &'static str {
        if message.contains("type") || message.contains("Type") {
            "TypeError"
        } else if message.contains("division by zero") || message.contains("modulo by zero") {
            "ArithmeticError"
        } else if message.contains("assertion") {
            "AssertionError"
        } else if message.contains("index") || message.contains("out of bounds") {
            "IndexError"
        } else if message.contains("not found") || message.contains("undefined") {
            "ReferenceError"
        } else if message.contains("immutable") || message.contains("cannot reassign") {
            "TypeError"
        } else {
            "RuntimeError"
        }
    }

    fn runtime_error_value(&mut self, err: &VMError) -> Value {
        let mut err_obj = IndexMap::new();
        err_obj.insert("message".to_string(), self.alloc_string(&err.message));
        err_obj.insert(
            "type".to_string(),
            self.alloc_string(Self::classify_error_type(&err.message)),
        );
        let err_ref = self.gc.alloc(ObjKind::Object(err_obj));
        Value::obj(err_ref)
    }

    /// Route a runtime error to the innermost `try`/`safe` handler that
    /// belongs to this `run_until` invocation (frames at or above
    /// `boundary_frame_idx`). If there is none, the frames of this
    /// invocation are discarded and the error is returned to the caller —
    /// which may be a native builtin (`yolo`, `assert_throws`, `map`, ...)
    /// that gets to observe it before any outer handler does, exactly like
    /// the interpreter's `Result` propagation.
    fn handle_runtime_error(
        &mut self,
        err: VMError,
        boundary_frame_idx: usize,
    ) -> Result<usize, VMError> {
        if err.is_unwound_to_handler() {
            return Err(err);
        }

        // Fatal resource-limit errors skip every handler.
        let handler_frames = if err.is_fatal() {
            0..0
        } else {
            boundary_frame_idx.min(self.frames.len())..self.frames.len()
        };
        for frame_idx in handler_frames.rev() {
            let handler = {
                let frame = &mut self.frames[frame_idx];
                frame.handlers.pop()
            };

            if let Some(handler) = handler {
                while self.frames.len() > frame_idx + 1 {
                    self.profiler.exit_function();
                    self.frames.pop();
                }

                let err_value = self.runtime_error_value(&err);
                let base = self.frames[frame_idx].base;
                self.registers[base + handler.error_register as usize] = err_value;
                self.frames[frame_idx].ip = handler.catch_ip;
                return Ok(frame_idx);
            }
        }

        let err = if err.stack_trace.is_empty() {
            VMError {
                stack_trace: self.collect_stack_trace(),
                ..err
            }
        } else {
            err
        };
        while self.frames.len() > boundary_frame_idx {
            self.profiler.exit_function();
            self.frames.pop();
        }
        Err(err)
    }

    pub(super) fn convert_to_interp_val(&self, v: &Value) -> crate::interpreter::Value {
        match v.classify(&self.gc) {
            ValueKind::Int(n) => crate::interpreter::Value::Int(n),
            ValueKind::Float(n) => crate::interpreter::Value::Float(n),
            ValueKind::Bool(b) => crate::interpreter::Value::Bool(b),
            ValueKind::Null => crate::interpreter::Value::Null,
            ValueKind::Obj(r) => {
                if let Some(obj) = self.gc.get(r) {
                    match &obj.kind {
                        ObjKind::String(s) => crate::interpreter::Value::String(s.clone()),
                        ObjKind::Array(items) => {
                            let converted: Vec<crate::interpreter::Value> = items
                                .iter()
                                .map(|i| self.convert_to_interp_val(i))
                                .collect();
                            crate::interpreter::Value::Array(converted)
                        }
                        ObjKind::Object(map) => {
                            let mut im = indexmap::IndexMap::new();
                            for (k, val) in map {
                                im.insert(k.clone(), self.convert_to_interp_val(val));
                            }
                            crate::interpreter::Value::Object(im)
                        }
                        ObjKind::ResultOk(v) => crate::interpreter::Value::ResultOk(Box::new(
                            self.convert_to_interp_val(v),
                        )),
                        ObjKind::ResultErr(v) => crate::interpreter::Value::ResultErr(Box::new(
                            self.convert_to_interp_val(v),
                        )),
                        ObjKind::Tuple(items) => {
                            let converted: Vec<crate::interpreter::Value> = items
                                .iter()
                                .map(|i| self.convert_to_interp_val(i))
                                .collect();
                            crate::interpreter::Value::Tuple(converted)
                        }
                        ObjKind::Set(items) => {
                            let converted: Vec<crate::interpreter::Value> = items
                                .iter()
                                .map(|i| self.convert_to_interp_val(i))
                                .collect();
                            crate::interpreter::Value::Set(converted)
                        }
                        ObjKind::Map(pairs) => {
                            let converted: Vec<(
                                crate::interpreter::Value,
                                crate::interpreter::Value,
                            )> = pairs
                                .iter()
                                .map(|(k, v)| {
                                    (self.convert_to_interp_val(k), self.convert_to_interp_val(v))
                                })
                                .collect();
                            crate::interpreter::Value::Map(converted)
                        }
                        ObjKind::Frozen(inner) => self.convert_to_interp_val(inner),
                        ObjKind::Stream(_) => {
                            // Streams cannot cross the VM/interpreter boundary.
                            // Set the flag so callers can surface a VMError.
                            // Return Null as a placeholder; caller must check
                            // via `check_stream_boundary()?` after each call.
                            self.stream_boundary_error.set(true);
                            crate::interpreter::Value::Null
                        }
                        _ => crate::interpreter::Value::Null,
                    }
                } else {
                    crate::interpreter::Value::Null
                }
            }
        }
    }

    pub(super) fn convert_interp_value(&mut self, v: &crate::interpreter::Value) -> Value {
        match v {
            crate::interpreter::Value::Int(n) => Value::int(*n, &mut self.gc),
            crate::interpreter::Value::Float(n) => Value::float(*n),
            crate::interpreter::Value::Bool(b) => Value::bool_val(*b),
            crate::interpreter::Value::Null => Value::null(),
            crate::interpreter::Value::String(s) => self.alloc_string(s),
            crate::interpreter::Value::Array(items) => {
                let vm_items: Vec<Value> =
                    items.iter().map(|i| self.convert_interp_value(i)).collect();
                let r = self.gc.alloc(ObjKind::Array(vm_items));
                Value::obj(r)
            }
            crate::interpreter::Value::Object(map) => {
                let mut vm_map = IndexMap::new();
                for (k, val) in map {
                    vm_map.insert(k.clone(), self.convert_interp_value(val));
                }
                let r = self.gc.alloc(ObjKind::Object(vm_map));
                Value::obj(r)
            }
            crate::interpreter::Value::Tuple(items) => {
                let vm_items: Vec<Value> =
                    items.iter().map(|i| self.convert_interp_value(i)).collect();
                let r = self.gc.alloc(ObjKind::Tuple(vm_items));
                Value::obj(r)
            }
            crate::interpreter::Value::Set(items) => {
                let vm_items: Vec<Value> =
                    items.iter().map(|i| self.convert_interp_value(i)).collect();
                let r = self.gc.alloc(ObjKind::Set(vm_items));
                Value::obj(r)
            }
            crate::interpreter::Value::Map(pairs) => {
                let vm_pairs: Vec<(Value, Value)> = pairs
                    .iter()
                    .map(|(k, v)| (self.convert_interp_value(k), self.convert_interp_value(v)))
                    .collect();
                let r = self.gc.alloc(ObjKind::Map(vm_pairs));
                Value::obj(r)
            }
            // Callable builtins returned by shared code (native plugin
            // namespaces from `plugins::import`).
            crate::interpreter::Value::BuiltIn(name) => self.alloc_builtin(name),
            crate::interpreter::Value::Stream(_) => {
                // Streams cannot cross the interpreter/VM boundary.
                // See `stream_boundary_error` docs on the VM struct.
                self.stream_boundary_error.set(true);
                Value::null()
            }
            _ => Value::null(),
        }
    }

    /// Convert an interpreter value to a VM value and immediately check
    /// for a boundary error. Use at call sites where the conversion is
    /// paired with returning the result to the VM. See bug #6.
    #[inline]
    pub(super) fn from_interp_checked(
        &mut self,
        v: &crate::interpreter::Value,
    ) -> Result<Value, VMError> {
        let out = self.convert_interp_value(v);
        self.check_stream_boundary()?;
        Ok(out)
    }

    /// After a conversion call site, check whether any inner `ObjKind::Stream`
    /// / `interpreter::Value::Stream` was encountered. Reads and clears BOTH
    /// the per-VM `stream_boundary_error` Cell (set by `&self` paths inside
    /// `convert_*`) and the thread-local `STREAM_BOUNDARY_ERROR` (set by the
    /// `value_to_shared` free function). Callers at the VM↔interpreter
    /// boundary must invoke this immediately after `convert_to_interp_val` /
    /// `convert_interp_value` / `value_to_shared` to surface the bug #6 error
    /// loudly instead of silently coercing the stream to Null.
    #[inline]
    pub(super) fn check_stream_boundary(&self) -> Result<(), VMError> {
        let cell_hit = self.stream_boundary_error.replace(false);
        let tls_hit = super::value::take_stream_boundary_error();
        if cell_hit || tls_hit {
            Err(VMError::new(
                "Stream cannot cross the VM/interpreter boundary; call .collect() first to materialize",
            ))
        } else {
            Ok(())
        }
    }

    /// Pre-dispatch guard for stdlib module arms in `builtins.rs` that build
    /// `interp_args` via an inline `match v.classify(...)` instead of calling
    /// `convert_to_interp_val`. Those inline matches never set the boundary
    /// flag, so a Stream argument would silently coerce to Null. This helper
    /// walks args and errors loudly if any is an `ObjKind::Stream`, preserving
    /// the bug #6 contract without rewriting every inline conversion.
    #[inline]
    pub(super) fn reject_stream_args(&self, args: &[Value]) -> Result<(), VMError> {
        for v in args {
            if let Some(r) = v.as_obj() {
                if let Some(obj) = self.gc.get(r) {
                    if matches!(obj.kind, ObjKind::Stream(_)) {
                        return Err(VMError::new(
                            "Stream cannot cross the VM/interpreter boundary; call .collect() first to materialize",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// `object.field` read. Shared by the `GetField` opcode and the
    /// `__forge_get_field` builtin (used when a constant index does not fit
    /// in the 8-bit operand).
    pub(super) fn get_field(&mut self, obj_val: Value, field: &str) -> Result<Value, VMError> {
        match self.get_field_strict(obj_val, field) {
            // Compiler-internal fields (`__variant__`, `__type__`, ...) are
            // probed on arbitrary values by pattern matching; a value that
            // lacks them simply does not match.
            Err(_) if field.starts_with("__") => Ok(Value::null()),
            other => other,
        }
    }

    fn get_field_strict(&mut self, obj_val: Value, field: &str) -> Result<Value, VMError> {
        let Some(r) = obj_val.as_obj() else {
            return Err(VMError::new(&format!(
                "cannot access field '{}' on {}",
                field,
                obj_val.type_name(&self.gc)
            )));
        };
        let needs_alloc: Option<String>;
        let direct_result: Option<Value>;
        let Some(obj) = self.gc.get(r) else {
            return Err(VMError::new("null reference"));
        };
        match &obj.kind {
            // Results are not ADT objects in the VM; expose the same
            // pattern-matching view (`Ok(v)` / `Err(e)`) the interpreter has.
            ObjKind::ResultOk(_) | ObjKind::ResultErr(_) if field.starts_with("__") => {
                let text = match (field, &obj.kind) {
                    ("__variant__", ObjKind::ResultOk(_)) => "Ok",
                    ("__variant__", _) => "Err",
                    ("__type__", _) => "Result",
                    _ => return Ok(Value::null()),
                };
                return Ok(self.alloc_string(text));
            }
            ObjKind::Object(map) => {
                if let Some(value) = map.get(field).cloned() {
                    direct_result = Some(value);
                } else if let Some(type_name) =
                    map.get("__type__").and_then(|value| self.get_string(value))
                {
                    let mut delegated = None;
                    if let Some(embeds) = self.embedded_fields.get(&type_name).cloned() {
                        for (embed_field, _) in embeds {
                            let Some(embed_ref) = map.get(&embed_field).and_then(|v| v.as_obj())
                            else {
                                continue;
                            };
                            let Some(embed_obj) = self.gc.get(embed_ref) else {
                                continue;
                            };
                            let ObjKind::Object(embed_map) = &embed_obj.kind else {
                                continue;
                            };
                            if let Some(value) = embed_map.get(field) {
                                delegated = Some(*value);
                                break;
                            }
                        }
                    }
                    direct_result =
                        Some(delegated.ok_or_else(|| {
                            VMError::new(&format!("no field '{}' on object", field))
                        })?);
                } else {
                    return Err(VMError::new(&format!("no field '{}' on object", field)));
                }
                needs_alloc = None;
            }
            ObjKind::String(s) => match field {
                "len" => {
                    let len = s.chars().count() as i64;
                    return Ok(Value::int(len, &mut self.gc));
                }
                "upper" => {
                    needs_alloc = Some(s.to_uppercase());
                    direct_result = None;
                }
                "lower" => {
                    needs_alloc = Some(s.to_lowercase());
                    direct_result = None;
                }
                "trim" => {
                    needs_alloc = Some(s.trim().to_string());
                    direct_result = None;
                }
                _ => return Err(VMError::new(&format!("no method '{}' on String", field))),
            },
            ObjKind::Array(items) | ObjKind::Set(items) => match field {
                "len" => {
                    let len = items.len() as i64;
                    return Ok(Value::int(len, &mut self.gc));
                }
                _ => {
                    let type_name = if matches!(&obj.kind, ObjKind::Set(_)) {
                        "Set"
                    } else {
                        "Array"
                    };
                    return Err(VMError::new(&format!(
                        "no method '{}' on {}",
                        field, type_name
                    )));
                }
            },
            ObjKind::Frozen(inner) => {
                let inner = *inner;
                return self.get_field(inner, field);
            }
            _ => {
                return Err(VMError::new(&format!(
                    "cannot access field '{}' on {}",
                    field,
                    obj.type_name()
                )))
            }
        }
        Ok(match needs_alloc {
            Some(s) => self.alloc_string(&s),
            None => {
                direct_result.expect("BUG: direct_result must be Some when needs_alloc is None")
            }
        })
    }

    /// `object.field = value`. Shared by `SetField` and `__forge_set_field`.
    /// Returns an updated copy; the original object is never modified
    /// (value semantics, see `compile_store` in the compiler).
    pub(super) fn set_field(
        &mut self,
        target: Value,
        field: &str,
        val: Value,
    ) -> Result<Value, VMError> {
        let Some(obj_ref) = target.as_obj() else {
            return Err(VMError::new("cannot set field on non-object"));
        };
        let mut map = match self.gc.get(obj_ref).map(|obj| &obj.kind) {
            Some(ObjKind::Object(map)) => map.clone(),
            Some(ObjKind::Frozen(_)) => return Err(VMError::new("cannot mutate a frozen value")),
            _ => return Err(VMError::new("cannot set field on non-object")),
        };
        map.insert(field.to_string(), val);
        Ok(Value::obj(self.gc.alloc(ObjKind::Object(map))))
    }

    /// `crate::semantics::check_call_arity` for a closure called directly.
    fn check_direct_call_arity(&self, func: Value, argc: usize) -> Result<(), VMError> {
        let kind = func.as_obj().and_then(|r| self.gc.get(r)).map(|o| &o.kind);
        let Some(ObjKind::Closure(closure)) = kind else {
            return Ok(());
        };
        let chunk = &closure.function.chunk;
        crate::semantics::check_call_arity(
            &closure.function.name,
            chunk.arity as usize,
            chunk.min_arity as usize,
            argc,
        )
        .map_err(|e| VMError::new(&e))
    }

    /// Element count used by `Len` and `for` loops (0 for non-collections).
    fn collection_len(&self, src: Value) -> i64 {
        match src.as_obj().and_then(|r| self.gc.get(r)).map(|o| &o.kind) {
            Some(ObjKind::String(s)) => s.chars().count() as i64,
            Some(ObjKind::Array(a) | ObjKind::Tuple(a) | ObjKind::Set(a)) => a.len() as i64,
            Some(ObjKind::Object(o)) => o.len() as i64,
            Some(ObjKind::Map(p)) => p.len() as i64,
            _ => 0,
        }
    }

    /// `container[index]` — shares negative-index and error-message rules
    /// with the interpreter through `crate::semantics`.
    pub(super) fn index_get(&self, container: Value, index: Value) -> Result<Value, VMError> {
        use crate::semantics;
        let mut current = container;
        loop {
            let Some(obj) = current.as_obj().and_then(|r| self.gc.get(r)) else {
                return Err(VMError::new(&semantics::invalid_index(
                    current.type_name(&self.gc),
                    index.type_name(&self.gc),
                )));
            };
            match (&obj.kind, index.classify(&self.gc)) {
                (ObjKind::Frozen(inner), _) => current = *inner,
                (ObjKind::Array(items) | ObjKind::Tuple(items), ValueKind::Int(i)) => {
                    return match semantics::normalize_index(i, items.len()) {
                        Some(slot) => Ok(items[slot]),
                        None => Err(VMError::new(&semantics::index_out_of_bounds(
                            i,
                            if matches!(obj.kind, ObjKind::Tuple(_)) {
                                "tuple"
                            } else {
                                "array"
                            },
                            items.len(),
                        ))),
                    };
                }
                (ObjKind::Object(map), _) if self.get_str_ref(&index).is_some() => {
                    let key = self.get_str_ref(&index).unwrap_or_default();
                    return map
                        .get(key)
                        .copied()
                        .ok_or_else(|| VMError::new(&semantics::missing_key(key)));
                }
                _ => {
                    return Err(VMError::new(&semantics::invalid_index(
                        current.type_name(&self.gc),
                        index.type_name(&self.gc),
                    )))
                }
            }
        }
    }

    /// `container[index] = value` — same rules as `index_get`. Returns an
    /// updated copy; the original container is never modified (value
    /// semantics, see `compile_store` in the compiler).
    pub(super) fn index_set(
        &mut self,
        container: Value,
        index: Value,
        value: Value,
    ) -> Result<Value, VMError> {
        use crate::semantics;
        let key = self.get_string(&index);
        let index_int = index.as_int(&self.gc);
        let index_type = index.type_name(&self.gc);
        let container_type = container.type_name(&self.gc);
        let Some(obj) = container.as_obj().and_then(|r| self.gc.get(r)) else {
            return Err(VMError::new(&semantics::invalid_index_assign(
                container_type,
            )));
        };
        let updated = match (&obj.kind, index_int, key) {
            (ObjKind::Array(items), Some(i), _) => {
                let len = items.len();
                let slot = semantics::normalize_index(i, len).ok_or_else(|| {
                    VMError::new(&semantics::index_out_of_bounds(i, "array", len))
                })?;
                let mut items = items.clone();
                items[slot] = value;
                ObjKind::Array(items)
            }
            (ObjKind::Object(map), _, Some(key)) => {
                let mut map = map.clone();
                map.insert(key, value);
                ObjKind::Object(map)
            }
            (ObjKind::Array(_) | ObjKind::Object(_), _, _) => {
                return Err(VMError::new(&semantics::invalid_index(
                    container_type,
                    index_type,
                )))
            }
            (ObjKind::Frozen(_), _, _) => {
                return Err(VMError::new("cannot modify frozen value: index assignment"))
            }
            _ => {
                return Err(VMError::new(&semantics::invalid_index_assign(
                    container_type,
                )))
            }
        };
        Ok(Value::obj(self.gc.alloc(updated)))
    }

    fn get_str_ref<'a>(&'a self, val: &Value) -> Option<&'a str> {
        match val.as_obj().and_then(|r| self.gc.get(r)).map(|o| &o.kind) {
            Some(ObjKind::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Project a VM value onto the shared operand view (see `crate::semantics`).
    pub(super) fn semantic_operand<'g>(gc: &'g Gc, value: &Value) -> crate::semantics::Operand<'g> {
        use crate::semantics::Operand;
        match value.classify(gc) {
            ValueKind::Int(n) => Operand::Int(n),
            ValueKind::Float(f) => Operand::Float(f),
            ValueKind::Bool(_) => Operand::Bool,
            ValueKind::Null => Operand::Null,
            ValueKind::Obj(r) => match gc.get(r).map(|obj| &obj.kind) {
                Some(ObjKind::String(s)) => Operand::Str(s.as_str()),
                Some(ObjKind::Frozen(inner)) => Operand::Other(inner.type_name(gc)),
                Some(_) => Operand::Other(value.type_name(gc)),
                None => Operand::Null,
            },
        }
    }

    /// Arithmetic and ordering share their rules with the interpreter via
    /// `crate::semantics::binary`.
    fn binary_op(
        &mut self,
        left: &Value,
        right: &Value,
        op: crate::semantics::BinaryOp,
    ) -> Result<Value, VMError> {
        use crate::semantics::Outcome;
        let outcome = crate::semantics::binary(
            op,
            Self::semantic_operand(&self.gc, left),
            Self::semantic_operand(&self.gc, right),
        )
        .map_err(|message| VMError::new(&message))?;
        Ok(match outcome {
            Outcome::Int(n) => Value::int(n, &mut self.gc),
            Outcome::Float(f) => Value::float(f),
            Outcome::Bool(b) => Value::bool_val(b),
            Outcome::Concat => {
                let (l, r) = (left.display(&self.gc), right.display(&self.gc));
                self.caps
                    .check_string(l.len() + r.len())
                    .map_err(|m| VMError::new(&m))?;
                Value::obj(self.gc.alloc_string(l + &r))
            }
        })
    }

    pub(super) fn arith_op(
        &mut self,
        left: &Value,
        right: &Value,
        op: OpCode,
    ) -> Result<Value, VMError> {
        use crate::semantics::BinaryOp;
        // Fast path: non-overflowing int arithmetic never needs the shared table.
        if let (ValueKind::Int(a), ValueKind::Int(b)) =
            (left.classify(&self.gc), right.classify(&self.gc))
        {
            let fast = match op {
                OpCode::Add => a.checked_add(b),
                OpCode::Sub => a.checked_sub(b),
                OpCode::Mul => a.checked_mul(b),
                _ => None,
            };
            if let Some(r) = fast {
                return Ok(Value::int(r, &mut self.gc));
            }
        }
        let shared = match op {
            OpCode::Add => BinaryOp::Add,
            OpCode::Sub => BinaryOp::Sub,
            OpCode::Mul => BinaryOp::Mul,
            OpCode::Div => BinaryOp::Div,
            OpCode::Mod => BinaryOp::Mod,
            _ => return Err(VMError::new("invalid operation")),
        };
        self.binary_op(left, right, shared)
    }

    fn compare_op(&mut self, left: &Value, right: &Value, op: OpCode) -> Result<Value, VMError> {
        use crate::semantics::BinaryOp;
        if let (ValueKind::Int(a), ValueKind::Int(b)) =
            (left.classify(&self.gc), right.classify(&self.gc))
        {
            return Ok(Value::bool_val(match op {
                OpCode::Lt => a < b,
                OpCode::Gt => a > b,
                OpCode::LtEq => a <= b,
                OpCode::GtEq => a >= b,
                _ => false,
            }));
        }
        let shared = match op {
            OpCode::Lt => BinaryOp::Lt,
            OpCode::Gt => BinaryOp::Gt,
            OpCode::LtEq => BinaryOp::LtEq,
            OpCode::GtEq => BinaryOp::GtEq,
            _ => return Err(VMError::new("invalid comparison")),
        };
        self.binary_op(left, right, shared)
    }
}
