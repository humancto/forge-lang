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
/// SAFETY: fork_for_spawn() asserts jit_cache/jit_modules are empty (no raw
/// pointers cross threads). All other VM fields are owned or Arc-wrapped.
/// The assert runs in release builds to prevent UB if the invariant breaks.
struct SendableVM(VM);
unsafe impl Send for SendableVM {}

/// Run a spawned closure on a forked VM in a new OS thread.
fn spawn_thread(
    sendable: SendableVM,
    closure: Value,
    slot: Arc<(Mutex<Option<SharedValue>>, Condvar)>,
) {
    std::thread::spawn(move || {
        sendable.run(closure, slot);
    });
}

/// Run a schedule closure in a loop on a forked VM in a new OS thread.
fn spawn_schedule_thread(sendable: SendableVM, closure: Value, interval: Duration) {
    std::thread::spawn(move || {
        sendable.run_loop(closure, interval);
    });
}

/// Run a watch closure on a forked VM, polling a file path for mtime changes.
fn spawn_watch_thread(sendable: SendableVM, closure: Value, path: String) {
    std::thread::spawn(move || {
        sendable.run_watch(closure, path);
    });
}

impl SendableVM {
    fn run(mut self, closure: Value, slot: Arc<(Mutex<Option<SharedValue>>, Condvar)>) {
        let vm = &mut self.0;
        let val = match vm.call_value(closure, vec![]) {
            Ok(v) => {
                let sv = value_to_shared(&vm.gc, &v);
                if vm.check_stream_boundary().is_err() {
                    eprintln!(
                        "spawn error: Stream cannot cross the VM/interpreter boundary; call .collect() first to materialize"
                    );
                    SharedValue::Null
                } else {
                    sv
                }
            }
            Err(e) => {
                eprintln!("spawn error: {}", e.message);
                SharedValue::Null
            }
        };
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

#[cfg(feature = "jit")]
#[derive(Clone, Copy)]
pub struct JitEntry {
    pub ptr: *const u8,
    pub uses_float: bool,
    pub has_string_ops: bool,
    pub has_collection_ops: bool,
    pub has_global_ops: bool,
    /// True when the function returns a GcRef (string, array, or object).
    pub returns_obj: bool,
    /// True when the function's return type is Float (decode result as f64 bits).
    pub returns_float: bool,
}

#[cfg(feature = "jit")]
/// Call a JIT-compiled function with arbitrary i64 arguments.
/// Supports 0–8 args; returns Err beyond that.
pub(super) unsafe fn jit_call_i64(ptr: *const u8, args: &[i64]) -> Result<i64, VMError> {
    Ok(match args.len() {
        0 => {
            let f: extern "C" fn() -> i64 = std::mem::transmute(ptr);
            f()
        }
        1 => {
            let f: extern "C" fn(i64) -> i64 = std::mem::transmute(ptr);
            f(args[0])
        }
        2 => {
            let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(ptr);
            f(args[0], args[1])
        }
        3 => {
            let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(ptr);
            f(args[0], args[1], args[2])
        }
        4 => {
            let f: extern "C" fn(i64, i64, i64, i64) -> i64 = std::mem::transmute(ptr);
            f(args[0], args[1], args[2], args[3])
        }
        5 => {
            let f: extern "C" fn(i64, i64, i64, i64, i64) -> i64 = std::mem::transmute(ptr);
            f(args[0], args[1], args[2], args[3], args[4])
        }
        6 => {
            let f: extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64 = std::mem::transmute(ptr);
            f(args[0], args[1], args[2], args[3], args[4], args[5])
        }
        7 => {
            let f: extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64 =
                std::mem::transmute(ptr);
            f(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            )
        }
        8 => {
            let f: extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64 =
                std::mem::transmute(ptr);
            f(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            )
        }
        n => {
            return Err(VMError::new(&format!(
                "JIT dispatch supports up to 8 arguments, got {}",
                n
            )))
        }
    })
}

#[cfg(feature = "jit")]

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
    #[cfg(feature = "jit")]
    pub jit_cache: HashMap<String, JitEntry>,
    #[cfg(feature = "jit")]
    /// Keeps JIT-compiled code pages alive. Must never be shrunk while
    /// `jit_cache` holds pointers into these modules.
    jit_modules: Vec<super::jit::jit_module::JitCompiler>,
    #[cfg(feature = "jit")]
    /// GcRef roots for string constants baked into JIT native code.
    /// These must survive GC so that bridge calls using the baked indices
    /// continue to resolve valid objects.
    pub jit_roots: Vec<GcRef>,
    pub profiler: Profiler,
    skip_timeout_check_once: bool,
    /// Set by the Stream arms of `convert_to_interp_val` / `convert_interp_value`
    /// / `value_to_shared` when a Stream is encountered at the VM↔interpreter
    /// boundary. Callers of those conversions must check this flag after each
    /// call and surface a `VMError` via `check_stream_boundary`. Streams are
    /// single-use and cannot cross engine boundaries — callers must
    /// `.collect()` first to materialize. (M9.4 bug #6.)
    pub(super) stream_boundary_error: std::cell::Cell<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorControl {
    Runtime,
    UnwoundToHandler,
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
}

impl std::fmt::Display for VMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if !self.stack_trace.is_empty() {
            for frame in &self.stack_trace {
                write!(f, "\n  at {} (line {})", frame.function, frame.line)?;
            }
        }
        Ok(())
    }
}

