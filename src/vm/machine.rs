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
/// Safe because forked VMs have empty jit_cache (no raw pointers).
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
            Ok(v) => value_to_shared(&vm.gc, &v),
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
}

#[cfg(feature = "jit")]
/// Call a JIT-compiled function with arbitrary i64 arguments.
/// Supports 0–8 args; panics beyond that (Forge functions rarely exceed 8).
unsafe fn jit_call_i64(ptr: *const u8, args: &[i64]) -> i64 {
    match args.len() {
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
        _ => panic!(
            "JIT dispatch supports up to 8 arguments, got {}",
            args.len()
        ),
    }
}

#[cfg(feature = "jit")]
/// Call a JIT-compiled function with arbitrary f64 arguments.
unsafe fn jit_call_f64(ptr: *const u8, args: &[f64]) -> f64 {
    match args.len() {
        0 => {
            let f: extern "C" fn() -> f64 = std::mem::transmute(ptr);
            f()
        }
        1 => {
            let f: extern "C" fn(f64) -> f64 = std::mem::transmute(ptr);
            f(args[0])
        }
        2 => {
            let f: extern "C" fn(f64, f64) -> f64 = std::mem::transmute(ptr);
            f(args[0], args[1])
        }
        3 => {
            let f: extern "C" fn(f64, f64, f64) -> f64 = std::mem::transmute(ptr);
            f(args[0], args[1], args[2])
        }
        4 => {
            let f: extern "C" fn(f64, f64, f64, f64) -> f64 = std::mem::transmute(ptr);
            f(args[0], args[1], args[2], args[3])
        }
        5 => {
            let f: extern "C" fn(f64, f64, f64, f64, f64) -> f64 = std::mem::transmute(ptr);
            f(args[0], args[1], args[2], args[3], args[4])
        }
        6 => {
            let f: extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64 = std::mem::transmute(ptr);
            f(args[0], args[1], args[2], args[3], args[4], args[5])
        }
        7 => {
            let f: extern "C" fn(f64, f64, f64, f64, f64, f64, f64) -> f64 =
                std::mem::transmute(ptr);
            f(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            )
        }
        8 => {
            let f: extern "C" fn(f64, f64, f64, f64, f64, f64, f64, f64) -> f64 =
                std::mem::transmute(ptr);
            f(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            )
        }
        _ => panic!(
            "JIT dispatch supports up to 8 arguments, got {}",
            args.len()
        ),
    }
}

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
    pub profiler: Profiler,
    skip_timeout_check_once: bool,
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
            registers: vec![Value::Null; 256],
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
            profiler: Profiler::new(false),
            skip_timeout_check_once: false,
        };
        vm.register_builtins();
        vm
    }

    pub fn with_profiling() -> Self {
        let mut vm = Self {
            registers: vec![Value::Null; 256],
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
            profiler: Profiler::new(true),
            skip_timeout_check_once: false,
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
        ];
        for name in &builtins {
            let name_ref = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: name.to_string(),
            }));
            self.globals.insert(name.to_string(), Value::Obj(name_ref));
        }

        self.globals.insert("null".to_string(), Value::Null);

        // Register stdlib modules
        self.register_stdlib();
    }

    fn register_stdlib(&mut self) {
        // math module
        let mut math_map = IndexMap::new();
        math_map.insert("pi".to_string(), Value::Float(std::f64::consts::PI));
        math_map.insert("e".to_string(), Value::Float(std::f64::consts::E));
        for name in &[
            "sqrt", "pow", "abs", "max", "min", "floor", "ceil", "round", "random", "sin", "cos",
            "tan", "log",
        ] {
            let full = format!("math.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            math_map.insert(name.to_string(), Value::Obj(nr));
        }
        let math_ref = self.gc.alloc(ObjKind::Object(math_map));
        self.globals
            .insert("math".to_string(), Value::Obj(math_ref));

        // fs module
        let mut fs_map = IndexMap::new();
        for name in &[
            "read", "write", "append", "exists", "list", "remove", "mkdir",
        ] {
            let full = format!("fs.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            fs_map.insert(name.to_string(), Value::Obj(nr));
        }
        let fs_ref = self.gc.alloc(ObjKind::Object(fs_map));
        self.globals.insert("fs".to_string(), Value::Obj(fs_ref));

        // io module
        let mut io_map = IndexMap::new();
        for name in &["prompt", "print", "args"] {
            let full = format!("io.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            io_map.insert(name.to_string(), Value::Obj(nr));
        }
        let io_ref = self.gc.alloc(ObjKind::Object(io_map));
        self.globals.insert("io".to_string(), Value::Obj(io_ref));

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
            crypto_map.insert(name.to_string(), Value::Obj(nr));
        }
        let crypto_ref = self.gc.alloc(ObjKind::Object(crypto_map));
        self.globals
            .insert("crypto".to_string(), Value::Obj(crypto_ref));

        // db module
        let mut db_map = IndexMap::new();
        for name in &["open", "query", "execute", "close"] {
            let full = format!("db.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            db_map.insert(name.to_string(), Value::Obj(nr));
        }
        let db_ref = self.gc.alloc(ObjKind::Object(db_map));
        self.globals.insert("db".to_string(), Value::Obj(db_ref));

        // env module
        let mut env_map = IndexMap::new();
        for name in &["get", "set", "keys", "has"] {
            let full = format!("env.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            env_map.insert(name.to_string(), Value::Obj(nr));
        }
        let env_ref = self.gc.alloc(ObjKind::Object(env_map));
        self.globals.insert("env".to_string(), Value::Obj(env_ref));

        // json module
        let mut json_map = IndexMap::new();
        for name in &["parse", "stringify"] {
            let full = format!("json.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            json_map.insert(name.to_string(), Value::Obj(nr));
        }
        let json_ref = self.gc.alloc(ObjKind::Object(json_map));
        self.globals
            .insert("json".to_string(), Value::Obj(json_ref));

        // regex module
        let mut regex_map = IndexMap::new();
        for name in &["test", "find", "find_all", "replace", "split"] {
            let full = format!("regex.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            regex_map.insert(name.to_string(), Value::Obj(nr));
        }
        let regex_ref = self.gc.alloc(ObjKind::Object(regex_map));
        self.globals
            .insert("regex".to_string(), Value::Obj(regex_ref));

        // log module
        let mut log_map = IndexMap::new();
        for name in &["info", "warn", "error", "debug"] {
            let full = format!("log.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            log_map.insert(name.to_string(), Value::Obj(nr));
        }
        let log_ref = self.gc.alloc(ObjKind::Object(log_map));
        self.globals.insert("log".to_string(), Value::Obj(log_ref));

        // http module
        let mut http_map = IndexMap::new();
        for name in &[
            "get", "post", "put", "delete", "patch", "head", "download", "crawl",
        ] {
            let full = format!("http.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            http_map.insert(name.to_string(), Value::Obj(nr));
        }
        let http_ref = self.gc.alloc(ObjKind::Object(http_map));
        self.globals
            .insert("http".to_string(), Value::Obj(http_ref));

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
            term_map.insert(name.to_string(), Value::Obj(nr));
        }
        let term_ref = self.gc.alloc(ObjKind::Object(term_map));
        self.globals
            .insert("term".to_string(), Value::Obj(term_ref));

        // csv module
        let mut csv_map = IndexMap::new();
        for name in &["parse", "stringify", "read", "write"] {
            let full = format!("csv.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            csv_map.insert(name.to_string(), Value::Obj(nr));
        }
        let csv_ref = self.gc.alloc(ObjKind::Object(csv_map));
        self.globals.insert("csv".to_string(), Value::Obj(csv_ref));

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
            time_map.insert(name.to_string(), Value::Obj(nr));
        }
        let time_ref = self.gc.alloc(ObjKind::Object(time_map));
        self.globals
            .insert("time".to_string(), Value::Obj(time_ref));

        // pg module
        #[cfg(feature = "postgres")]
        {
            let mut pg_map = IndexMap::new();
            for name in &["connect", "query", "execute", "close"] {
                let full = format!("pg.{}", name);
                let nr = self
                    .gc
                    .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
                pg_map.insert(name.to_string(), Value::Obj(nr));
            }
            let pg_ref = self.gc.alloc(ObjKind::Object(pg_map));
            self.globals.insert("pg".to_string(), Value::Obj(pg_ref));
        }

        // jwt module
        let mut jwt_map = IndexMap::new();
        for name in &["sign", "verify", "decode", "valid"] {
            let full = format!("jwt.{}", name);
            let nr = self
                .gc
                .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
            jwt_map.insert(name.to_string(), Value::Obj(nr));
        }
        let jwt_ref = self.gc.alloc(ObjKind::Object(jwt_map));
        self.globals.insert("jwt".to_string(), Value::Obj(jwt_ref));

        // mysql module
        #[cfg(feature = "mysql")]
        {
            let mut mysql_map = IndexMap::new();
            for name in &["connect", "query", "execute", "close"] {
                let full = format!("mysql.{}", name);
                let nr = self
                    .gc
                    .alloc(ObjKind::NativeFunction(NativeFn { name: full }));
                mysql_map.insert(name.to_string(), Value::Obj(nr));
            }
            let mysql_ref = self.gc.alloc(ObjKind::Object(mysql_map));
            self.globals
                .insert("mysql".to_string(), Value::Obj(mysql_ref));
        }

        // Option prelude
        let mut none_obj = IndexMap::new();
        none_obj.insert("__type__".to_string(), self.alloc_string("Option"));
        none_obj.insert("__variant__".to_string(), self.alloc_string("None"));
        let none_ref = self.gc.alloc(ObjKind::Object(none_obj));
        self.globals
            .insert("None".to_string(), Value::Obj(none_ref));

        let some_native = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
            name: "Some".to_string(),
        }));
        self.globals
            .insert("Some".to_string(), Value::Obj(some_native));
    }

    pub(super) fn alloc_string(&mut self, s: &str) -> Value {
        let r = self.gc.alloc_string(s.to_string());
        Value::Obj(r)
    }

    pub(super) fn alloc_builtin(&mut self, name: &str) -> Value {
        let native = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
            name: name.to_string(),
        }));
        Value::Obj(native)
    }

    fn constant_to_value(&mut self, constant: &Constant) -> Value {
        match constant {
            Constant::Int(n) => Value::Int(*n),
            Constant::Float(n) => Value::Float(*n),
            Constant::Bool(b) => Value::Bool(*b),
            Constant::Null => Value::Null,
            Constant::Str(s) => {
                let r = self.gc.alloc_string(s.clone());
                Value::Obj(r)
            }
        }
    }

    pub(super) fn get_string(&self, val: &Value) -> Option<String> {
        if let Value::Obj(r) = val {
            if let Some(obj) = self.gc.get(*r) {
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
            if matches!(shared, SharedValue::Null) && !matches!(val, Value::Null) {
                continue;
            }
            let child_val = shared_to_value(&mut child.gc, &shared);
            child.globals.insert(name.clone(), child_val);
        }

        for (name, methods) in &self.method_tables {
            let mut child_methods = IndexMap::new();
            for (k, v) in methods {
                let shared = value_to_shared(&self.gc, v);
                if matches!(shared, SharedValue::Null) && !matches!(v, Value::Null) {
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
                if matches!(shared, SharedValue::Null) && !matches!(v, Value::Null) {
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
                if matches!(shared, SharedValue::Null) && !matches!(v, Value::Null) {
                    continue;
                }
                child_defaults.insert(k.clone(), shared_to_value(&mut child.gc, &shared));
            }
            child.struct_defaults.insert(name.clone(), child_defaults);
        }

        #[cfg(feature = "jit")]
        debug_assert!(
            child.jit_cache.is_empty() && child.jit_modules.is_empty(),
            "BUG: SendableVM must have empty jit_cache/jit_modules to be safely Send"
        );
        SendableVM(child)
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
                        .unwrap_or(Value::Null);
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
                Value::Obj(r)
            }
            ObjKind::Function(f) => {
                let function = ObjFunction {
                    name: f.name.clone(),
                    chunk: std::sync::Arc::clone(&f.chunk),
                };
                let r = child.gc.alloc(ObjKind::Function(function));
                Value::Obj(r)
            }
            _ => Value::Null,
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

        self.frames.push(CallFrame::new(closure_ref, 0));
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
        let new_base = self.frames.last().map(|f| f.base + 256).unwrap_or(0);
        if self.frames.len() >= MAX_FRAMES {
            return Err(VMError::new("stack overflow"));
        }
        self.ensure_registers(new_base + 256);
        self.frames.push(CallFrame::new(closure_ref, new_base));
        let boundary = self.frames.len() - 1;
        self.run_until(boundary)
    }

    fn ensure_registers(&mut self, needed: usize) {
        if needed > self.registers.len() {
            self.registers.resize(needed, Value::Null);
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
                return Ok(Value::Null);
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
                        self.registers[base + a as usize] = Value::Null;
                    }
                    OpCode::LoadTrue => {
                        self.registers[base + a as usize] = Value::Bool(true);
                    }
                    OpCode::LoadFalse => {
                        self.registers[base + a as usize] = Value::Bool(false);
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
                        let src = &self.registers[base + b as usize];
                        self.registers[base + a as usize] = match src {
                            Value::Int(n) => Value::Int(-n),
                            Value::Float(n) => Value::Float(-n),
                            _ => return Err(VMError::new("cannot negate non-number")),
                        };
                    }
                    OpCode::Eq => {
                        let left = &self.registers[base + b as usize];
                        let right = &self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            Value::Bool(left.equals(right, &self.gc));
                    }
                    OpCode::NotEq => {
                        let left = &self.registers[base + b as usize];
                        let right = &self.registers[base + c as usize];
                        self.registers[base + a as usize] =
                            Value::Bool(!left.equals(right, &self.gc));
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
                        self.registers[base + a as usize] = Value::Bool(left && right);
                    }
                    OpCode::Or => {
                        let left = self.registers[base + b as usize].is_truthy(&self.gc);
                        let right = self.registers[base + c as usize].is_truthy(&self.gc);
                        self.registers[base + a as usize] = Value::Bool(left || right);
                    }
                    OpCode::Not => {
                        let val = self.registers[base + b as usize].is_truthy(&self.gc);
                        self.registers[base + a as usize] = Value::Bool(!val);
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
                        return Ok(Some(Value::Null));
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
                        self.registers[base + a as usize] = Value::Obj(r);
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
                        self.registers[base + a as usize] = Value::Obj(r);
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
                        self.registers[base + a as usize] = Value::Obj(r);
                    }
                    OpCode::GetField => {
                        let obj_val = &self.registers[base + b as usize];
                        let field_const = &chunk.constants[c as usize];
                        if let (Value::Obj(r), Constant::Str(field)) = (obj_val, field_const) {
                            let r = *r;
                            let field = field.clone();
                            let needs_alloc: Option<String>;
                            let direct_result: Option<Value>;
                            if let Some(obj) = self.gc.get(r) {
                                match &obj.kind {
                                    ObjKind::Object(map) => {
                                        if let Some(value) = map.get(&field).cloned() {
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
                                                    let Some(Value::Obj(embed_ref)) =
                                                        map.get(&embed_field)
                                                    else {
                                                        continue;
                                                    };
                                                    let Some(embed_obj) = self.gc.get(*embed_ref)
                                                    else {
                                                        continue;
                                                    };
                                                    let ObjKind::Object(embed_map) =
                                                        &embed_obj.kind
                                                    else {
                                                        continue;
                                                    };
                                                    if let Some(value) = embed_map.get(&field) {
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
                                            direct_result = Some(
                                                map.get(&field).cloned().ok_or_else(|| {
                                                    VMError::new(&format!(
                                                        "no field '{}' on object",
                                                        field
                                                    ))
                                                })?,
                                            );
                                        }
                                        needs_alloc = None;
                                    }
                                    ObjKind::String(s) => match field.as_str() {
                                        "len" => {
                                            direct_result = Some(Value::Int(s.len() as i64));
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
                                    ObjKind::Array(items) => match field.as_str() {
                                        "len" => {
                                            direct_result = Some(Value::Int(items.len() as i64));
                                            needs_alloc = None;
                                        }
                                        _ => {
                                            return Err(VMError::new(&format!(
                                                "no method '{}' on Array",
                                                field
                                            )))
                                        }
                                    },
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
                            let obj_ref = if let Value::Obj(r) = &self.registers[base + a as usize]
                            {
                                *r
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
                        let result = match (&obj, &idx) {
                            (Value::Obj(r), Value::Int(i)) => {
                                if let Some(o) = self.gc.get(*r) {
                                    if let ObjKind::Array(items) = &o.kind {
                                        items
                                            .get(*i as usize)
                                            .cloned()
                                            .ok_or_else(|| VMError::new("index out of bounds"))?
                                    } else {
                                        return Err(VMError::new("cannot index non-array"));
                                    }
                                } else {
                                    Value::Null
                                }
                            }
                            (Value::Obj(r), Value::Obj(_key_ref)) => {
                                let key = self.get_string(&idx).ok_or_else(|| {
                                    VMError::new("index must be string for objects")
                                })?;
                                if let Some(o) = self.gc.get(*r) {
                                    if let ObjKind::Object(map) = &o.kind {
                                        map.get(&key).cloned().unwrap_or(Value::Null)
                                    } else {
                                        Value::Null
                                    }
                                } else {
                                    Value::Null
                                }
                            }
                            _ => return Err(VMError::new("invalid index operation")),
                        };
                        self.registers[base + a as usize] = result;
                    }
                    OpCode::SetIndex => {
                        let idx = self.registers[base + b as usize];
                        let val = self.registers[base + c as usize];
                        if let Value::Obj(r) = &self.registers[base + a as usize] {
                            let r = *r;
                            let key_str = self.get_string(&idx);
                            if let Some(obj) = self.gc.get_mut(r) {
                                match (&mut obj.kind, &idx) {
                                    (ObjKind::Array(items), Value::Int(i)) => {
                                        let i = *i as usize;
                                        if i < items.len() {
                                            items[i] = val;
                                        }
                                    }
                                    (ObjKind::Object(map), _) => {
                                        if let Some(key) = key_str {
                                            map.insert(key, val);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    OpCode::Len => {
                        let src = &self.registers[base + b as usize];
                        let len = match src {
                            Value::Obj(r) => {
                                if let Some(obj) = self.gc.get(*r) {
                                    match &obj.kind {
                                        ObjKind::String(s) => s.chars().count() as i64,
                                        ObjKind::Array(a) => a.len() as i64,
                                        ObjKind::Object(o) => o.len() as i64,
                                        _ => 0,
                                    }
                                } else {
                                    0
                                }
                            }
                            _ => 0,
                        };
                        self.registers[base + a as usize] = Value::Int(len);
                    }
                    OpCode::Concat => {
                        let left = self.registers[base + b as usize].display(&self.gc);
                        let right = self.registers[base + c as usize].display(&self.gc);
                        let r = self.gc.alloc_string(format!("{}{}", left, right));
                        self.registers[base + a as usize] = Value::Obj(r);
                    }
                    OpCode::Interpolate => {
                        let start = base + b as usize;
                        let count = c as usize;
                        let mut result = String::new();
                        for i in 0..count {
                            result.push_str(&self.registers[start + i].display(&self.gc));
                        }
                        let r = self.gc.alloc_string(result);
                        self.registers[base + a as usize] = Value::Obj(r);
                    }
                    OpCode::ExtractField => {
                        let obj = &self.registers[base + b as usize];
                        let field_name = format!("_{}", c);
                        if let Value::Obj(r) = obj {
                            if let Some(o) = self.gc.get(*r) {
                                if let ObjKind::Object(map) = &o.kind {
                                    self.registers[base + a as usize] =
                                        map.get(&field_name).cloned().unwrap_or(Value::Null);
                                }
                            }
                        }
                    }
                    OpCode::Try => {
                        let src = &self.registers[base + b as usize];
                        if let Value::Obj(r) = src {
                            if let Some(obj) = self.gc.get(*r) {
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
                        let child_closure = if let Value::Obj(r) = &closure_val {
                            self.transfer_closure(*r, &mut sendable.0)
                        } else {
                            Value::Null
                        };

                        spawn_thread(sendable, child_closure, slot_clone);

                        let handle = self.gc.alloc(ObjKind::TaskHandle(result_slot));
                        self.registers[base + a as usize] = Value::Obj(handle);
                    }
                    OpCode::Await => {
                        let src = self.registers[base + b as usize];
                        // Extract the Arc first, releasing the GC borrow
                        let maybe_slot = if let Value::Obj(r) = &src {
                            self.gc.get(*r).and_then(|obj| {
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
                        let seconds = match &self.registers[base + a as usize] {
                            Value::Int(n) => (*n).max(0) as u64,
                            Value::Float(n) => n.max(0.0) as u64,
                            _ => 5,
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
                        let interval_val = &self.registers[base + b as usize];
                        let secs = match interval_val {
                            Value::Int(n) if *n > 0 => {
                                // Read unit string from register C
                                let unit_val = &self.registers[base + c as usize];
                                let unit_str = if let Value::Obj(r) = unit_val {
                                    self.gc
                                        .get(*r)
                                        .and_then(|o| match &o.kind {
                                            ObjKind::String(s) => Some(s.clone()),
                                            _ => None,
                                        })
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                };
                                match unit_str.as_str() {
                                    "minutes" => *n as u64 * 60,
                                    "hours" => *n as u64 * 3600,
                                    _ => *n as u64, // "seconds" or default
                                }
                            }
                            Value::Int(_) => {
                                return Err(VMError::new(
                                    "schedule interval must be a positive integer",
                                ));
                            }
                            _ => 60, // Non-integer defaults to 60s (matches interpreter)
                        };

                        let mut sendable = self.fork_for_spawn();
                        let child_closure = if let Value::Obj(r) = &closure_val {
                            self.transfer_closure(*r, &mut sendable.0)
                        } else {
                            Value::Null
                        };

                        spawn_schedule_thread(sendable, child_closure, Duration::from_secs(secs));
                    }
                    OpCode::Watch => {
                        let closure_val = self.registers[base + a as usize];
                        let path_val = &self.registers[base + b as usize];
                        let path = if let Value::Obj(r) = path_val {
                            self.gc.get(*r).and_then(|o| match &o.kind {
                                ObjKind::String(s) => Some(s.clone()),
                                _ => None,
                            })
                        } else {
                            None
                        };
                        let path =
                            path.ok_or_else(|| VMError::new("watch requires a string path"))?;

                        let mut sendable = self.fork_for_spawn();
                        let child_closure = if let Value::Obj(r) = &closure_val {
                            self.transfer_closure(*r, &mut sendable.0)
                        } else {
                            Value::Null
                        };

                        spawn_watch_thread(sendable, child_closure, path);
                    }
                    OpCode::Must => {
                        let src = self.registers[base + b as usize];
                        let result = match &src {
                            Value::Null => {
                                return Err(VMError::new("must failed: got null"));
                            }
                            Value::Obj(r) => match self.gc.get(*r).map(|o| &o.kind) {
                                Some(ObjKind::ResultErr(v)) => {
                                    let msg = v.display(&self.gc);
                                    return Err(VMError::new(&format!("must failed: {}", msg)));
                                }
                                Some(ObjKind::ResultOk(v)) => *v,
                                _ => src,
                            },
                            _ => src,
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
                        let body = format!(
                            r#"{{"model":"{}","messages":[{{"role":"user","content":"{}"}}],"max_tokens":1000}}"#,
                            model,
                            prompt_str.replace('\\', "\\\\").replace('"', "\\\"")
                        );
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
                                    self.registers[base + a as usize] = Value::Null;
                                }
                            }
                            Ok(_) => {
                                self.registers[base + a as usize] = Value::Null;
                            }
                            Err(e) => {
                                return Err(VMError::new(&format!("ask error: {}", e)));
                            }
                        }
                    }
                    OpCode::Freeze => {
                        let src = self.registers[base + b as usize];
                        let frozen_ref = self.gc.alloc(ObjKind::Frozen(src));
                        self.registers[base + a as usize] = Value::Obj(frozen_ref);
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
                let max_reg = self.frames.last().map(|f| f.base + 256).unwrap_or(256);
                let scan_limit = max_reg.min(self.registers.len());
                let mut roots = Vec::with_capacity(scan_limit / 4);
                for r in &self.registers[..scan_limit] {
                    if let Value::Obj(gr) = r {
                        roots.push(*gr);
                    }
                }
                for v in self.globals.values() {
                    if let Value::Obj(gr) = v {
                        roots.push(*gr);
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
                        if let Value::Obj(gr) = v {
                            roots.push(*gr);
                        }
                    }
                }
                for methods in self.static_methods.values() {
                    for v in methods.values() {
                        if let Value::Obj(gr) = v {
                            roots.push(*gr);
                        }
                    }
                }
                for defaults in self.struct_defaults.values() {
                    for v in defaults.values() {
                        if let Value::Obj(gr) = v {
                            roots.push(*gr);
                        }
                    }
                }
                self.gc.collect(&roots);
            }
        }
    }

    pub fn call_value(&mut self, func: Value, args: Vec<Value>) -> Result<Value, VMError> {
        match &func {
            Value::Obj(r) => {
                let obj = self
                    .gc
                    .get(*r)
                    .ok_or_else(|| VMError::new("null function"))?;
                match &obj.kind {
                    ObjKind::Closure(closure) => {
                        let chunk = closure.function.chunk.clone();
                        let func_name = closure.function.name.clone();

                        if !func_name.is_empty() {
                            self.profiler.enter_function(&func_name);
                        }

                        // Auto-JIT: compile hot functions on the fly
                        #[cfg(feature = "jit")]
                        if !func_name.is_empty()
                            && !self.jit_cache.contains_key(&func_name)
                            && self.profiler.is_hot(&func_name)
                        {
                            let type_info = super::jit::type_analysis::analyze(&chunk);
                            if !type_info.has_unsupported_ops && chunk.arity <= 8 {
                                if let Ok(mut jit) = super::jit::jit_module::JitCompiler::new() {
                                    if let Ok(ptr) = jit.compile_function(&chunk, &func_name) {
                                        self.jit_cache.insert(
                                            func_name.clone(),
                                            JitEntry {
                                                ptr,
                                                uses_float: type_info.has_float,
                                            },
                                        );
                                        self.jit_modules.push(jit);
                                    }
                                }
                            }
                        }

                        // JIT dispatch
                        #[cfg(feature = "jit")]
                        if !func_name.is_empty() {
                            if let Some(&entry) = self.jit_cache.get(&func_name) {
                                let result_val = if entry.uses_float {
                                    let raw_args: Vec<f64> = args
                                        .iter()
                                        .map(|v| match v {
                                            Value::Int(n) => *n as f64,
                                            Value::Float(f) => *f,
                                            Value::Bool(b) => {
                                                if *b {
                                                    1.0
                                                } else {
                                                    0.0
                                                }
                                            }
                                            _ => 0.0,
                                        })
                                        .collect();
                                    let result: f64 = unsafe { jit_call_f64(entry.ptr, &raw_args) };
                                    if result.fract() == 0.0
                                        && result >= i64::MIN as f64
                                        && result <= i64::MAX as f64
                                    {
                                        Value::Int(result as i64)
                                    } else {
                                        Value::Float(result)
                                    }
                                } else {
                                    let raw_args: Vec<i64> = args
                                        .iter()
                                        .map(|v| match v {
                                            Value::Int(n) => *n,
                                            Value::Bool(b) => {
                                                if *b {
                                                    1
                                                } else {
                                                    0
                                                }
                                            }
                                            _ => 0,
                                        })
                                        .collect();
                                    let result: i64 = unsafe { jit_call_i64(entry.ptr, &raw_args) };
                                    Value::Int(result)
                                };
                                self.profiler.exit_function();
                                return Ok(result_val);
                            }
                        }

                        let arity = chunk.arity as usize;
                        let new_base = self.frames.last().map(|f| f.base + 256).unwrap_or(0);
                        if self.frames.len() >= MAX_FRAMES {
                            return Err(VMError::new("stack overflow"));
                        }
                        self.ensure_registers(new_base + 256);

                        for (i, arg) in args.iter().enumerate() {
                            if i < arity {
                                self.registers[new_base + i] = *arg;
                            }
                        }
                        for i in args.len()..arity {
                            self.registers[new_base + i] = Value::Null;
                        }

                        self.frames.push(CallFrame::new(*r, new_base));
                        let boundary = self.frames.len() - 1;
                        self.run_until(boundary)
                    }
                    ObjKind::NativeFunction(nf) => {
                        let name = nf.name.clone();
                        self.call_native(&name, args)
                    }
                    _ => Err(VMError::new("cannot call non-function")),
                }
            }
            _ => Err(VMError::new("cannot call non-function")),
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

    pub(super) fn args_to_interp(&self, args: &[Value]) -> Vec<crate::interpreter::Value> {
        args.iter().map(|v| self.convert_to_interp_val(v)).collect()
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
        Value::Obj(err_ref)
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
        match v {
            Value::Int(n) => crate::interpreter::Value::Int(*n),
            Value::Float(n) => crate::interpreter::Value::Float(*n),
            Value::Bool(b) => crate::interpreter::Value::Bool(*b),
            Value::Null => crate::interpreter::Value::Null,
            Value::Obj(r) => {
                if let Some(obj) = self.gc.get(*r) {
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
            crate::interpreter::Value::Int(n) => Value::Int(*n),
            crate::interpreter::Value::Float(n) => Value::Float(*n),
            crate::interpreter::Value::Bool(b) => Value::Bool(*b),
            crate::interpreter::Value::Null => Value::Null,
            crate::interpreter::Value::String(s) => self.alloc_string(s),
            crate::interpreter::Value::Array(items) => {
                let vm_items: Vec<Value> =
                    items.iter().map(|i| self.convert_interp_value(i)).collect();
                let r = self.gc.alloc(ObjKind::Array(vm_items));
                Value::Obj(r)
            }
            crate::interpreter::Value::Object(map) => {
                let mut vm_map = IndexMap::new();
                for (k, val) in map {
                    vm_map.insert(k.clone(), self.convert_interp_value(val));
                }
                let r = self.gc.alloc(ObjKind::Object(vm_map));
                Value::Obj(r)
            }
            _ => Value::Null,
        }
    }

    fn arith_op(&mut self, left: &Value, right: &Value, op: OpCode) -> Result<Value, VMError> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => match op {
                OpCode::Add => match a.checked_add(*b) {
                    Some(r) => Ok(Value::Int(r)),
                    None => Ok(Value::Float(*a as f64 + *b as f64)),
                },
                OpCode::Sub => match a.checked_sub(*b) {
                    Some(r) => Ok(Value::Int(r)),
                    None => Ok(Value::Float(*a as f64 - *b as f64)),
                },
                OpCode::Mul => match a.checked_mul(*b) {
                    Some(r) => Ok(Value::Int(r)),
                    None => Ok(Value::Float(*a as f64 * *b as f64)),
                },
                OpCode::Div => {
                    if *b == 0 {
                        return Err(VMError::new("division by zero"));
                    }
                    Ok(Value::Int(a / b))
                }
                OpCode::Mod => {
                    if *b == 0 {
                        return Err(VMError::new("modulo by zero"));
                    }
                    Ok(Value::Int(a % b))
                }
                _ => Err(VMError::new("invalid operation")),
            },
            (Value::Float(a), Value::Float(b)) => match op {
                OpCode::Add => Ok(Value::Float(a + b)),
                OpCode::Sub => Ok(Value::Float(a - b)),
                OpCode::Mul => Ok(Value::Float(a * b)),
                OpCode::Div => Ok(Value::Float(a / b)),
                OpCode::Mod => Ok(Value::Float(a % b)),
                _ => Err(VMError::new("invalid operation")),
            },
            (Value::Int(a), Value::Float(_b)) => self.arith_op(&Value::Float(*a as f64), right, op),
            (Value::Float(_a), Value::Int(b)) => self.arith_op(left, &Value::Float(*b as f64), op),
            // String concatenation
            (Value::Obj(_), _) | (_, Value::Obj(_)) if op == OpCode::Add => {
                let ls = left.display(&self.gc);
                let rs = right.display(&self.gc);
                let r = self.gc.alloc_string(format!("{}{}", ls, rs));
                Ok(Value::Obj(r))
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
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                let result = match op {
                    OpCode::Lt => a < b,
                    OpCode::Gt => a > b,
                    OpCode::LtEq => a <= b,
                    OpCode::GtEq => a >= b,
                    _ => false,
                };
                Ok(Value::Bool(result))
            }
            (Value::Float(a), Value::Float(b)) => {
                let result = match op {
                    OpCode::Lt => a < b,
                    OpCode::Gt => a > b,
                    OpCode::LtEq => a <= b,
                    OpCode::GtEq => a >= b,
                    _ => false,
                };
                Ok(Value::Bool(result))
            }
            (Value::Int(a), Value::Float(_b)) => {
                self.compare_op(&Value::Float(*a as f64), right, op)
            }
            (Value::Float(_a), Value::Int(b)) => {
                self.compare_op(left, &Value::Float(*b as f64), op)
            }
            _ => Err(VMError::new("cannot compare non-numbers")),
        }
    }
}