impl VM {
    pub fn new() -> Self {
        let mut vm = Self {
            registers: vec![Value::null(); 256],
            frames: Vec::with_capacity(MAX_FRAMES),
            globals: HashMap::new(),
            method_tables: HashMap::new(),
            static_methods: HashMap::new(),
            embedded_fields: HashMap::new(),
            struct_defaults: HashMap::new(),
            gc: Gc::new(),
            output: Vec::new(),
            #[cfg(feature = "jit")]
            jit_cache: HashMap::new(),
            #[cfg(feature = "jit")]
            jit_modules: Vec::new(),
            #[cfg(feature = "jit")]
            jit_roots: Vec::new(),
            profiler: Profiler::new(false),
            skip_timeout_check_once: false,
            stream_boundary_error: std::cell::Cell::new(false),
        };
        vm.register_builtins();
        vm
    }

    pub fn with_profiling() -> Self {
        let mut vm = Self {
            registers: vec![Value::null(); 256],
            frames: Vec::with_capacity(MAX_FRAMES),
            globals: HashMap::new(),
            method_tables: HashMap::new(),
            static_methods: HashMap::new(),
            embedded_fields: HashMap::new(),
            struct_defaults: HashMap::new(),
            gc: Gc::new(),
            output: Vec::new(),
            #[cfg(feature = "jit")]
            jit_cache: HashMap::new(),
            #[cfg(feature = "jit")]
            jit_modules: Vec::new(),
            #[cfg(feature = "jit")]
            jit_roots: Vec::new(),
            profiler: Profiler::new(true),
            skip_timeout_check_once: false,
            stream_boundary_error: std::cell::Cell::new(false),
        };
        vm.register_builtins();
        vm
    }

    fn register_builtins(&mut self) {
        let builtins = [
            "print",
            "println",
            "len",
            "type",
            "str",
            "int",
            "float",
            "push",
            "pop",
            "keys",
            "values",
            "contains",
            "range",
            "set",
            "enumerate",
            "map",
            "filter",
            "reduce",
            "sort",
            "reverse",
            "split",
            "join",
            "replace",
            "starts_with",
            "ends_with",
            "Ok",
            "Err",
            "is_ok",
            "is_err",
            "unwrap",
            "unwrap_or",
            "json",
            "fetch",
            "uuid",
            "exit",
            "run_command",
            "say",
            "yell",
            "whisper",
            "wait",
            "is_some",
            "is_none",
            "satisfies",
            "assert",
            "assert_eq",
            "shell",
            "sh",
            "sh_lines",
            "sh_json",
            "sh_ok",
            "which",
            "cwd",
            "cd",
            "lines",
            "pipe_to",
            "has_key",
            "get",
            "pick",
            "omit",
            "merge",
            "find",
            "flat_map",
            "entries",
            "from_entries",
            "ok",
            "err",
            "input",
            "Some",
            // Added in audit fix — implementations in vm/builtins.rs
            "assert_ne",
            "any",
            "all",
            "unique",
            "sum",
            "min_of",
            "max_of",
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
            // Collections
            "first",
            "last",
            "zip",
            "flatten",
            "chunk",
            "slice",
            "compact",
            "partition",
            "group_by",
            "sort_by",
            "for_each",
            "take_n",
            "skip",
            "frequencies",
            "sample",
            "shuffle",
            // Strings
            "typeof",
            "substring",
            "index_of",
            "last_index_of",
            "capitalize",
            "title",
            "upper",
            "lower",
            "trim",
            "pad_start",
            "pad_end",
            "repeat_str",
            "count",
            "slugify",
            "snake_case",
            "camel_case",
            // Results
            "unwrap_err",
            // Misc
            "diff",
            "assert_throws",
            // GenZ debug kit
            "sus",
            "bruh",
            "bet",
            "no_cap",
            "ick",
            // Execution helpers
            "cook",
            "yolo",
            "ghost",
            "slay",
            // Channels
            "channel",
            "send",
            "receive",
            "close",
            "try_send",
            "try_receive",
            "select",
            "await_all",
            "await_timeout",
        ];
        for name in &builtins {
            let name_ref = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: name.to_string(),
            }));
            self.globals.insert(name.to_string(), Value::obj(name_ref));
        }

        self.globals.insert("null".to_string(), Value::null());

        // Register stdlib modules
        self.register_stdlib();
    }

    fn register_stdlib(&mut self) {
        // math module
        let mut math_map = IndexMap::new();
        math_map.insert("pi".to_string(), Value::float(std::f64::consts::PI));
        math_map.insert("e".to_string(), Value::float(std::f64::consts::E));
        for name in &[
            "sqrt", "pow", "abs", "max", "min", "floor", "ceil", "round", "random", "sin", "cos",
            "tan", "log",
        ] {
            let full = format!("math.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            math_map.insert(name.to_string(), Value::obj(nr));
        }
        let math_ref = self.gc.alloc(ObjKind::Object(math_map));
        self.globals
            .insert("math".to_string(), Value::obj(math_ref));

        // fs module
        let mut fs_map = IndexMap::new();
        for name in &[
            "read", "write", "append", "exists", "list", "remove", "mkdir",
        ] {
            let full = format!("fs.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            fs_map.insert(name.to_string(), Value::obj(nr));
        }
        let fs_ref = self.gc.alloc(ObjKind::Object(fs_map));
        self.globals.insert("fs".to_string(), Value::obj(fs_ref));

        // io module
        let mut io_map = IndexMap::new();
        for name in &["prompt", "print", "args"] {
            let full = format!("io.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            io_map.insert(name.to_string(), Value::obj(nr));
        }
        let io_ref = self.gc.alloc(ObjKind::Object(io_map));
        self.globals.insert("io".to_string(), Value::obj(io_ref));

        // crypto module
        let mut crypto_map = IndexMap::new();
        for name in &[
            "sha256",
            "md5",
            "base64_encode",
            "base64_decode",
            "hex_encode",
            "hex_decode",
        ] {
            let full = format!("crypto.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            crypto_map.insert(name.to_string(), Value::obj(nr));
        }
        let crypto_ref = self.gc.alloc(ObjKind::Object(crypto_map));
        self.globals
            .insert("crypto".to_string(), Value::obj(crypto_ref));

        // db module
        let mut db_map = IndexMap::new();
        for name in &["open", "query", "execute", "close"] {
            let full = format!("db.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            db_map.insert(name.to_string(), Value::obj(nr));
        }
        let db_ref = self.gc.alloc(ObjKind::Object(db_map));
        self.globals.insert("db".to_string(), Value::obj(db_ref));

        // env module
        let mut env_map = IndexMap::new();
        for name in &["get", "set", "keys", "has"] {
            let full = format!("env.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            env_map.insert(name.to_string(), Value::obj(nr));
        }
        let env_ref = self.gc.alloc(ObjKind::Object(env_map));
        self.globals.insert("env".to_string(), Value::obj(env_ref));

        // json module
        let mut json_map = IndexMap::new();
        for name in &["parse", "stringify"] {
            let full = format!("json.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            json_map.insert(name.to_string(), Value::obj(nr));
        }
        let json_ref = self.gc.alloc(ObjKind::Object(json_map));
        self.globals
            .insert("json".to_string(), Value::obj(json_ref));

        // regex module
        let mut regex_map = IndexMap::new();
        for name in &["test", "find", "find_all", "replace", "split"] {
            let full = format!("regex.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            regex_map.insert(name.to_string(), Value::obj(nr));
        }
        let regex_ref = self.gc.alloc(ObjKind::Object(regex_map));
        self.globals
            .insert("regex".to_string(), Value::obj(regex_ref));

        // log module
        let mut log_map = IndexMap::new();
        for name in &["info", "warn", "error", "debug"] {
            let full = format!("log.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            log_map.insert(name.to_string(), Value::obj(nr));
        }
        let log_ref = self.gc.alloc(ObjKind::Object(log_map));
        self.globals.insert("log".to_string(), Value::obj(log_ref));

        // http module
        let mut http_map = IndexMap::new();
        for name in &[
            "get", "post", "put", "delete", "patch", "head", "download", "crawl",
        ] {
            let full = format!("http.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            http_map.insert(name.to_string(), Value::obj(nr));
        }
        let http_ref = self.gc.alloc(ObjKind::Object(http_map));
        self.globals
            .insert("http".to_string(), Value::obj(http_ref));

        // term module
        let mut term_map = IndexMap::new();
        for name in &[
            "red", "green", "blue", "yellow", "cyan", "magenta", "bold", "dim", "table", "hr",
            "clear", "confirm",
        ] {
            let full = format!("term.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            term_map.insert(name.to_string(), Value::obj(nr));
        }
        let term_ref = self.gc.alloc(ObjKind::Object(term_map));
        self.globals
            .insert("term".to_string(), Value::obj(term_ref));

        // csv module
        let mut csv_map = IndexMap::new();
        for name in &["parse", "stringify", "read", "write"] {
            let full = format!("csv.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            csv_map.insert(name.to_string(), Value::obj(nr));
        }
        let csv_ref = self.gc.alloc(ObjKind::Object(csv_map));
        self.globals.insert("csv".to_string(), Value::obj(csv_ref));

        // time module
        let mut time_map = IndexMap::new();
        for name in &[
            "now",
            "unix",
            "parse",
            "format",
            "diff",
            "add",
            "sub",
            "zone",
            "zones",
            "elapsed",
            "is_before",
            "is_after",
            "start_of",
            "end_of",
            "from_unix",
            "today",
            "date",
            "sleep",
            "measure",
            "local",
            "is_weekend",
            "is_weekday",
            "day_of_week",
            "days_in_month",
            "is_leap_year",
        ] {
            let full = format!("time.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            time_map.insert(name.to_string(), Value::obj(nr));
        }
        // time() as a function calls the "time" builtin (returns datetime object)
        let time_call = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
            name: "time".to_string(),
        }));
        time_map.insert("__call__".to_string(), Value::obj(time_call));
        let time_ref = self.gc.alloc(ObjKind::Object(time_map));
        self.globals
            .insert("time".to_string(), Value::obj(time_ref));

        // pg module
        #[cfg(feature = "postgres")]
        {
            let mut pg_map = IndexMap::new();
            for name in &["connect", "query", "execute", "close"] {
                let full = format!("pg.{}", name);
                let nr = self
                    .gc
                    .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
                pg_map.insert(name.to_string(), Value::obj(nr));
            }
            let pg_ref = self.gc.alloc(ObjKind::Object(pg_map));
            self.globals.insert("pg".to_string(), Value::obj(pg_ref));
        }

        // jwt module
        let mut jwt_map = IndexMap::new();
        for name in &["sign", "verify", "decode", "valid"] {
            let full = format!("jwt.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            jwt_map.insert(name.to_string(), Value::obj(nr));
        }
        let jwt_ref = self.gc.alloc(ObjKind::Object(jwt_map));
        self.globals.insert("jwt".to_string(), Value::obj(jwt_ref));

        // mysql module
        #[cfg(feature = "mysql")]
        {
            let mut mysql_map = IndexMap::new();
            for name in &["connect", "query", "execute", "close"] {
                let full = format!("mysql.{}", name);
                let nr = self
                    .gc
                    .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
                mysql_map.insert(name.to_string(), Value::obj(nr));
            }
            let mysql_ref = self.gc.alloc(ObjKind::Object(mysql_map));
            self.globals
                .insert("mysql".to_string(), Value::obj(mysql_ref));
        }

        // os module
        let mut os_map = IndexMap::new();
        for name in &["hostname", "platform", "arch", "pid", "cpus", "homedir"] {
            let full = format!("os.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            os_map.insert(name.to_string(), Value::obj(nr));
        }
        let os_ref = self.gc.alloc(ObjKind::Object(os_map));
        self.globals.insert("os".to_string(), Value::obj(os_ref));

        // path module
        let mut path_map = IndexMap::new();
        for name in &[
            "join",
            "resolve",
            "relative",
            "is_absolute",
            "dirname",
            "basename",
            "extname",
        ] {
            let full = format!("path.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            path_map.insert(name.to_string(), Value::obj(nr));
        }
        path_map.insert(
            "separator".to_string(),
            self.alloc_string(std::path::MAIN_SEPARATOR_STR),
        );
        let path_ref = self.gc.alloc(ObjKind::Object(path_map));
        self.globals
            .insert("path".to_string(), Value::obj(path_ref));

        // Option prelude
        let mut none_obj = IndexMap::new();
        none_obj.insert("__type__".to_string(), self.alloc_string("Option"));
        none_obj.insert("__variant__".to_string(), self.alloc_string("None"));
        let none_ref = self.gc.alloc(ObjKind::Object(none_obj));
        self.globals
            .insert("None".to_string(), Value::obj(none_ref));

        let some_native = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
            name: "Some".to_string(),
        }));
        self.globals
            .insert("Some".to_string(), Value::obj(some_native));
    }

    pub(super) fn alloc_string(&mut self, s: &str) -> Value {
        let r = self.gc.alloc_string(s.to_string());
        Value::obj(r)
    }

    pub(super) fn alloc_builtin(&mut self, name: &str) -> Value {
        let native = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
            name: name.to_string(),
        }));
        Value::obj(native)
    }

    fn constant_to_value(&mut self, constant: &Constant) -> Value {
        match constant {
            Constant::Int(n) => Value::int(*n, &mut self.gc),
            Constant::Float(n) => Value::float(*n),
            Constant::Bool(b) => Value::bool_val(*b),
            Constant::Null => Value::null(),
            Constant::Str(s) => {
                let r = self.gc.alloc_string(s.clone());
                Value::obj(r)
            }
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
    /// Calls VM::new() for fresh builtins + empty jit_cache, then copies
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

        #[cfg(feature = "jit")]
        assert!(
            child.jit_cache.is_empty() && child.jit_modules.is_empty(),
            "BUG: SendableVM must have empty jit_cache/jit_modules to be safely Send"
        );
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
        if self.frames.len() >= MAX_FRAMES {
            return Err(VMError::new("stack overflow"));
        }
        self.ensure_registers(new_base + frame_size);
        self.frames
            .push(CallFrame::new(closure_ref, new_base, frame_size));
        let boundary = self.frames.len() - 1;
        self.run_until(boundary)
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
        self.skip_timeout_check_once = true;
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
            let chunk = cached_closure.as_ref().unwrap().1.clone();

            if self.frames[frame_idx].ip >= chunk.code.len() {
                self.frames.pop();
                continue;
            }

            if self.skip_timeout_check_once {
                self.skip_timeout_check_once = false;
            } else if self.earliest_expired_timeout().is_some() {
                match self.handle_timeout_expiry() {
                    Ok(handler_frame_idx) => {
                        if handler_frame_idx < boundary_frame_idx {
                            return Err(VMError::unwound_to_handler());
                        }
                        continue;
                    }
                    Err(err) => return Err(err),
                }
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
                        self.registers[base + a as usize] = self.registers[base + b as usize];
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
                        let left = &self.registers[base + b as usize];
                        let right = &self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.compare_op(left, right, OpCode::Lt)?;
                    }
                    OpCode::Gt => {
                        let left = &self.registers[base + b as usize];
                        let right = &self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.compare_op(left, right, OpCode::Gt)?;
                    }
                    OpCode::LtEq => {
                        let left = &self.registers[base + b as usize];
                        let right = &self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.compare_op(left, right, OpCode::LtEq)?;
                    }
                    OpCode::GtEq => {
                        let left = &self.registers[base + b as usize];
                        let right = &self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            self.compare_op(left, right, OpCode::GtEq)?;
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
                                VMError::new(&format!("undefined variable: {}", name))
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
                        let local_slot = b;
                        let value = if let Some(uv_ref) = self.frames[frame_idx]
                            .open_upvalues
                            .get(&local_slot)
                            .copied()
                        {
                            let value = self
                                .gc
                                .get(uv_ref)
                                .and_then(|uv_obj| match &uv_obj.kind {
                                    ObjKind::Upvalue(uv) => Some(uv.value),
                                    _ => None,
                                })
                                .ok_or_else(|| VMError::new("invalid open upvalue"))?;
                            self.registers[base + local_slot as usize] = value;
                            value
                        } else {
                            self.registers[base + local_slot as usize]
                        };
                        self.registers[base + a as usize] = value;
                    }
                    OpCode::SetLocal => {
                        let val = self.registers[base + b as usize];
                        self.registers[base + a as usize] = val;
                        let open_upvalue = self.frames[frame_idx].open_upvalues.get(&a).copied();
                        if let Some(uv_ref) = open_upvalue {
                            if let Some(uv_obj) = self.gc.get_mut(uv_ref) {
                                if let ObjKind::Upvalue(uv) = &mut uv_obj.kind {
                                    uv.value = val;
                                }
                            }
                        }
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
                        let frame = &mut self.frames[frame_idx];
                        frame.ip = (frame.ip as i64 + sbx as i64) as usize;
                    }
                    OpCode::Call => {
                        let func_val = self.registers[base + a as usize];
                        let arg_count = b as usize;
                        let dst_reg = base + c as usize;

                        let mut args = Vec::with_capacity(arg_count);
                        for i in 0..arg_count {
                            args.push(self.registers[base + a as usize + 1 + i]);
                        }

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
                        let obj_val = &self.registers[base + b as usize];
                        let field_const = &chunk.constants[c as usize];
                        if let (Some(r), Constant::Str(field)) = (obj_val.as_obj(), field_const) {
                            let needs_alloc: Option<String>;
                            let direct_result: Option<Value>;
                            if let Some(obj) = self.gc.get(r) {
                                match &obj.kind {
                                    ObjKind::Object(map) => {
                                        if let Some(value) = map.get(field.as_str()).cloned() {
                                            direct_result = Some(value);
                                        } else if let Some(type_name) = map
                                            .get("__type__")
                                            .and_then(|value| self.get_string(value))
                                        {
                                            let mut delegated = None;
                                            if let Some(embeds) =
                                                self.embedded_fields.get(&type_name).cloned()
                                            {
                                                for (embed_field, _) in embeds {
                                                    let Some(embed_ref) = map
                                                        .get(&embed_field)
                                                        .and_then(|v| v.as_obj())
                                                    else {
                                                        continue;
                                                    };
                                                    let Some(embed_obj) = self.gc.get(embed_ref)
                                                    else {
                                                        continue;
                                                    };
                                                    let ObjKind::Object(embed_map) =
                                                        &embed_obj.kind
                                                    else {
                                                        continue;
                                                    };
                                                    if let Some(value) =
                                                        embed_map.get(field.as_str())
                                                    {
                                                        delegated = Some(*value);
                                                        break;
                                                    }
                                                }
                                            }
                                            direct_result = Some(delegated.ok_or_else(|| {
                                                VMError::new(&format!(
                                                    "no field '{}' on object",
                                                    field
                                                ))
                                            })?);
                                        } else {
                                            direct_result =
                                                Some(map.get(field.as_str()).cloned().ok_or_else(
                                                    || {
                                                        VMError::new(&format!(
                                                            "no field '{}' on object",
                                                            field
                                                        ))
                                                    },
                                                )?);
                                        }
                                        needs_alloc = None;
                                    }
                                    ObjKind::String(s) => match field.as_str() {
                                        "len" => {
                                            direct_result =
                                                Some(Value::small_int(s.chars().count() as i64));
                                            needs_alloc = None;
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
                                        _ => {
                                            return Err(VMError::new(&format!(
                                                "no method '{}' on String",
                                                field
                                            )))
                                        }
                                    },
                                    ObjKind::Array(items) | ObjKind::Set(items) => {
                                        match field.as_str() {
                                            "len" => {
                                                direct_result =
                                                    Some(Value::small_int(items.len() as i64));
                                                needs_alloc = None;
                                            }
                                            _ => {
                                                let type_name =
                                                    if matches!(&obj.kind, ObjKind::Set(_)) {
                                                        "Set"
                                                    } else {
                                                        "Array"
                                                    };
                                                return Err(VMError::new(&format!(
                                                    "no method '{}' on {}",
                                                    field, type_name
                                                )));
                                            }
                                        }
                                    }
                                    _ => {
                                        return Err(VMError::new(&format!(
                                            "cannot access field '{}' on {}",
                                            field,
                                            obj.type_name()
                                        )))
                                    }
                                }
                            } else {
                                return Err(VMError::new("null reference"));
                            }
                            let result = if let Some(s) = needs_alloc {
                                self.alloc_string(&s)
                            } else {
                                direct_result.expect(
                                    "BUG: direct_result must be Some when needs_alloc is None",
                                )
                            };
                            self.registers[base + a as usize] = result;
                        }
                    }
                    OpCode::SetField => {
                        let field_const = &chunk.constants[b as usize];
                        let val = self.registers[base + c as usize];
                        if let Constant::Str(field) = field_const {
                            let obj_ref =
                                if let Some(r) = self.registers[base + a as usize].as_obj() {
                                    r
                                } else {
                                    return Err(VMError::new("cannot set field on non-object"));
                                };
                            if let Some(obj) = self.gc.get(obj_ref) {
                                if matches!(&obj.kind, ObjKind::Frozen(_)) {
                                    return Err(VMError::new("cannot mutate a frozen value"));
                                }
                            }
                            if let Some(obj) = self.gc.get_mut(obj_ref) {
                                if let ObjKind::Object(map) = &mut obj.kind {
                                    map.insert(field.clone(), val);
                                }
                            }
                        }
                    }
                    OpCode::GetIndex => {
                        let obj = self.registers[base + b as usize];
                        let idx = self.registers[base + c as usize];
                        let result = if let Some(r) = obj.as_obj() {
                            if let Some(i) = idx.as_int(&self.gc) {
                                if let Some(o) = self.gc.get(r) {
                                    if let ObjKind::Array(items) | ObjKind::Tuple(items) = &o.kind {
                                        items
                                            .get(i as usize)
                                            .cloned()
                                            .ok_or_else(|| VMError::new("index out of bounds"))?
                                    } else if matches!(&o.kind, ObjKind::Set(_)) {
                                        return Err(VMError::new(
                                            "cannot index a set; sets are unordered — use .has() or iteration",
                                        ));
                                    } else {
                                        return Err(VMError::new("cannot index non-array"));
                                    }
                                } else {
                                    Value::null()
                                }
                            } else if idx.as_obj().is_some() {
                                let key = self.get_string(&idx).ok_or_else(|| {
                                    VMError::new("index must be string for objects")
                                })?;
                                if let Some(o) = self.gc.get(r) {
                                    if let ObjKind::Object(map) = &o.kind {
                                        map.get(&key).cloned().unwrap_or(Value::null())
                                    } else {
                                        Value::null()
                                    }
                                } else {
                                    Value::null()
                                }
                            } else {
                                return Err(VMError::new("invalid index operation"));
                            }
                        } else {
                            return Err(VMError::new("invalid index operation"));
                        };
                        self.registers[base + a as usize] = result;
                    }
                    OpCode::IterGet => {
                        let obj = self.registers[base + b as usize];
                        let idx = self.registers[base + c as usize];
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
                        let idx = self.registers[base + b as usize];
                        let val = self.registers[base + c as usize];
                        if let Some(r) = self.registers[base + a as usize].as_obj() {
                            // Check for tuple mutation
                            if let Some(obj) = self.gc.get(r) {
                                if matches!(&obj.kind, ObjKind::Tuple(_)) {
                                    return Err(VMError::new("cannot mutate a tuple"));
                                }
                                if matches!(&obj.kind, ObjKind::Set(_)) {
                                    return Err(VMError::new(
                                        "cannot index-assign a set; use .add() and .remove()",
                                    ));
                                }
                            }
                            let key_str = self.get_string(&idx);
                            let idx_int = idx.as_int(&self.gc);
                            if let Some(obj) = self.gc.get_mut(r) {
                                if let Some(i) = idx_int {
                                    if let ObjKind::Array(items) = &mut obj.kind {
                                        let i = i as usize;
                                        if i < items.len() {
                                            items[i] = val;
                                        }
                                    }
                                } else if let ObjKind::Object(map) = &mut obj.kind {
                                    if let Some(key) = key_str {
                                        map.insert(key, val);
                                    }
                                }
                            }
                        }
                    }
                    OpCode::Len => {
                        let src = self.registers[base + b as usize];
                        let len = if let Some(r) = src.as_obj() {
                            if let Some(obj) = self.gc.get(r) {
                                match &obj.kind {
                                    ObjKind::String(s) => s.chars().count() as i64,
                                    ObjKind::Array(a) | ObjKind::Tuple(a) | ObjKind::Set(a) => {
                                        a.len() as i64
                                    }
                                    ObjKind::Object(o) => o.len() as i64,
                                    ObjKind::Map(p) => p.len() as i64,
                                    _ => 0,
                                }
                            } else {
                                0
                            }
                        } else {
                            0
                        };
                        self.registers[base + a as usize] = Value::small_int(len);
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
                        if let Some(r) = obj.as_obj() {
                            if let Some(o) = self.gc.get(r) {
                                if let ObjKind::Object(map) = &o.kind {
                                    self.registers[base + a as usize] =
                                        map.get(&field_name).cloned().unwrap_or(Value::null());
                                }
                            }
                        }
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

                        spawn_thread(sendable, child_closure, slot_clone);
                        self.drain_stream_boundary_flags();

                        let handle = self.gc.alloc(ObjKind::TaskHandle(result_slot));
                        self.registers[base + a as usize] = Value::obj(handle);
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
                            shared_to_value(&mut self.gc, &shared)
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
                Err(err) => match self.handle_runtime_error(err) {
                    Ok(handler_frame_idx) => {
                        if handler_frame_idx < boundary_frame_idx {
                            return Err(VMError::unwound_to_handler());
                        }
                        continue;
                    }
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
                // Keep string constants baked into JIT native code alive.
                #[cfg(feature = "jit")]
                roots.extend_from_slice(&self.jit_roots);
                self.gc.collect(&roots);
            }
        }
    }

    pub fn call_value(&mut self, func: Value, args: Vec<Value>) -> Result<Value, VMError> {
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

                        // Count calls for profiling and JIT hotness detection.
                        // Skip for functions already JIT-compiled to avoid
                        // per-call string allocation overhead on hot paths.
                        #[cfg(feature = "jit")]
                        let already_jit =
                            !func_name.is_empty() && self.jit_cache.contains_key(&func_name);
                        #[cfg(not(feature = "jit"))]
                        let already_jit = false;

                        // Anonymous lambdas all share the name "<lambda>", so
                        // JIT cache keyed by name would collide across distinct
                        // lambdas. Exclude them from auto-JIT and hotness
                        // tracking until a stable per-prototype key exists.
                        let jit_eligible = !func_name.is_empty() && func_name != "<lambda>";
                        if jit_eligible && !already_jit {
                            self.profiler.enter_function(&func_name);
                        }

                        // Auto-JIT: compile hot functions on the fly
                        #[cfg(feature = "jit")]
                        if jit_eligible && !already_jit && self.profiler.is_hot(&func_name) {
                            let type_info = super::jit::type_analysis::analyze(&chunk);
                            let needs_vm_ptr = type_info.has_string_ops
                                || type_info.has_collection_ops
                                || type_info.has_global_ops;
                            let max_arity: u8 = if needs_vm_ptr { 7 } else { 8 };
                            if !type_info.has_unsupported_ops && chunk.arity <= max_arity {
                                // Pre-allocate string constants into GC so their
                                // GcRef indices can be baked into JIT code.
                                let string_refs = if needs_vm_ptr {
                                    let refs: Vec<Option<i64>> = chunk
                                        .constants
                                        .iter()
                                        .map(|c| match c {
                                            Constant::Str(s) => {
                                                let r = self.gc.alloc_string(s.clone());
                                                self.jit_roots.push(r);
                                                Some(r.0 as i64)
                                            }
                                            _ => None,
                                        })
                                        .collect();
                                    Some(refs)
                                } else {
                                    None
                                };
                                if let Ok(mut jit) = super::jit::jit_module::JitCompiler::new() {
                                    if let Ok(ptr) = jit.compile_function(
                                        &chunk,
                                        &func_name,
                                        string_refs.as_ref(),
                                    ) {
                                        let ret_is_obj = matches!(
                                            type_info.return_type,
                                            super::jit::type_analysis::RegType::StringRef
                                                | super::jit::type_analysis::RegType::ObjRef
                                        );
                                        self.jit_cache.insert(
                                            func_name.clone(),
                                            JitEntry {
                                                ptr,
                                                uses_float: type_info.has_float,
                                                has_string_ops: type_info.has_string_ops,
                                                has_collection_ops: type_info.has_collection_ops,
                                                has_global_ops: type_info.has_global_ops,
                                                returns_obj: ret_is_obj,
                                                returns_float: matches!(
                                                    type_info.return_type,
                                                    super::jit::type_analysis::RegType::Float
                                                ),
                                            },
                                        );
                                        self.jit_modules.push(jit);
                                    }
                                }
                            }
                        }

                        // JIT dispatch — unified I64 ABI
                        // Float values are passed/returned as IEEE 754 bits in i64.
                        #[cfg(feature = "jit")]
                        if jit_eligible {
                            if let Some(&entry) = self.jit_cache.get(&func_name) {
                                let mut raw_args: Vec<i64> = Vec::new();
                                if entry.has_string_ops
                                    || entry.has_collection_ops
                                    || entry.has_global_ops
                                {
                                    raw_args.push(self as *mut VM as *mut () as i64);
                                }
                                for v in &args {
                                    raw_args.push(if let Some(n) = v.as_inline_int() {
                                        if entry.uses_float {
                                            // Float functions expect all args as f64 bits
                                            (n as f64).to_bits() as i64
                                        } else {
                                            n
                                        }
                                    } else if let Some(f) = v.as_float() {
                                        // Float values: pass IEEE 754 bits in i64
                                        f.to_bits() as i64
                                    } else if let Some(b) = v.as_bool() {
                                        if entry.uses_float {
                                            (if b { 1.0_f64 } else { 0.0_f64 }).to_bits() as i64
                                        } else if b {
                                            1
                                        } else {
                                            0
                                        }
                                    } else if let Some(r) = v.as_obj() {
                                        r.0 as i64
                                    } else {
                                        0
                                    });
                                }
                                let result: i64 = unsafe { jit_call_i64(entry.ptr, &raw_args)? };
                                let result_val = if entry.returns_obj {
                                    Value::obj(GcRef(result as usize))
                                } else if entry.returns_float {
                                    let f = f64::from_bits(result as u64);
                                    if f.fract() == 0.0
                                        && f >= i64::MIN as f64
                                        && f <= i64::MAX as f64
                                    {
                                        Value::int(f as i64, &mut self.gc)
                                    } else {
                                        Value::float(f)
                                    }
                                } else {
                                    Value::int(result, &mut self.gc)
                                };
                                self.profiler.exit_function();
                                return Ok(result_val);
                            }
                        }

                        let arity = chunk.arity as usize;
                        let frame_size = (chunk.max_registers as usize).max(1);
                        let new_base = self.frames.last().map(|f| f.base + f.size).unwrap_or(0);
                        if self.frames.len() >= MAX_FRAMES {
                            return Err(VMError::new("stack overflow"));
                        }
                        self.ensure_registers(new_base + frame_size);

                        for (i, arg) in args.iter().enumerate() {
                            if i < arity {
                                self.registers[new_base + i] = *arg;
                            }
                        }
                        for i in args.len()..arity {
                            self.registers[new_base + i] = Value::null();
                        }

                        self.frames.push(CallFrame::new(r, new_base, frame_size));
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
                    trace.push(StackFrame {
                        function: c.function.name.clone(),
                        line,
                    });
                }
            }
        }
        trace
    }

    #[allow(dead_code)]
    fn error_with_trace(&self, msg: &str) -> VMError {
        VMError::with_trace(msg, self.collect_stack_trace())
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

    fn handle_runtime_error(&mut self, err: VMError) -> Result<usize, VMError> {
        if err.is_unwound_to_handler() {
            return Err(err);
        }

        for frame_idx in (0..self.frames.len()).rev() {
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

        if err.stack_trace.is_empty() {
            Err(self.error_with_trace(&err.message))
        } else {
            Err(err)
        }
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

    fn arith_op(&mut self, left: &Value, right: &Value, op: OpCode) -> Result<Value, VMError> {
        match (left.classify(&self.gc), right.classify(&self.gc)) {
            (ValueKind::Int(a), ValueKind::Int(b)) => match op {
                OpCode::Add => match a.checked_add(b) {
                    Some(r) => Ok(Value::int(r, &mut self.gc)),
                    None => Ok(Value::float(a as f64 + b as f64)),
                },
                OpCode::Sub => match a.checked_sub(b) {
                    Some(r) => Ok(Value::int(r, &mut self.gc)),
                    None => Ok(Value::float(a as f64 - b as f64)),
                },
                OpCode::Mul => match a.checked_mul(b) {
                    Some(r) => Ok(Value::int(r, &mut self.gc)),
                    None => Ok(Value::float(a as f64 * b as f64)),
                },
                OpCode::Div => {
                    if b == 0 {
                        return Err(VMError::new("division by zero"));
                    }
                    Ok(Value::int(a / b, &mut self.gc))
                }
                OpCode::Mod => {
                    if b == 0 {
                        return Err(VMError::new("modulo by zero"));
                    }
                    Ok(Value::int(a % b, &mut self.gc))
                }
                _ => Err(VMError::new("invalid operation")),
            },
            (ValueKind::Float(a), ValueKind::Float(b)) => match op {
                OpCode::Add => Ok(Value::float(a + b)),
                OpCode::Sub => Ok(Value::float(a - b)),
                OpCode::Mul => Ok(Value::float(a * b)),
                OpCode::Div => Ok(Value::float(a / b)),
                OpCode::Mod => Ok(Value::float(a % b)),
                _ => Err(VMError::new("invalid operation")),
            },
            (ValueKind::Int(a), ValueKind::Float(_b)) => {
                self.arith_op(&Value::float(a as f64), right, op)
            }
            (ValueKind::Float(_a), ValueKind::Int(b)) => {
                self.arith_op(left, &Value::float(b as f64), op)
            }
            // String concatenation
            (ValueKind::Obj(_), _) | (_, ValueKind::Obj(_)) if op == OpCode::Add => {
                let ls = left.display(&self.gc);
                let rs = right.display(&self.gc);
                let r = self.gc.alloc_string(format!("{}{}", ls, rs));
                Ok(Value::obj(r))
            }
            _ => Err(VMError::new(&format!(
                "cannot apply {:?} to {} and {}",
                op,
                left.type_name(&self.gc),
                right.type_name(&self.gc)
            ))),
        }
    }

    fn compare_op(&self, left: &Value, right: &Value, op: OpCode) -> Result<Value, VMError> {
        match (left.classify(&self.gc), right.classify(&self.gc)) {
            (ValueKind::Int(a), ValueKind::Int(b)) => {
                let result = match op {
                    OpCode::Lt => a < b,
                    OpCode::Gt => a > b,
                    OpCode::LtEq => a <= b,
                    OpCode::GtEq => a >= b,
                    _ => false,
                };
                Ok(Value::bool_val(result))
            }
            (ValueKind::Float(a), ValueKind::Float(b)) => {
                let result = match op {
                    OpCode::Lt => a < b,
                    OpCode::Gt => a > b,
                    OpCode::LtEq => a <= b,
                    OpCode::GtEq => a >= b,
                    _ => false,
                };
                Ok(Value::bool_val(result))
            }
            (ValueKind::Int(a), ValueKind::Float(_b)) => {
                self.compare_op(&Value::float(a as f64), right, op)
            }
            (ValueKind::Float(_a), ValueKind::Int(b)) => {
                self.compare_op(left, &Value::float(b as f64), op)
            }
            _ => Err(VMError::new("cannot compare non-numbers")),
        }
    }
}
