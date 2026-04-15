mod builtins; // call_builtin — extracted for readability
use crate::parser::ast::*;
/// Forge Tree-Walk Interpreter
/// Walks the AST and executes it directly.
/// Phase 1 only — replaced by bytecode VM in Phase 3.
use indexmap::IndexMap;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

/// Escape a string for safe JSON embedding. Handles backslashes, quotes,
/// newlines, tabs, carriage returns, and control characters.
fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Sender variant: bounded (sync_channel) or unbounded (channel)
#[derive(Debug)]
pub enum ChannelSender {
    Bounded(std::sync::mpsc::SyncSender<Value>),
    Unbounded(std::sync::mpsc::Sender<Value>),
}

/// Thread-safe channel inner type
#[derive(Debug)]
#[allow(dead_code)]
pub struct ChannelInner {
    pub tx: std::sync::Mutex<Option<ChannelSender>>,
    pub rx: std::sync::Mutex<Option<std::sync::mpsc::Receiver<Value>>>,
    pub capacity: Option<usize>,
}

/// Runtime values
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Array(Vec<Value>),
    Tuple(Vec<Value>),
    Set(Vec<Value>),
    Object(IndexMap<String, Value>),
    Function {
        name: String,
        params: Vec<Param>,
        body: Vec<SpannedStmt>,
        closure: Environment,
        decorators: Vec<Decorator>,
    },
    Lambda {
        params: Vec<Param>,
        body: Vec<SpannedStmt>,
        closure: Arc<std::sync::Mutex<Environment>>,
    },
    ResultOk(Box<Value>),
    ResultErr(Box<Value>),
    /// Option type: Some(value) or None
    Some(Box<Value>),
    None,
    /// Built-in function
    BuiltIn(String),
    /// Task handle from spawn (awaitable) — uses Condvar for notification
    TaskHandle(Arc<(std::sync::Mutex<Option<Value>>, std::sync::Condvar)>),
    /// Thread-safe channel for send/receive
    Channel(Arc<ChannelInner>),
    /// Frozen (immutable) wrapper — prevents field/index mutation
    Frozen(Box<Value>),
    Null,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => {
                // Order-independent with container-aware equality: same length
                // + every element in A is equivalent to some element in B
                // (handles NaN==NaN and Int/Float promotion for set semantics).
                a.len() == b.len()
                    && a.iter()
                        .all(|x| b.iter().any(|y| Value::container_eq(x, y)))
            }
            (Value::Object(a), Value::Object(b)) => a == b,
            (Value::ResultOk(a), Value::ResultOk(b)) => a == b,
            (Value::ResultErr(a), Value::ResultErr(b)) => a == b,
            (Value::Some(a), Value::Some(b)) => a == b,
            (Value::None, Value::None) => true,
            (Value::BuiltIn(a), Value::BuiltIn(b)) => a == b,
            (Value::Channel(a), Value::Channel(b)) => Arc::ptr_eq(a, b),
            (Value::Frozen(a), b) => a.as_ref() == b,
            (a, Value::Frozen(b)) => a == b.as_ref(),
            _ => false,
        }
    }
}

impl Value {
    /// Container-aware equality used for set membership, set equality, and
    /// any other collection where we want NaN==NaN and Int↔Float promotion
    /// to agree with the VM's `Value::equals` semantics.
    ///
    /// Differs from `PartialEq` in three ways:
    ///   1. `NaN == NaN` is `true` (so `set([f64::NAN])` dedups correctly).
    ///   2. `Int(n) == Float(n as f64)` is `true` (so the two backends agree
    ///      on `set([1, 2]) == set([1.0, 2.0])` and `.has(1.0)`).
    ///   3. Recursively applies itself to nested Array/Tuple/Set/Object so
    ///      the above two rules propagate through containers.
    pub fn container_eq(a: &Value, b: &Value) -> bool {
        // Peel Frozen on either side.
        if let Value::Frozen(inner) = a {
            return Value::container_eq(inner, b);
        }
        if let Value::Frozen(inner) = b {
            return Value::container_eq(a, inner);
        }
        match (a, b) {
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => (x.is_nan() && y.is_nan()) || x == y,
            (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => {
                !y.is_nan() && (*x as f64) == *y
            }
            (Value::Array(x), Value::Array(y)) | (Value::Tuple(x), Value::Tuple(y)) => {
                x.len() == y.len()
                    && x.iter()
                        .zip(y.iter())
                        .all(|(a, b)| Value::container_eq(a, b))
            }
            (Value::Set(x), Value::Set(y)) => {
                x.len() == y.len()
                    && x.iter()
                        .all(|xv| y.iter().any(|yv| Value::container_eq(xv, yv)))
            }
            (Value::Object(x), Value::Object(y)) => {
                x.len() == y.len()
                    && x.iter()
                        .all(|(k, v)| y.get(k).is_some_and(|yv| Value::container_eq(v, yv)))
            }
            (Value::ResultOk(x), Value::ResultOk(y))
            | (Value::ResultErr(x), Value::ResultErr(y)) => Value::container_eq(x, y),
            (Value::Some(x), Value::Some(y)) => Value::container_eq(x, y),
            // Everything else: defer to PartialEq (String, Bool, Null, None, BuiltIn, Channel).
            _ => a == b,
        }
    }

    pub fn type_name(&self) -> &str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::String(_) => "String",
            Value::Bool(_) => "Bool",
            Value::Array(_) => "Array",
            Value::Tuple(_) => "Tuple",
            Value::Set(_) => "Set",
            Value::Object(_) => "Object",
            Value::Function { .. } => "Function",
            Value::Lambda { .. } => "Lambda",
            Value::ResultOk(_) | Value::ResultErr(_) => "Result",
            Value::Some(_) | Value::None => "Option",
            Value::BuiltIn(_) => "BuiltIn",
            Value::TaskHandle(_) => "TaskHandle",
            Value::Channel(_) => "Channel",
            Value::Frozen(inner) => inner.type_name(),
            Value::Null => "Null",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Null => false,
            Value::Array(a) | Value::Tuple(a) | Value::Set(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            Value::ResultOk(_) => true,
            Value::ResultErr(_) => false,
            Value::Some(_) => true,
            Value::None => false,
            Value::Frozen(inner) => inner.is_truthy(),
            _ => true,
        }
    }

    /// Check if this value is frozen (immutable)
    pub fn is_frozen(&self) -> bool {
        matches!(self, Value::Frozen(_))
    }

    pub fn to_json_string(&self) -> String {
        match self {
            Value::Object(map) => {
                let entries: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", escape_json_string(k), v.to_json_string()))
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
            Value::Array(items) => {
                let entries: Vec<String> = items.iter().map(|v| v.to_json_string()).collect();
                format!("[{}]", entries.join(", "))
            }
            Value::Tuple(items) | Value::Set(items) => {
                let entries: Vec<String> = items.iter().map(|v| v.to_json_string()).collect();
                format!("[{}]", entries.join(", "))
            }
            Value::String(s) => escape_json_string(s),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => format!("{}", n),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::ResultOk(v) => format!("{{ \"Ok\": {} }}", v.to_json_string()),
            Value::ResultErr(v) => format!("{{ \"Err\": {} }}", v.to_json_string()),
            Value::Some(v) => v.to_json_string(),
            Value::None => "null".to_string(),
            Value::Frozen(inner) => inner.to_json_string(),
            _ => format!("\"<{}>\"", self.type_name()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Array(items) => {
                let strs: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                write!(f, "[{}]", strs.join(", "))
            }
            Value::Tuple(items) => {
                let strs: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                write!(f, "({})", strs.join(", "))
            }
            Value::Set(items) => {
                let strs: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                write!(f, "set({})", strs.join(", "))
            }
            Value::Object(_) => write!(f, "{}", self.to_json_string()),
            Value::Function { name, .. } => write!(f, "<fn {}>", name),
            Value::Lambda { .. } => write!(f, "<lambda>"),
            Value::ResultOk(v) => write!(f, "Ok({})", v),
            Value::ResultErr(v) => write!(f, "Err({})", v),
            Value::Some(v) => write!(f, "Some({})", v),
            Value::None => write!(f, "None"),
            Value::BuiltIn(name) => write!(f, "<builtin {}>", name),
            Value::TaskHandle(_) => write!(f, "<task>"),
            Value::Channel(_) => write!(f, "<channel>"),
            Value::Frozen(inner) => write!(f, "{}", inner),
        }
    }
}

/// Variable environment (scope chain) — uses Arc for O(1) cloning
#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<Arc<std::sync::Mutex<HashMap<String, Value>>>>,
    mutability: Vec<Arc<std::sync::Mutex<HashMap<String, bool>>>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            scopes: vec![Arc::new(std::sync::Mutex::new(HashMap::new()))],
            mutability: vec![Arc::new(std::sync::Mutex::new(HashMap::new()))],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes
            .push(Arc::new(std::sync::Mutex::new(HashMap::new())));
        self.mutability
            .push(Arc::new(std::sync::Mutex::new(HashMap::new())));
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
        self.mutability.pop();
    }

    pub fn define(&mut self, name: String, value: Value) {
        self.define_with_mutability(name, value, true);
    }

    pub fn define_with_mutability(&mut self, name: String, value: Value, mutable: bool) {
        // Use poison-recovery: if another thread panicked while holding the lock,
        // we still get a usable guard rather than propagating the panic.
        if let Some(scope) = self.scopes.last() {
            scope
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(name.clone(), value);
        }
        if let Some(muts) = self.mutability.last() {
            muts.lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(name, mutable);
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            let guard = scope.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(val) = guard.get(name) {
                return Some(val.clone());
            }
        }
        None
    }

    fn is_mutable(&self, name: &str) -> Option<bool> {
        for muts in self.mutability.iter().rev() {
            let guard = muts.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(m) = guard.get(name) {
                return Some(*m);
            }
        }
        None
    }

    pub fn set(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        if let Some(false) = self.is_mutable(name) {
            return Err(RuntimeError::new(&format!(
                "cannot reassign immutable variable '{}' (use 'let mut' to make it mutable)",
                name
            )));
        }
        for scope in self.scopes.iter().rev() {
            let mut guard = scope.lock().unwrap_or_else(|p| p.into_inner());
            if guard.contains_key(name) {
                guard.insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(RuntimeError::new(&format!("undefined variable: {}", name)))
    }

    /// Collect all defined variable names across all scopes (for REPL tab completion).
    pub fn all_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for scope in &self.scopes {
            let guard = scope.lock().unwrap_or_else(|p| p.into_inner());
            names.extend(guard.keys().cloned());
        }
        names.sort();
        names.dedup();
        names
    }

    /// Deep clone for spawn — breaks sharing so thread gets independent copy
    pub fn deep_clone(&self) -> Self {
        Self {
            scopes: self
                .scopes
                .iter()
                .map(|s| {
                    Arc::new(std::sync::Mutex::new(
                        s.lock().unwrap_or_else(|p| p.into_inner()).clone(),
                    ))
                })
                .collect(),
            mutability: self
                .mutability
                .iter()
                .map(|m| {
                    Arc::new(std::sync::Mutex::new(
                        m.lock().unwrap_or_else(|p| p.into_inner()).clone(),
                    ))
                })
                .collect(),
        }
    }

    pub fn suggest_similar(&self, name: &str) -> Option<String> {
        let mut best: Option<(String, usize)> = None;
        for scope in &self.scopes {
            let guard = scope.lock().unwrap_or_else(|p| p.into_inner());
            for key in guard.keys() {
                let dist = levenshtein(name, key);
                if dist <= 2 && dist < name.len() {
                    match &best {
                        Some((_, d)) if dist < *d => best = Some((key.clone(), dist)),
                        None => best = Some((key.clone(), dist)),
                        _ => {}
                    }
                }
            }
        }
        best.map(|(s, _)| s)
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut matrix = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        matrix[i][0] = i;
    }
    for j in 0..=b.len() {
        matrix[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }
    matrix[a.len()][b.len()]
}

/// Control flow signals
enum Signal {
    None,
    Return(Value),
    ImplicitReturn(Value),
    Break,
    Continue,
}

const MAX_CALL_DEPTH: usize = 512;

/// Debug action requested by the DAP client
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DebugAction {
    Continue,
    StepOver,
    StepIn,
    StepOut,
    Pause,
}

/// Debug frame for stack trace reporting
#[derive(Clone)]
#[allow(dead_code)]
pub struct DebugFrame {
    pub name: String,
    pub line: usize,
    pub col: usize,
}

/// Shared state between DAP server and interpreter
pub struct DebugState {
    pub breakpoints: Mutex<std::collections::HashMap<String, std::collections::HashSet<usize>>>,
    pub action: Mutex<DebugAction>,
    pub step_depth: Mutex<usize>,
    /// Interpreter signals it has paused (sends current line)
    pub paused_sender: std::sync::mpsc::Sender<usize>,
    /// DAP server signals interpreter to resume
    pub resume: (Mutex<bool>, std::sync::Condvar),
    /// Snapshot of variables at last pause point (written by interpreter, read by DAP)
    pub variables: Mutex<Vec<(String, String)>>,
    /// Snapshot of call stack at last pause point
    pub call_frames: Mutex<Vec<DebugFrame>>,
    /// Current call depth at last pause point
    pub paused_depth: Mutex<usize>,
}

/// The interpreter
pub struct Interpreter {
    pub env: Environment,
    call_depth: usize,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    defer_host_runtime: bool,
    /// Instance methods: type_name -> { method_name -> Value::Function }
    pub method_tables: HashMap<String, IndexMap<String, Value>>,
    /// Static methods: type_name -> { method_name -> Value::Function }
    pub static_methods: HashMap<String, IndexMap<String, Value>>,
    /// Embedded fields: type_name -> [(field_name, embedded_type_name)]
    pub embedded_fields: HashMap<String, Vec<(String, String)>>,
    /// Struct defaults: type_name -> { field_name -> default_value }
    pub struct_defaults: HashMap<String, IndexMap<String, Value>>,
    /// Current source line number (set during run())
    pub current_line: usize,
    /// Source code (for error display)
    pub source: Option<String>,
    /// Path of the currently executing source file (for relative imports)
    pub source_file: Option<std::path::PathBuf>,
    /// Coverage tracking: set of executed line numbers (when enabled)
    pub coverage: Option<std::collections::HashSet<usize>>,
    /// Debug state for DAP debugger (breakpoints, stepping, pause control)
    pub debug_state: Option<Arc<DebugState>>,
    /// Output sink: when set, print/say/yell/whisper write here instead of stdout
    pub output_sink: Option<Arc<Mutex<Vec<String>>>>,
    /// Call stack frames for debugger stack traces
    pub call_stack: Vec<DebugFrame>,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
            env: Environment::new(),
            call_depth: 0,
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            defer_host_runtime: false,
            method_tables: HashMap::new(),
            static_methods: HashMap::new(),
            embedded_fields: HashMap::new(),
            struct_defaults: HashMap::new(),
            current_line: 0,
            source: None,
            source_file: None,
            coverage: None,
            debug_state: None,
            output_sink: None,
            call_stack: Vec::new(),
        };
        interp.register_builtins();
        interp
    }

    pub(crate) fn set_defer_host_runtime(&mut self, defer: bool) {
        self.defer_host_runtime = defer;
    }

    pub(crate) fn fork_for_background_runtime(&self) -> Self {
        let mut interp = Interpreter::new();
        interp.env = self.env.clone();
        interp.method_tables = self.method_tables.clone();
        interp.static_methods = self.static_methods.clone();
        interp.embedded_fields = self.embedded_fields.clone();
        interp.struct_defaults = self.struct_defaults.clone();
        interp.current_line = self.current_line;
        interp.source = self.source.clone();
        interp.source_file = self.source_file.clone();
        interp.debug_state = self.debug_state.clone();
        interp.output_sink = self.output_sink.clone();
        interp
    }

    pub(crate) fn exec_background_block(
        &mut self,
        stmts: &[SpannedStmt],
    ) -> Result<(), RuntimeError> {
        let _ = self.exec_block(stmts)?;
        Ok(())
    }

    fn register_builtins(&mut self) {
        self.env
            .define("math".to_string(), crate::stdlib::create_math_module());
        self.env
            .define("fs".to_string(), crate::stdlib::create_fs_module());
        self.env
            .define("io".to_string(), crate::stdlib::create_io_module());
        self.env
            .define("crypto".to_string(), crate::stdlib::create_crypto_module());
        self.env
            .define("db".to_string(), crate::stdlib::create_db_module());
        self.env
            .define("env".to_string(), crate::stdlib::create_env_module());
        self.env
            .define("json".to_string(), crate::stdlib::create_json_module());
        self.env
            .define("regex".to_string(), crate::stdlib::create_regex_module());
        self.env
            .define("log".to_string(), crate::stdlib::create_log_module());
        #[cfg(feature = "postgres")]
        self.env
            .define("pg".to_string(), crate::stdlib::create_pg_module());
        self.env
            .define("term".to_string(), crate::stdlib::create_term_module());
        self.env
            .define("http".to_string(), crate::stdlib::create_http_module());
        self.env
            .define("csv".to_string(), crate::stdlib::create_csv_module());
        self.env
            .define("time".to_string(), crate::stdlib::create_time_module());
        self.env
            .define("npc".to_string(), crate::stdlib::create_npc_module());
        self.env
            .define("url".to_string(), crate::stdlib::create_url_module());
        self.env
            .define("toml".to_string(), crate::stdlib::create_toml_module());
        self.env
            .define("ws".to_string(), crate::stdlib::create_ws_module());
        self.env
            .define("jwt".to_string(), crate::stdlib::create_jwt_module());
        self.env
            .define("os".to_string(), crate::stdlib::create_os_module());
        self.env
            .define("path".to_string(), crate::stdlib::create_path_module());
        #[cfg(feature = "mysql")]
        self.env
            .define("mysql".to_string(), crate::stdlib::create_mysql_module());

        // Prelude: Option type = Some(value) | None
        self.env
            .define("Some".to_string(), Value::BuiltIn("Some".to_string()));
        self.env.define("None".to_string(), Value::None);
        self.env.define("null".to_string(), Value::Null);
        {
            let mut type_meta = IndexMap::new();
            type_meta.insert("__kind__".to_string(), Value::String("type".to_string()));
            type_meta.insert("name".to_string(), Value::String("Option".to_string()));
            type_meta.insert(
                "variants".to_string(),
                Value::Array(vec![
                    Value::String("Some".to_string()),
                    Value::String("None".to_string()),
                ]),
            );
            self.env
                .define("__type_Option__".to_string(), Value::Object(type_meta));
        }

        for name in &[
            "print",
            "println",
            "len",
            "type",
            "typeof",
            "str",
            "int",
            "float",
            "push",
            "pop",
            "keys",
            "values",
            "contains",
            "has_key",
            "get",
            "pick",
            "omit",
            "merge",
            "find",
            "flat_map",
            "entries",
            "from_entries",
            "range",
            "set",
            "enumerate",
            "map",
            "filter",
            "Ok",
            "ok",
            "Err",
            "err",
            "is_ok",
            "is_err",
            "unwrap",
            "unwrap_or",
            "unwrap_err",
            "fetch",
            "uuid",
            "say",
            "yell",
            "whisper",
            "wait",
            "channel",
            "send",
            "receive",
            "is_some",
            "is_none",
            "satisfies",
            "assert",
            "assert_eq",
            "exit",
            "run_command",
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
            "input",
            "reduce",
            "sort",
            "reverse",
            "split",
            "join",
            "replace",
            "starts_with",
            "ends_with",
            "substring",
            "index_of",
            "last_index_of",
            "pad_start",
            "pad_end",
            "capitalize",
            "title",
            "repeat_str",
            "count",
            "sum",
            "min_of",
            "max_of",
            "any",
            "all",
            "unique",
            "zip",
            "flatten",
            "group_by",
            "chunk",
            "slice",
            "assert_ne",
            "assert_throws",
            "try_send",
            "try_receive",
            "select",
            "close",
            "await_all",
            "await_timeout",
            // GenZ Debug Kit
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
            // String utils
            "slugify",
            "snake_case",
            "camel_case",
            // Array utils
            "sample",
            "shuffle",
            "partition",
            "diff",
            // New collection utils
            "sort_by",
            "first",
            "last",
            "compact",
            "take_n",
            "skip",
            "frequencies",
            "for_each",
        ] {
            self.env
                .define(name.to_string(), Value::BuiltIn(name.to_string()));
        }
    }

    pub fn run(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        for spanned in &program.statements {
            self.current_line = spanned.line;
            if let Some(ref mut cov) = self.coverage {
                if spanned.line > 0 {
                    cov.insert(spanned.line);
                }
            }
            match self.exec_stmt(&spanned.stmt) {
                Ok(signal) => match signal {
                    Signal::Return(v) => return Ok(v),
                    Signal::Break => return Err(RuntimeError::new("break outside of loop")),
                    Signal::Continue => return Err(RuntimeError::new("continue outside of loop")),
                    Signal::None | Signal::ImplicitReturn(_) => {}
                },
                Err(mut e) => {
                    if e.line == 0 {
                        e.line = spanned.line;
                        e.col = spanned.col;
                    }
                    return Err(e);
                }
            }
        }
        Ok(Value::Null)
    }

    /// Run in REPL mode — returns the value of the last expression for display
    pub fn run_repl(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        let mut last = Value::Null;
        for spanned in &program.statements {
            self.current_line = spanned.line;
            match self.exec_stmt(&spanned.stmt).map_err(|mut e| {
                if e.line == 0 {
                    e.line = spanned.line;
                    e.col = spanned.col;
                }
                e
            })? {
                Signal::Return(v) => return Ok(v),
                Signal::Break => return Err(RuntimeError::new("break outside of loop")),
                Signal::Continue => return Err(RuntimeError::new("continue outside of loop")),
                Signal::None | Signal::ImplicitReturn(_) => {}
            }
            if let Stmt::Expression(ref expr) = spanned.stmt {
                match expr {
                    Expr::Call { function, .. } => {
                        if let Expr::Ident(name) = function.as_ref() {
                            let is_output = matches!(
                                name.as_str(),
                                "print" | "println" | "say" | "yell" | "whisper"
                            );
                            if is_output {
                                continue;
                            }
                        }
                        last = self.eval_expr(expr)?;
                    }
                    _ => {
                        last = self.eval_expr(expr)?;
                    }
                }
            }
        }
        Ok(last)
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<Signal, RuntimeError> {
        // Cooperative cancellation check (used by timeout blocks)
        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(RuntimeError::new("cancelled"));
        }
        match stmt {
            Stmt::Let {
                name,
                mutable,
                value,
                ..
            } => {
                let val = self.eval_expr(value)?;
                self.env.define_with_mutability(name.clone(), val, *mutable);
                Ok(Signal::None)
            }

            Stmt::Assign { target, value } => {
                let val = self.eval_expr(value)?;
                match target {
                    Expr::Ident(name) => self.env.set(name, val)?,
                    Expr::FieldAccess { object, field } => {
                        let name = if let Expr::Ident(n) = object.as_ref() {
                            n.clone()
                        } else {
                            return Err(RuntimeError::new("can only assign to variable fields"));
                        };
                        let obj = self
                            .env
                            .get(&name)
                            .ok_or_else(|| RuntimeError::new(&format!("undefined: {}", name)))?;
                        if obj.is_frozen() {
                            return Err(RuntimeError::new(&format!(
                                "cannot modify frozen value '{}': field '{}'",
                                name, field
                            )));
                        }
                        let mut obj = obj;
                        if let Value::Object(ref mut map) = obj {
                            map.insert(field.clone(), val);
                        }
                        self.env.set(&name, obj)?;
                    }
                    Expr::Index { object, index } => {
                        let name = if let Expr::Ident(n) = object.as_ref() {
                            n.clone()
                        } else {
                            return Err(RuntimeError::new("can only assign to variable indices"));
                        };
                        let idx = self.eval_expr(index)?;
                        let existing = self
                            .env
                            .get(&name)
                            .ok_or_else(|| RuntimeError::new(&format!("undefined: {}", name)))?;
                        if existing.is_frozen() {
                            return Err(RuntimeError::new(&format!(
                                "cannot modify frozen value '{}': index assignment",
                                name
                            )));
                        }
                        if matches!(existing, Value::Tuple(_)) {
                            return Err(RuntimeError::new("cannot mutate a tuple"));
                        }
                        if matches!(existing, Value::Set(_)) {
                            return Err(RuntimeError::new(
                                "cannot index-assign a set; use .add() and .remove()",
                            ));
                        }
                        let mut arr = existing;
                        if let (Value::Array(ref mut items), Value::Int(i)) = (&mut arr, &idx) {
                            let i = *i as usize;
                            if i < items.len() {
                                items[i] = val;
                            } else {
                                return Err(RuntimeError::new("index out of bounds"));
                            }
                        }
                        self.env.set(&name, arr)?;
                    }
                    _ => return Err(RuntimeError::new("invalid assignment target")),
                }
                Ok(Signal::None)
            }

            Stmt::FnDef {
                name,
                params,
                body,
                decorators,
                ..
            } => {
                let func = Value::Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                    closure: self.env.clone(),
                    decorators: decorators.clone(),
                };
                self.env.define(name.clone(), func.clone());
                if let Value::Function {
                    name: n,
                    params: p,
                    body: b,
                    decorators: d,
                    ..
                } = func
                {
                    let recursive_func = Value::Function {
                        name: n,
                        params: p,
                        body: b,
                        closure: self.env.clone(),
                        decorators: d,
                    };
                    self.env.define(name.clone(), recursive_func);
                }
                Ok(Signal::None)
            }

            Stmt::StructDef { name, fields, .. } => {
                self.env
                    .define(name.clone(), Value::BuiltIn(format!("struct:{}", name)));

                // Record embedded fields and defaults
                let mut embeds = Vec::new();
                let mut defaults = IndexMap::new();
                for field in fields {
                    if field.embedded {
                        if let TypeAnn::Simple(ref type_name) = field.type_ann {
                            embeds.push((field.name.clone(), type_name.clone()));
                        }
                    }
                    if let Some(ref default_expr) = field.default {
                        let default_val = self.eval_expr(default_expr)?;
                        defaults.insert(field.name.clone(), default_val);
                    }
                }
                if !embeds.is_empty() {
                    self.embedded_fields.insert(name.clone(), embeds);
                }
                if !defaults.is_empty() {
                    self.struct_defaults.insert(name.clone(), defaults);
                }

                Ok(Signal::None)
            }

            Stmt::TypeDef { name, variants } => {
                let mut variant_names = Vec::new();
                for variant in variants {
                    variant_names.push(variant.name.clone());
                    let type_name = name.clone();
                    let var_name = variant.name.clone();
                    let field_count = variant.fields.len();

                    if field_count == 0 {
                        let mut obj = IndexMap::new();
                        obj.insert("__type__".to_string(), Value::String(type_name));
                        obj.insert("__variant__".to_string(), Value::String(var_name.clone()));
                        self.env.define(var_name, Value::Object(obj));
                    } else {
                        self.env.define(
                            var_name,
                            Value::BuiltIn(format!(
                                "adt:{}:{}:{}",
                                type_name, variant.name, field_count
                            )),
                        );
                    }
                }
                let mut type_meta = IndexMap::new();
                type_meta.insert("__kind__".to_string(), Value::String("type".to_string()));
                type_meta.insert("name".to_string(), Value::String(name.clone()));
                type_meta.insert(
                    "variants".to_string(),
                    Value::Array(variant_names.into_iter().map(Value::String).collect()),
                );
                self.env
                    .define(format!("__type_{}__", name), Value::Object(type_meta));
                Ok(Signal::None)
            }

            Stmt::InterfaceDef { name, methods } => {
                let mut method_list = Vec::new();
                for method in methods {
                    let mut m = IndexMap::new();
                    m.insert("name".to_string(), Value::String(method.name.clone()));
                    m.insert(
                        "param_count".to_string(),
                        Value::Int(method.params.len() as i64),
                    );
                    if let Some(ref rt) = method.return_type {
                        m.insert(
                            "return_type".to_string(),
                            Value::String(format!("{:?}", rt)),
                        );
                    }
                    method_list.push(Value::Object(m));
                }
                let mut iface = IndexMap::new();
                iface.insert(
                    "__kind__".to_string(),
                    Value::String("interface".to_string()),
                );
                iface.insert("name".to_string(), Value::String(name.clone()));
                iface.insert("methods".to_string(), Value::Array(method_list));
                self.env.define(name.clone(), Value::Object(iface.clone()));
                self.env
                    .define(format!("__interface_{}__", name), Value::Object(iface));
                Ok(Signal::None)
            }

            Stmt::ImplBlock {
                type_name,
                ability,
                methods,
            } => {
                // Phase 4 will fully implement method tables + dispatch
                // For now, register each method function in the environment
                for method_spanned in methods {
                    if let Stmt::FnDef {
                        name: method_name,
                        params,
                        return_type: _,
                        body,
                        is_async: _,
                        ..
                    } = &method_spanned.stmt
                    {
                        let has_receiver = params.first().map_or(false, |p| p.name == "it");
                        let qualified_name = if has_receiver {
                            // Instance method: stored for dispatch
                            format!("{}::{}", type_name, method_name)
                        } else {
                            // Static method: accessible as Type.method()
                            format!("{}::{}", type_name, method_name)
                        };

                        let func_val = Value::Function {
                            name: qualified_name.clone(),
                            params: params.clone(),
                            body: body.clone(),
                            closure: self.env.clone(),
                            decorators: Vec::new(),
                        };

                        // Register in method tables
                        let type_methods = self
                            .method_tables
                            .entry(type_name.clone())
                            .or_insert_with(IndexMap::new);
                        type_methods.insert(method_name.clone(), func_val.clone());

                        if !has_receiver {
                            let type_statics = self
                                .static_methods
                                .entry(type_name.clone())
                                .or_insert_with(IndexMap::new);
                            type_statics.insert(method_name.clone(), func_val);
                        }
                    }
                }

                // If ability specified, validate all required methods are present
                if let Some(ref ability_name) = ability {
                    let iface_key = format!("__interface_{}__", ability_name);
                    if let Some(Value::Object(iface)) = self.env.get(&iface_key) {
                        if let Some(Value::Array(required_methods)) = iface.get("methods") {
                            let type_methods = self.method_tables.get(type_name);
                            for req in required_methods {
                                if let Value::Object(m) = req {
                                    if let Some(Value::String(mname)) = m.get("name") {
                                        let has_it =
                                            type_methods.map_or(false, |tm| tm.contains_key(mname));
                                        if !has_it {
                                            return Err(RuntimeError::new(&format!(
                                                "'{}' does not implement '{}' required by power '{}'",
                                                type_name, mname, ability_name
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(Signal::None)
            }

            Stmt::Return(expr) => {
                let val = match expr {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Null,
                };
                Ok(Signal::Return(val))
            }

            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let cond = self.eval_expr(condition)?;
                if cond.is_truthy() {
                    self.exec_block(then_body)
                } else if let Some(else_b) = else_body {
                    self.exec_block(else_b)
                } else {
                    Ok(Signal::None)
                }
            }

            Stmt::Match { subject, arms } => {
                let val = self.eval_expr(subject)?;

                // Check exhaustiveness for ADT values
                if let Value::Object(ref obj) = val {
                    if let Some(Value::String(type_name)) = obj.get("__type__") {
                        let type_key = format!("__type_{}__", type_name);
                        if let Some(Value::Object(type_meta)) = self.env.get(&type_key) {
                            if let Some(Value::Array(variant_list)) = type_meta.get("variants") {
                                let has_wildcard =
                                    arms.iter().any(|a| matches!(a.pattern, Pattern::Wildcard));
                                let variant_names: Vec<&str> = variant_list
                                    .iter()
                                    .filter_map(|v| {
                                        if let Value::String(s) = v {
                                            Some(s.as_str())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                let has_true_catchall = arms.iter().any(|a| {
                                    if let Pattern::Binding(bname) = &a.pattern {
                                        !variant_names.contains(&bname.as_str())
                                    } else {
                                        false
                                    }
                                });
                                if !has_wildcard && !has_true_catchall {
                                    for vname in &variant_names {
                                        let covered = arms.iter().any(|a| match &a.pattern {
                                            Pattern::Constructor { name, .. } => name == vname,
                                            Pattern::Binding(bname) => bname == vname,
                                            _ => false,
                                        });
                                        if !covered {
                                            return Err(RuntimeError::new(&format!(
                                                "non-exhaustive match: missing variant '{}'",
                                                vname
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                for arm in arms {
                    if self.match_pattern(&arm.pattern, &val) {
                        self.env.push_scope();
                        self.bind_pattern(&arm.pattern, &val);
                        let result = self.exec_block(&arm.body);
                        self.env.pop_scope();
                        return result;
                    }
                }
                Err(RuntimeError::new("non-exhaustive match"))
            }

            Stmt::For {
                var,
                var2,
                iterable,
                body,
            } => {
                let iter_val = self.eval_expr(iterable)?;
                match iter_val {
                    Value::Array(items) | Value::Tuple(items) | Value::Set(items) => {
                        for item in items {
                            self.env.push_scope();
                            self.env.define(var.clone(), item);
                            match self.exec_block(body)? {
                                Signal::Break => {
                                    self.env.pop_scope();
                                    break;
                                }
                                Signal::Continue => {
                                    self.env.pop_scope();
                                    continue;
                                }
                                Signal::Return(v) => {
                                    self.env.pop_scope();
                                    return Ok(Signal::Return(v));
                                }
                                Signal::None | Signal::ImplicitReturn(_) => {
                                    self.env.pop_scope();
                                }
                            }
                        }
                    }
                    Value::Object(map) => {
                        for (key, val) in map {
                            self.env.push_scope();
                            self.env.define(var.clone(), Value::String(key));
                            if let Some(v2) = var2 {
                                self.env.define(v2.clone(), val);
                            }
                            match self.exec_block(body)? {
                                Signal::Break => {
                                    self.env.pop_scope();
                                    break;
                                }
                                Signal::Continue => {
                                    self.env.pop_scope();
                                    continue;
                                }
                                Signal::Return(v) => {
                                    self.env.pop_scope();
                                    return Ok(Signal::Return(v));
                                }
                                Signal::None | Signal::ImplicitReturn(_) => {
                                    self.env.pop_scope();
                                }
                            }
                        }
                    }
                    Value::Channel(ch) => loop {
                        let val = {
                            let rx_guard = ch.rx.lock().expect("BUG: channel mutex poisoned");
                            match rx_guard.as_ref() {
                                Some(rx) => match rx.recv() {
                                    Ok(v) => v,
                                    Err(_) => break,
                                },
                                None => break,
                            }
                        };
                        self.env.push_scope();
                        self.env.define(var.clone(), val);
                        match self.exec_block(body)? {
                            Signal::Break => {
                                self.env.pop_scope();
                                break;
                            }
                            Signal::Continue => {
                                self.env.pop_scope();
                                continue;
                            }
                            Signal::Return(v) => {
                                self.env.pop_scope();
                                return Ok(Signal::Return(v));
                            }
                            Signal::None | Signal::ImplicitReturn(_) => {
                                self.env.pop_scope();
                            }
                        }
                    },
                    _ => {
                        return Err(RuntimeError::new(
                            "can only iterate over arrays, objects, or channels",
                        ))
                    }
                }
                Ok(Signal::None)
            }

            Stmt::While { condition, body } => {
                loop {
                    let cond = self.eval_expr(condition)?;
                    if !cond.is_truthy() {
                        break;
                    }
                    match self.exec_block(body)? {
                        Signal::Break => break,
                        Signal::Continue => continue,
                        Signal::Return(v) => return Ok(Signal::Return(v)),
                        Signal::None | Signal::ImplicitReturn(_) => {}
                    }
                }
                Ok(Signal::None)
            }

            Stmt::Loop { body } => {
                loop {
                    match self.exec_block(body)? {
                        Signal::Break => break,
                        Signal::Continue => continue,
                        Signal::Return(v) => return Ok(Signal::Return(v)),
                        Signal::None | Signal::ImplicitReturn(_) => {}
                    }
                }
                Ok(Signal::None)
            }

            Stmt::Break => Ok(Signal::Break),
            Stmt::Continue => Ok(Signal::Continue),

            Stmt::Spawn { body } => {
                // Fire-and-forget spawn (backward compat — result is discarded)
                let _ = self.spawn_task(body)?;
                Ok(Signal::None)
            }

            Stmt::TryCatch {
                try_body,
                catch_var,
                catch_body,
            } => match self.exec_block(try_body) {
                Ok(signal) => Ok(signal),
                Err(e) => {
                    self.env.push_scope();
                    let mut err_obj = IndexMap::new();
                    err_obj.insert("message".to_string(), Value::String(e.message.clone()));
                    let error_type = if e.message.contains("type") || e.message.contains("Type") {
                        "TypeError"
                    } else if e.message.contains("division by zero") {
                        "ArithmeticError"
                    } else if e.message.contains("assertion") {
                        "AssertionError"
                    } else if e.message.contains("index") || e.message.contains("out of bounds") {
                        "IndexError"
                    } else if e.message.contains("not found") || e.message.contains("undefined") {
                        "ReferenceError"
                    } else if e.message.contains("immutable")
                        || e.message.contains("cannot reassign")
                    {
                        "TypeError"
                    } else {
                        "RuntimeError"
                    };
                    err_obj.insert("type".to_string(), Value::String(error_type.to_string()));
                    self.env.define(catch_var.clone(), Value::Object(err_obj));
                    // FIX: was `result.unwrap_or(Signal::None);` — the semicolon
                    // silently discarded errors from the catch body itself.
                    let catch_result = self.exec_block(catch_body);
                    self.env.pop_scope();
                    match catch_result {
                        Ok(sig) => Ok(sig),
                        Err(e) => Err(e), // propagate errors from catch body
                    }
                }
            },

            Stmt::Import { path, names } => {
                let builtin_modules = [
                    "math", "fs", "io", "crypto", "db", "pg", "env", "json", "regex", "log",
                    "term", "http", "csv", "exec", "time", "url", "toml", "npc", "ws", "jwt",
                    "mysql",
                ];
                if builtin_modules.contains(&path.as_str()) {
                    if self.env.get(path).is_some() {
                        return Ok(Signal::None);
                    }
                    return Err(RuntimeError::new(&format!(
                        "'{}' is a built-in module — it's already available. Use it directly: {}.function()",
                        path, path
                    )));
                }

                let base_dir = self
                    .source_file
                    .as_ref()
                    .and_then(|p| p.parent().map(|d| d.to_path_buf()));
                let file_path = match crate::package::resolve_import_from(path, base_dir.as_deref())
                {
                    Some(p) => p,
                    None => {
                        return Err(RuntimeError::new(&format!(
                            "cannot import '{}': file not found (checked {0}.fg, forge_modules/{0}/main.fg)",
                            path
                        )));
                    }
                };
                let source = std::fs::read_to_string(&file_path)
                    .map_err(|e| RuntimeError::new(&format!("cannot import '{}': {}", path, e)))?;
                let mut lexer = crate::lexer::Lexer::new(&source);
                let tokens = lexer.tokenize().map_err(|e| {
                    RuntimeError::new(&format!("import '{}' lex error: {}", path, e.message))
                })?;
                let mut parser = crate::parser::Parser::new(tokens);
                let program = parser.parse_program().map_err(|e| {
                    RuntimeError::new(&format!("import '{}' parse error: {}", path, e.message))
                })?;

                let mut import_interp = Interpreter::new();
                import_interp.source_file = Some(file_path.clone());
                import_interp.run(&program)?;

                if let Some(name_list) = names {
                    for name in name_list {
                        if let Some(val) = import_interp.env.get(name) {
                            self.env.define(name.to_string(), val);
                        }
                    }
                } else {
                    // Import all top-level definitions
                    for spanned in &program.statements {
                        match &spanned.stmt {
                            Stmt::FnDef { name, .. } | Stmt::Let { name, .. } => {
                                if let Some(val) = import_interp.env.get(name) {
                                    self.env.define(name.clone(), val);
                                }
                            }
                            Stmt::StructDef { name, .. } => {
                                if let Some(val) = import_interp.env.get(name) {
                                    self.env.define(name.clone(), val);
                                }
                                // Copy struct defaults and embedded fields
                                if let Some(defaults) = import_interp.struct_defaults.get(name) {
                                    self.struct_defaults.insert(name.clone(), defaults.clone());
                                }
                                if let Some(embeds) = import_interp.embedded_fields.get(name) {
                                    self.embedded_fields.insert(name.clone(), embeds.clone());
                                }
                            }
                            Stmt::TypeDef { name, variants } => {
                                // Import each variant individually
                                for variant in variants {
                                    if let Some(val) = import_interp.env.get(&variant.name) {
                                        self.env.define(variant.name.clone(), val);
                                    }
                                }
                                // Import type metadata
                                let meta_key = format!("__type_{}__", name);
                                if let Some(val) = import_interp.env.get(&meta_key) {
                                    self.env.define(meta_key, val);
                                }
                            }
                            Stmt::ImplBlock { type_name, .. } => {
                                // Copy method tables and static methods
                                if let Some(methods) = import_interp.method_tables.get(type_name) {
                                    let entry = self
                                        .method_tables
                                        .entry(type_name.clone())
                                        .or_insert_with(IndexMap::new);
                                    for (k, v) in methods {
                                        entry.insert(k.clone(), v.clone());
                                    }
                                }
                                if let Some(statics) = import_interp.static_methods.get(type_name) {
                                    let entry = self
                                        .static_methods
                                        .entry(type_name.clone())
                                        .or_insert_with(IndexMap::new);
                                    for (k, v) in statics {
                                        entry.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Signal::None)
            }

            Stmt::DecoratorStmt(dec) => {
                let name = format!("@{}", dec.name);
                let mut config = IndexMap::new();
                for arg in &dec.args {
                    match arg {
                        DecoratorArg::Named(key, expr) => {
                            let val = self.eval_expr(expr)?;
                            config.insert(key.clone(), val);
                        }
                        DecoratorArg::Positional(expr) => {
                            let val = self.eval_expr(expr)?;
                            config.insert(format!("_{}", config.len()), val);
                        }
                    }
                }
                self.env.define(name, Value::Object(config));
                Ok(Signal::None)
            }

            Stmt::Destructure { pattern, value } => {
                let val = self.eval_expr(value)?;
                match pattern {
                    DestructurePattern::Object(names) => {
                        if let Value::Object(map) = &val {
                            for name in names {
                                let v = map.get(name).cloned().unwrap_or(Value::Null);
                                self.env.define(name.clone(), v);
                            }
                        } else {
                            return Err(RuntimeError::new("cannot destructure non-object"));
                        }
                    }
                    DestructurePattern::Array { items, rest } => {
                        if let Value::Array(arr) = &val {
                            for (i, name) in items.iter().enumerate() {
                                let v = arr.get(i).cloned().unwrap_or(Value::Null);
                                self.env.define(name.clone(), v);
                            }
                            if let Some(rest_name) = rest {
                                let rest_items = if items.len() < arr.len() {
                                    arr[items.len()..].to_vec()
                                } else {
                                    Vec::new()
                                };
                                self.env.define(rest_name.clone(), Value::Array(rest_items));
                            }
                        } else {
                            return Err(RuntimeError::new("cannot destructure non-array"));
                        }
                    }
                    DestructurePattern::Tuple(names) => {
                        if let Value::Tuple(items) = &val {
                            for (i, name) in names.iter().enumerate() {
                                let v = items.get(i).cloned().unwrap_or(Value::Null);
                                self.env.define(name.clone(), v);
                            }
                        } else {
                            return Err(RuntimeError::new("cannot destructure non-tuple"));
                        }
                    }
                }
                Ok(Signal::None)
            }

            Stmt::YieldStmt(_expr) => Ok(Signal::None),

            Stmt::When { subject, arms } => {
                let val = self.eval_expr(subject)?;
                for arm in arms {
                    if arm.is_else {
                        let result = self.eval_expr(&arm.result)?;
                        return Ok(Signal::ImplicitReturn(result));
                    }
                    if let (Some(op), Some(cmp_val)) = (&arm.op, &arm.value) {
                        let cmp = self.eval_expr(cmp_val)?;
                        let matches = match (op, &val, &cmp) {
                            (BinOp::Lt, Value::Int(a), Value::Int(b)) => a < b,
                            (BinOp::Gt, Value::Int(a), Value::Int(b)) => a > b,
                            (BinOp::LtEq, Value::Int(a), Value::Int(b)) => a <= b,
                            (BinOp::GtEq, Value::Int(a), Value::Int(b)) => a >= b,
                            (BinOp::Eq, _, _) => format!("{}", val) == format!("{}", cmp),
                            (BinOp::NotEq, _, _) => format!("{}", val) != format!("{}", cmp),
                            (BinOp::Lt, Value::Float(a), Value::Float(b)) => a < b,
                            (BinOp::Gt, Value::Float(a), Value::Float(b)) => a > b,
                            (BinOp::LtEq, Value::Float(a), Value::Float(b)) => a <= b,
                            (BinOp::GtEq, Value::Float(a), Value::Float(b)) => a >= b,
                            (BinOp::Lt, Value::Int(a), Value::Float(b)) => (*a as f64) < *b,
                            (BinOp::Gt, Value::Int(a), Value::Float(b)) => (*a as f64) > *b,
                            (BinOp::Lt, Value::Float(a), Value::Int(b)) => *a < (*b as f64),
                            (BinOp::Gt, Value::Float(a), Value::Int(b)) => *a > (*b as f64),
                            _ => false,
                        };
                        if matches {
                            let result = self.eval_expr(&arm.result)?;
                            return Ok(Signal::ImplicitReturn(result));
                        }
                    }
                }
                Ok(Signal::None)
            }

            Stmt::CheckStmt { expr, check_kind } => {
                let val = self.eval_expr(expr)?;
                let valid = match check_kind {
                    CheckKind::IsNotEmpty => match &val {
                        Value::String(s) => !s.is_empty(),
                        Value::Array(a) => !a.is_empty(),
                        Value::Null => false,
                        _ => true,
                    },
                    CheckKind::Contains(needle_expr) => {
                        let needle = self.eval_expr(needle_expr)?;
                        match (&val, &needle) {
                            (Value::String(s), Value::String(n)) => s.contains(n.as_str()),
                            _ => false,
                        }
                    }
                    CheckKind::Between(lo_expr, hi_expr) => {
                        let lo = self.eval_expr(lo_expr)?;
                        let hi = self.eval_expr(hi_expr)?;
                        match (&val, &lo, &hi) {
                            (Value::Int(v), Value::Int(l), Value::Int(h)) => v >= l && v <= h,
                            (Value::Float(v), Value::Float(l), Value::Float(h)) => v >= l && v <= h,
                            _ => false,
                        }
                    }
                    CheckKind::IsTrue => val.is_truthy(),
                };
                if !valid {
                    return Err(RuntimeError::new(&format!(
                        "check failed: {} did not pass validation",
                        val
                    )));
                }
                Ok(Signal::None)
            }

            Stmt::SafeBlock { body } => match self.exec_block(body) {
                Ok(signal) => Ok(signal),
                Err(_) => Ok(Signal::ImplicitReturn(Value::Null)),
            },

            Stmt::TimeoutBlock { duration, body } => {
                let secs = match self.eval_expr(duration)? {
                    Value::Int(n) => n.max(0) as u64,
                    Value::Float(n) => n.max(0.0) as u64,
                    _ => 5,
                };
                let body = body.clone();
                let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let mut timeout_interp = Interpreter::new();
                timeout_interp.env = self.env.clone();
                timeout_interp.cancelled = cancel_flag.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let handle = std::thread::spawn(move || {
                    let result = timeout_interp.exec_block(&body);
                    let _ = tx.send(result);
                });
                match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
                    Ok(result) => {
                        let _ = handle.join();
                        result.map(|_| Signal::None)
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Signal cooperative cancellation
                        cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        // Give the thread a moment to notice and exit
                        let _ = handle.join();
                        Err(RuntimeError::new(&format!(
                            "timeout: operation exceeded {} second limit",
                            secs
                        )))
                    }
                    Err(_) => Err(RuntimeError::new("timeout: execution failed")),
                }
            }

            Stmt::RetryBlock { count, body } => {
                let max = match self.eval_expr(count)? {
                    Value::Int(n) => n as usize,
                    _ => 3,
                };
                let mut last_err = String::new();
                for attempt in 0..max {
                    match self.exec_block(body) {
                        Ok(signal) => return Ok(signal),
                        Err(e) => {
                            last_err = e.message.clone();
                            if attempt < max - 1 {
                                std::thread::sleep(std::time::Duration::from_millis(
                                    100 * (attempt as u64 + 1),
                                ));
                            }
                        }
                    }
                }
                Err(RuntimeError::new(&format!(
                    "retry failed after {} attempts: {}",
                    max, last_err
                )))
            }

            Stmt::ScheduleBlock {
                interval,
                unit,
                body,
            } => {
                if self.defer_host_runtime {
                    return Ok(Signal::None);
                }
                crate::runtime::host::spawn_schedule(
                    self,
                    &crate::runtime::metadata::SchedulePlan {
                        interval: interval.clone(),
                        unit: unit.clone(),
                        body: body.clone(),
                        line: self.current_line,
                    },
                )?;
                Ok(Signal::None)
            }

            Stmt::WatchBlock { path, body } => {
                if self.defer_host_runtime {
                    return Ok(Signal::None);
                }
                crate::runtime::host::spawn_watch(
                    self,
                    &crate::runtime::metadata::WatchPlan {
                        path: path.clone(),
                        body: body.clone(),
                        line: self.current_line,
                    },
                )?;
                Ok(Signal::None)
            }

            Stmt::PromptDef {
                name,
                params,
                system,
                user_template,
                ..
            } => {
                let sys = system.clone();
                let tmpl = user_template.clone();
                let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                self.env
                    .define(name.clone(), Value::BuiltIn(format!("prompt:{}", name)));
                let _ = (sys, tmpl, param_names);
                Ok(Signal::None)
            }

            Stmt::AgentDef { name, .. } => {
                self.env
                    .define(name.clone(), Value::BuiltIn(format!("agent:{}", name)));
                Ok(Signal::None)
            }

            Stmt::Expression(expr) => {
                self.eval_expr(expr)?;
                Ok(Signal::None)
            }
        }
    }

    fn exec_block(&mut self, stmts: &[SpannedStmt]) -> Result<Signal, RuntimeError> {
        self.env.push_scope();
        let result = self.exec_stmts(stmts);
        self.env.pop_scope();
        result
    }

    fn exec_stmts(&mut self, stmts: &[SpannedStmt]) -> Result<Signal, RuntimeError> {
        let mut result = Signal::None;
        let mut last_expr_value = Value::Null;
        for s in stmts {
            self.current_line = s.line;
            if let Some(ref mut cov) = self.coverage {
                if s.line > 0 {
                    cov.insert(s.line);
                }
            }
            if s.line > 0 {
                self.debug_check(s.line);
            }
            let stmt = &s.stmt;
            if let Stmt::Expression(expr) = stmt {
                last_expr_value = self.eval_expr(expr).map_err(|mut e| {
                    if e.line == 0 {
                        e.line = s.line;
                        e.col = s.col;
                    }
                    e
                })?;
                continue;
            }
            last_expr_value = Value::Null;
            result = self.exec_stmt(stmt).map_err(|mut e| {
                if e.line == 0 {
                    e.line = s.line;
                    e.col = s.col;
                }
                e
            })?;
            match &result {
                Signal::Return(_) | Signal::Break | Signal::Continue => break,
                Signal::None | Signal::ImplicitReturn(_) => {}
            }
        }
        match result {
            Signal::Return(_) | Signal::Break | Signal::Continue => Ok(result),
            Signal::None | Signal::ImplicitReturn(_) => Ok(Signal::ImplicitReturn(last_expr_value)),
        }
    }

    /// Check if the debugger should pause at this line
    fn debug_check(&mut self, line: usize) {
        let ds = match self.debug_state {
            Some(ref ds) => ds.clone(),
            None => return,
        };

        let should_stop = {
            let bps = ds.breakpoints.lock().unwrap_or_else(|e| e.into_inner());
            let action = *ds.action.lock().unwrap_or_else(|e| e.into_inner());
            let step_depth = *ds.step_depth.lock().unwrap_or_else(|e| e.into_inner());

            let has_breakpoint = if let Some(ref sf) = self.source_file {
                bps.get(sf.to_string_lossy().as_ref())
                    .map_or(false, |lines| lines.contains(&line))
            } else {
                bps.values().any(|lines| lines.contains(&line))
            };

            match action {
                DebugAction::Pause => true,
                DebugAction::StepOver => self.call_depth <= step_depth,
                DebugAction::StepIn => true,
                DebugAction::StepOut => self.call_depth < step_depth,
                DebugAction::Continue => has_breakpoint,
            }
        };

        if should_stop {
            // Snapshot state before pausing
            if let Ok(mut vars) = ds.variables.lock() {
                *vars = self.snapshot_user_variables();
            }
            if let Ok(mut frames) = ds.call_frames.lock() {
                *frames = self.call_stack.clone();
            }
            if let Ok(mut d) = ds.paused_depth.lock() {
                *d = self.call_depth;
            }

            // Notify DAP server we've paused
            let _ = ds.paused_sender.send(line);

            // Wait for resume signal
            let (lock, cvar) = &ds.resume;
            let mut resumed = lock.lock().unwrap_or_else(|e| e.into_inner());
            *resumed = false;
            while !*resumed {
                // Use timeout to keep cooperative cancellation alive
                let result = cvar
                    .wait_timeout(resumed, std::time::Duration::from_millis(50))
                    .unwrap_or_else(|e| e.into_inner());
                resumed = result.0;
                if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
            }
        }
    }

    /// Write output to sink (for DAP) or stdout
    pub fn write_output(&self, text: &str, newline: bool) {
        if let Some(ref sink) = self.output_sink {
            if let Ok(mut buf) = sink.lock() {
                if newline {
                    buf.push(format!("{}\n", text));
                } else {
                    buf.push(text.to_string());
                }
                return;
            }
        }
        if newline {
            println!("{}", text);
        } else {
            print!("{}", text);
        }
    }

    /// Snapshot user-defined variables (excludes modules, builtins, internal vars)
    pub fn snapshot_user_variables(&self) -> Vec<(String, String)> {
        self.env
            .all_names()
            .into_iter()
            .filter_map(|name| {
                if name.starts_with("__") {
                    return None;
                }
                let val = self.env.get(&name)?;
                match val {
                    Value::BuiltIn(_) | Value::Function { .. } | Value::Lambda { .. } => None,
                    Value::Object(ref map) if map.contains_key("__module__") => None,
                    _ => Some((name, format!("{}", val))),
                }
            })
            .collect()
    }

    // ========== Expression Evaluation ==========

    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(n) => Ok(Value::Float(*n)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::StringLit(s) => Ok(Value::String(s.clone())),

            Expr::StringInterp(parts) => {
                let mut result = String::new();
                for part in parts {
                    match part {
                        StringPart::Literal(s) => result.push_str(s),
                        StringPart::Expr(e) => {
                            let val = self.eval_expr(e)?;
                            result.push_str(&format!("{}", val));
                        }
                    }
                }
                Ok(Value::String(result))
            }

            Expr::Array(items) => {
                let mut result = Vec::new();
                for item in items {
                    if let Expr::Spread(inner) = item {
                        let val = self.eval_expr(inner)?;
                        if let Value::Array(arr) = val {
                            result.extend(arr);
                        } else {
                            result.push(val);
                        }
                    } else {
                        result.push(self.eval_expr(item)?);
                    }
                }
                Ok(Value::Array(result))
            }

            Expr::Tuple(items) => {
                let mut result = Vec::new();
                for item in items {
                    result.push(self.eval_expr(item)?);
                }
                Ok(Value::Tuple(result))
            }

            Expr::Object(fields) => {
                let mut map = IndexMap::new();
                for (key, expr) in fields {
                    map.insert(key.clone(), self.eval_expr(expr)?);
                }
                Ok(Value::Object(map))
            }

            Expr::Ident(name) => self.env.get(name).ok_or_else(|| {
                let suggestion = self.env.suggest_similar(name);
                let mut msg = format!("undefined variable: '{}'", name);
                if let Some(similar) = suggestion {
                    msg.push_str(&format!("\n  hint: did you mean '{}'?", similar));
                } else {
                    msg.push_str("\n  hint: make sure the variable is defined before use");
                }
                RuntimeError::new(&msg)
            }),

            Expr::BinOp { left, op, right } => {
                // Short-circuit && and ||
                if matches!(op, BinOp::And) {
                    let l = self.eval_expr(left)?;
                    return if !l.is_truthy() {
                        Ok(Value::Bool(false))
                    } else {
                        let r = self.eval_expr(right)?;
                        Ok(Value::Bool(r.is_truthy()))
                    };
                }
                if matches!(op, BinOp::Or) {
                    let l = self.eval_expr(left)?;
                    return if l.is_truthy() {
                        Ok(Value::Bool(true))
                    } else {
                        let r = self.eval_expr(right)?;
                        Ok(Value::Bool(r.is_truthy()))
                    };
                }
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                // Unwrap Frozen for binary operations
                let l_inner = match &l {
                    Value::Frozen(v) => v.as_ref(),
                    other => other,
                };
                let r_inner = match &r {
                    Value::Frozen(v) => v.as_ref(),
                    other => other,
                };
                self.eval_binop(l_inner, op, r_inner)
            }

            Expr::UnaryOp { op, operand } => {
                let val = self.eval_expr(operand)?;
                match op {
                    UnaryOp::Neg => match val {
                        Value::Int(n) => Ok(Value::Int(-n)),
                        Value::Float(n) => Ok(Value::Float(-n)),
                        _ => Err(RuntimeError::new("cannot negate non-number")),
                    },
                    UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
                }
            }

            Expr::FieldAccess { object, field } => {
                let obj = self.eval_expr(object)?;
                // Unwrap Frozen for read access
                let inner = match &obj {
                    Value::Frozen(v) => v.as_ref(),
                    other => other,
                };
                match inner {
                    Value::Object(map) => {
                        // Direct field access
                        if let Some(val) = map.get(field) {
                            return Ok(val.clone());
                        }
                        // Embedded field delegation: check embedded sub-objects
                        if let Some(Value::String(type_name)) = map.get("__type__") {
                            if let Some(embeds) = self.embedded_fields.get(type_name).cloned() {
                                for (embed_field, _embed_type) in &embeds {
                                    if let Some(Value::Object(sub)) = map.get(embed_field) {
                                        if let Some(val) = sub.get(field) {
                                            return Ok(val.clone());
                                        }
                                    }
                                }
                            }
                        }
                        Err(RuntimeError::new(&format!(
                            "no field '{}' on object",
                            field
                        )))
                    }
                    Value::String(s) => match field.as_str() {
                        "len" => Ok(Value::Int(s.chars().count() as i64)),
                        "upper" => Ok(Value::String(s.to_uppercase())),
                        "lower" => Ok(Value::String(s.to_lowercase())),
                        "trim" => Ok(Value::String(s.trim().to_string())),
                        "trim_start" => Ok(Value::String(s.trim_start().to_string())),
                        "trim_end" => Ok(Value::String(s.trim_end().to_string())),
                        "is_empty" => Ok(Value::Bool(s.is_empty())),
                        "is_numeric" => Ok(Value::Bool(
                            s.chars()
                                .all(|c| c.is_ascii_digit() || c == '.' || c == '-'),
                        )),
                        "is_alpha" => Ok(Value::Bool(
                            !s.is_empty() && s.chars().all(|c| c.is_alphabetic()),
                        )),
                        "is_alphanumeric" => Ok(Value::Bool(
                            !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()),
                        )),
                        "chars" => Ok(Value::Array(
                            s.chars().map(|c| Value::String(c.to_string())).collect(),
                        )),
                        "bytes" => Ok(Value::Array(
                            s.bytes().map(|b| Value::Int(b as i64)).collect(),
                        )),
                        "words" => Ok(Value::Array(
                            s.split_whitespace()
                                .map(|w| Value::String(w.to_string()))
                                .collect(),
                        )),
                        "lines" => Ok(Value::Array(
                            s.lines().map(|l| Value::String(l.to_string())).collect(),
                        )),
                        "reverse" => Ok(Value::String(s.chars().rev().collect())),
                        _ => Err(RuntimeError::new(&format!(
                            "no method '{}' on String",
                            field
                        ))),
                    },
                    Value::Array(items) => match field.as_str() {
                        "len" => Ok(Value::Int(items.len() as i64)),
                        _ => Err(RuntimeError::new(&format!(
                            "no method '{}' on Array",
                            field
                        ))),
                    },
                    Value::Tuple(items) => match field.as_str() {
                        "len" => Ok(Value::Int(items.len() as i64)),
                        _ => Err(RuntimeError::new(&format!(
                            "no method '{}' on Tuple",
                            field
                        ))),
                    },
                    Value::Set(items) => match field.as_str() {
                        "len" => Ok(Value::Int(items.len() as i64)),
                        _ => Err(RuntimeError::new(&format!("no method '{}' on Set", field))),
                    },
                    _ => Err(RuntimeError::new(&format!(
                        "cannot access field '{}' on {}",
                        field,
                        obj.type_name()
                    ))),
                }
            }

            Expr::Index { object, index } => {
                let obj = self.eval_expr(object)?;
                let idx = self.eval_expr(index)?;
                // Unwrap Frozen for read access
                let inner = match &obj {
                    Value::Frozen(v) => v.as_ref(),
                    other => other,
                };
                match (inner, &idx) {
                    (Value::Array(items) | Value::Tuple(items), Value::Int(i)) => {
                        // Support negative indices (Python-style: -1 = last)
                        let len = items.len() as i64;
                        let actual = if *i < 0 { len + i } else { *i };
                        if actual < 0 || actual >= len {
                            Err(RuntimeError::new(&format!(
                                "index out of bounds: index {} on {} of length {}",
                                i,
                                if matches!(inner, Value::Tuple(_)) {
                                    "tuple"
                                } else {
                                    "array"
                                },
                                len
                            )))
                        } else {
                            Ok(items[actual as usize].clone())
                        }
                    }
                    (Value::Object(map), Value::String(key)) => map
                        .get(key)
                        .cloned()
                        .ok_or_else(|| RuntimeError::new(&format!("key '{}' not found", key))),
                    _ => Err(RuntimeError::new("invalid index operation")),
                }
            }

            Expr::Call { function, args } => {
                // Method call: obj.method(args) -> method(obj, args)
                if let Expr::FieldAccess { object, field } = function.as_ref() {
                    // In-place mutation for arr.push(x) / arr.pop() on mutable variables
                    if let Expr::Ident(var_name) = object.as_ref() {
                        if self.env.is_mutable(var_name) == Some(true) {
                            if field == "push" && args.len() == 1 {
                                let arr = self.eval_expr(object)?;
                                let val = self.eval_expr(&args[0])?;
                                if let Value::Array(mut items) = arr {
                                    items.push(val);
                                    let new_arr = Value::Array(items);
                                    self.env.set(var_name, new_arr.clone())?;
                                    return Ok(new_arr);
                                }
                                return Err(RuntimeError::new(
                                    "push() first argument must be array",
                                ));
                            }
                            if field == "pop" && args.is_empty() {
                                let arr = self.eval_expr(object)?;
                                if let Value::Array(mut items) = arr {
                                    let popped = items.pop().unwrap_or(Value::Null);
                                    self.env.set(var_name, Value::Array(items))?;
                                    return Ok(popped);
                                }
                                return Err(RuntimeError::new("pop() requires array"));
                            }
                            // In-place set mutation: s.add(x) / s.remove(x) on a mutable set variable.
                            // Peel Frozen so we can produce a useful error rather than a silent no-op.
                            if field == "add" && args.len() == 1 {
                                let s = self.eval_expr(object)?;
                                let raw = match s {
                                    Value::Frozen(_) => {
                                        return Err(RuntimeError::new("cannot add to a frozen set"))
                                    }
                                    other => other,
                                };
                                if let Value::Set(mut items) = raw {
                                    let val = self.eval_expr(&args[0])?;
                                    if !items.iter().any(|v| Value::container_eq(v, &val)) {
                                        items.push(val);
                                    }
                                    let new_set = Value::Set(items);
                                    self.env.set(var_name, new_set.clone())?;
                                    return Ok(new_set);
                                }
                            }
                            if field == "remove" && args.len() == 1 {
                                let s = self.eval_expr(object)?;
                                let raw = match s {
                                    Value::Frozen(_) => {
                                        return Err(RuntimeError::new(
                                            "cannot remove from a frozen set",
                                        ))
                                    }
                                    other => other,
                                };
                                if let Value::Set(items) = raw {
                                    let val = self.eval_expr(&args[0])?;
                                    let filtered: Vec<Value> = items
                                        .into_iter()
                                        .filter(|v| !Value::container_eq(v, &val))
                                        .collect();
                                    let new_set = Value::Set(filtered);
                                    self.env.set(var_name, new_set.clone())?;
                                    return Ok(new_set);
                                }
                            }
                        }
                    }
                    let obj = self.eval_expr(object)?;
                    let method_name = field.as_str();
                    let known_methods = [
                        "map",
                        "filter",
                        "reduce",
                        "sort",
                        "reverse",
                        "push",
                        "pop",
                        "len",
                        "contains",
                        "keys",
                        "values",
                        "enumerate",
                        "split",
                        "join",
                        "replace",
                        "find",
                        "flat_map",
                        "has_key",
                        "get",
                        "pick",
                        "omit",
                        "merge",
                        "entries",
                        "from_entries",
                        "starts_with",
                        "ends_with",
                        "upper",
                        "lower",
                        "trim",
                        "substring",
                        "index_of",
                        "last_index_of",
                        "pad_start",
                        "pad_end",
                        "capitalize",
                        "title",
                        "repeat_str",
                        "count",
                        "sum",
                        "min_of",
                        "max_of",
                        "any",
                        "all",
                        "unique",
                        "zip",
                        "flatten",
                        "group_by",
                        "chunk",
                        "slice",
                        "slugify",
                        "snake_case",
                        "camel_case",
                        "sample",
                        "shuffle",
                        "partition",
                        "diff",
                        // New string methods
                        "trim_start",
                        "trim_end",
                        "is_empty",
                        "is_numeric",
                        "is_alpha",
                        "is_alphanumeric",
                        "char_at",
                        "encode_uri",
                        "decode_uri",
                        "words",
                        "bytes",
                        // New collection methods
                        "sort_by",
                        "first",
                        "last",
                        "compact",
                        "take_n",
                        "skip",
                        "frequencies",
                        "for_each",
                    ];
                    let func = match &obj {
                        Value::Object(map) if map.get(field).is_some() => {
                            // Safety: guarded by `is_some()` above; use unwrap_or for defence-in-depth
                            map.get(field).cloned().unwrap_or(Value::Null)
                        }
                        // Static method call: Type.method(args)
                        Value::BuiltIn(ref tag) if tag.starts_with("struct:") => {
                            let type_name = tag[7..].to_string();
                            let func_opt = self
                                .static_methods
                                .get(&type_name)
                                .and_then(|s| s.get(method_name))
                                .cloned();
                            if let Some(func) = func_opt {
                                let eval_args: Result<Vec<Value>, _> =
                                    args.iter().map(|a| self.eval_expr(a)).collect();
                                return self.call_function(func, eval_args?);
                            }
                            return Err(RuntimeError::new(&format!(
                                "no static method '{}' on {}",
                                method_name, type_name
                            )));
                        }
                        // Instance method from give/impl block
                        Value::Object(map) if map.get("__type__").is_some() => {
                            let type_name = match map.get("__type__") {
                                Some(Value::String(t)) => t.clone(),
                                _ => String::new(),
                            };
                            // Clone func out before calling self methods
                            let func_opt = self
                                .method_tables
                                .get(&type_name)
                                .and_then(|m| m.get(method_name))
                                .cloned();
                            if let Some(func) = func_opt {
                                let mut full_args = vec![obj.clone()];
                                for arg in args {
                                    full_args.push(self.eval_expr(arg)?);
                                }
                                return self.call_function(func, full_args);
                            }
                            // Check embedded fields for delegation
                            let embeds = self.embedded_fields.get(&type_name).cloned();
                            if let Some(embedded) = embeds {
                                for (embed_field, embed_type) in &embedded {
                                    let efunc = self
                                        .method_tables
                                        .get(embed_type)
                                        .and_then(|m| m.get(method_name))
                                        .cloned();
                                    if let Some(func) = efunc {
                                        let embed_obj =
                                            map.get(embed_field).cloned().unwrap_or(Value::Null);
                                        let mut full_args = vec![embed_obj];
                                        for arg in args {
                                            full_args.push(self.eval_expr(arg)?);
                                        }
                                        return self.call_function(func, full_args);
                                    }
                                }
                            }
                            // Fall through to known_methods / error
                            if known_methods.contains(&method_name) {
                                let mut full_args = vec![obj.clone()];
                                for arg in args {
                                    full_args.push(self.eval_expr(arg)?);
                                }
                                if let Some(func) = self.env.get(method_name) {
                                    return self.call_function(func, full_args);
                                }
                            }
                            return Err(RuntimeError::new(&format!(
                                "no method '{}' on {}",
                                field, type_name
                            )));
                        }
                        Value::String(s)
                            if matches!(
                                method_name,
                                "upper"
                                    | "lower"
                                    | "trim"
                                    | "trim_start"
                                    | "trim_end"
                                    | "len"
                                    | "chars"
                                    | "bytes"
                                    | "words"
                                    | "is_empty"
                                    | "is_numeric"
                                    | "is_alpha"
                                    | "is_alphanumeric"
                                    | "reverse"
                                    | "char_at"
                                    | "encode_uri"
                                    | "decode_uri"
                            ) =>
                        {
                            match method_name {
                                "upper" => return Ok(Value::String(s.to_uppercase())),
                                "lower" => return Ok(Value::String(s.to_lowercase())),
                                "trim" => return Ok(Value::String(s.trim().to_string())),
                                "trim_start" => {
                                    return Ok(Value::String(s.trim_start().to_string()))
                                }
                                "trim_end" => return Ok(Value::String(s.trim_end().to_string())),
                                "len" => return Ok(Value::Int(s.chars().count() as i64)),
                                "is_empty" => return Ok(Value::Bool(s.is_empty())),
                                "is_numeric" => {
                                    return Ok(Value::Bool(
                                        s.chars()
                                            .all(|c| c.is_ascii_digit() || c == '.' || c == '-'),
                                    ))
                                }
                                "is_alpha" => {
                                    return Ok(Value::Bool(
                                        !s.is_empty() && s.chars().all(|c| c.is_alphabetic()),
                                    ))
                                }
                                "is_alphanumeric" => {
                                    return Ok(Value::Bool(
                                        !s.is_empty() && s.chars().all(|c| c.is_alphanumeric()),
                                    ))
                                }
                                "reverse" => return Ok(Value::String(s.chars().rev().collect())),
                                "chars" => {
                                    return Ok(Value::Array(
                                        s.chars().map(|c| Value::String(c.to_string())).collect(),
                                    ))
                                }
                                "bytes" => {
                                    return Ok(Value::Array(
                                        s.bytes().map(|b| Value::Int(b as i64)).collect(),
                                    ))
                                }
                                "words" => {
                                    return Ok(Value::Array(
                                        s.split_whitespace()
                                            .map(|w| Value::String(w.to_string()))
                                            .collect(),
                                    ))
                                }
                                "char_at" => {
                                    let idx = match args.first().map(|a| self.eval_expr(a)) {
                                        Some(Ok(Value::Int(i))) => i as usize,
                                        _ => {
                                            return Err(RuntimeError::new(
                                                "char_at() requires an integer index",
                                            ))
                                        }
                                    };
                                    return Ok(s
                                        .chars()
                                        .nth(idx)
                                        .map(|c| Value::String(c.to_string()))
                                        .unwrap_or(Value::Null));
                                }
                                "encode_uri" => {
                                    let encoded: String = s
                                        .chars()
                                        .map(|c| match c {
                                            'A'..='Z'
                                            | 'a'..='z'
                                            | '0'..='9'
                                            | '-'
                                            | '_'
                                            | '.'
                                            | '~' => c.to_string(),
                                            _ => format!("%{:02X}", c as u32),
                                        })
                                        .collect();
                                    return Ok(Value::String(encoded));
                                }
                                "decode_uri" => {
                                    let mut result = String::new();
                                    let mut chars = s.chars();
                                    while let Some(c) = chars.next() {
                                        if c == '%' {
                                            let hex: String = chars.by_ref().take(2).collect();
                                            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                                result.push(byte as char);
                                            } else {
                                                result.push('%');
                                                result.push_str(&hex);
                                            }
                                        } else if c == '+' {
                                            result.push(' ');
                                        } else {
                                            result.push(c);
                                        }
                                    }
                                    return Ok(Value::String(result));
                                }
                                _ => {}
                            }
                            return Ok(Value::Null);
                        }
                        _ if {
                            let is_set_receiver = matches!(&obj, Value::Set(_))
                                || matches!(&obj, Value::Frozen(ref inner) if matches!(inner.as_ref(), Value::Set(_)));
                            is_set_receiver
                                && matches!(
                                    method_name,
                                    "has"
                                        | "add"
                                        | "remove"
                                        | "union"
                                        | "intersect"
                                        | "diff"
                                        | "to_array"
                                )
                        } =>
                        {
                            let is_frozen = matches!(&obj, Value::Frozen(_));
                            let items: Vec<Value> = match &obj {
                                Value::Set(items) => items.clone(),
                                Value::Frozen(inner) => match inner.as_ref() {
                                    Value::Set(items) => items.clone(),
                                    _ => unreachable!(),
                                },
                                _ => unreachable!(),
                            };
                            // Extract the "other" argument for set-set operations, peeling Frozen.
                            let peel_other_set = |v: Value| -> Result<Vec<Value>, RuntimeError> {
                                match v {
                                    Value::Set(items) => Ok(items),
                                    Value::Frozen(inner) => match *inner {
                                        Value::Set(items) => Ok(items),
                                        _ => Err(RuntimeError::new(
                                            "set operation requires a set argument",
                                        )),
                                    },
                                    _ => Err(RuntimeError::new(
                                        "set operation requires a set argument",
                                    )),
                                }
                            };
                            match method_name {
                                "has" => {
                                    if args.len() != 1 {
                                        return Err(RuntimeError::new(
                                            "has() requires one argument",
                                        ));
                                    }
                                    let val = self.eval_expr(&args[0])?;
                                    return Ok(Value::Bool(
                                        items.iter().any(|v| Value::container_eq(v, &val)),
                                    ));
                                }
                                "add" => {
                                    if is_frozen {
                                        return Err(RuntimeError::new(
                                            "cannot add to a frozen set",
                                        ));
                                    }
                                    if args.len() != 1 {
                                        return Err(RuntimeError::new(
                                            "add() requires one argument",
                                        ));
                                    }
                                    let val = self.eval_expr(&args[0])?;
                                    let mut new_items = items;
                                    if !new_items.iter().any(|v| Value::container_eq(v, &val)) {
                                        new_items.push(val);
                                    }
                                    return Ok(Value::Set(new_items));
                                }
                                "remove" => {
                                    if is_frozen {
                                        return Err(RuntimeError::new(
                                            "cannot remove from a frozen set",
                                        ));
                                    }
                                    if args.len() != 1 {
                                        return Err(RuntimeError::new(
                                            "remove() requires one argument",
                                        ));
                                    }
                                    let val = self.eval_expr(&args[0])?;
                                    let new_items: Vec<Value> = items
                                        .into_iter()
                                        .filter(|v| !Value::container_eq(v, &val))
                                        .collect();
                                    return Ok(Value::Set(new_items));
                                }
                                "union" => {
                                    if args.len() != 1 {
                                        return Err(RuntimeError::new(
                                            "union() requires one argument",
                                        ));
                                    }
                                    let other = self.eval_expr(&args[0])?;
                                    let other_items = peel_other_set(other)?;
                                    let mut result = items;
                                    for v in other_items {
                                        if !result.iter().any(|x| Value::container_eq(x, &v)) {
                                            result.push(v);
                                        }
                                    }
                                    return Ok(Value::Set(result));
                                }
                                "intersect" => {
                                    if args.len() != 1 {
                                        return Err(RuntimeError::new(
                                            "intersect() requires one argument",
                                        ));
                                    }
                                    let other = self.eval_expr(&args[0])?;
                                    let other_items = peel_other_set(other)?;
                                    let result: Vec<Value> = items
                                        .into_iter()
                                        .filter(|v| {
                                            other_items.iter().any(|x| Value::container_eq(x, v))
                                        })
                                        .collect();
                                    return Ok(Value::Set(result));
                                }
                                "diff" => {
                                    if args.len() != 1 {
                                        return Err(RuntimeError::new(
                                            "diff() requires one argument",
                                        ));
                                    }
                                    let other = self.eval_expr(&args[0])?;
                                    let other_items = peel_other_set(other)?;
                                    let result: Vec<Value> = items
                                        .into_iter()
                                        .filter(|v| {
                                            !other_items.iter().any(|x| Value::container_eq(x, v))
                                        })
                                        .collect();
                                    return Ok(Value::Set(result));
                                }
                                "to_array" => {
                                    return Ok(Value::Array(items));
                                }
                                _ => {}
                            }
                            return Ok(Value::Null);
                        }
                        _ if known_methods.contains(&method_name) => {
                            let mut full_args = vec![obj.clone()];
                            for arg in args {
                                full_args.push(self.eval_expr(arg)?);
                            }
                            if let Some(func) = self.env.get(method_name) {
                                return self.call_function(func, full_args);
                            }
                            return Err(RuntimeError::new(&format!(
                                "unknown method '{}'",
                                method_name
                            )));
                        }
                        Value::Object(_map) => {
                            return Err(RuntimeError::new(&format!(
                                "no method '{}' on object",
                                field
                            )));
                        }
                        _ => {
                            return Err(RuntimeError::new(&format!(
                                "cannot call '{}' on {}",
                                field,
                                obj.type_name()
                            )))
                        }
                    };
                    let eval_args: Result<Vec<Value>, _> =
                        args.iter().map(|a| self.eval_expr(a)).collect();
                    return self.call_function(func, eval_args?);
                }

                // Special-case push/pop for in-place mutation when first arg is a mutable variable
                if let Expr::Ident(fn_name) = function.as_ref() {
                    if fn_name == "push" && args.len() == 2 {
                        if let Expr::Ident(var_name) = &args[0] {
                            if self.env.is_mutable(var_name) == Some(true) {
                                let arr = self.eval_expr(&args[0])?;
                                let val = self.eval_expr(&args[1])?;
                                if let Value::Array(mut items) = arr {
                                    items.push(val);
                                    let new_arr = Value::Array(items);
                                    self.env.set(var_name, new_arr.clone())?;
                                    return Ok(new_arr);
                                }
                                return Err(RuntimeError::new(
                                    "push() first argument must be array",
                                ));
                            }
                        }
                    }
                    if fn_name == "pop" && args.len() == 1 {
                        if let Expr::Ident(var_name) = &args[0] {
                            if self.env.is_mutable(var_name) == Some(true) {
                                let arr = self.eval_expr(&args[0])?;
                                if let Value::Array(mut items) = arr {
                                    let popped = items.pop().unwrap_or(Value::Null);
                                    self.env.set(var_name, Value::Array(items))?;
                                    return Ok(popped);
                                }
                                return Err(RuntimeError::new("pop() requires array"));
                            }
                        }
                    }
                }

                let func = self.eval_expr(function)?;
                let eval_args: Result<Vec<Value>, _> =
                    args.iter().map(|a| self.eval_expr(a)).collect();
                let eval_args = eval_args?;
                self.call_function(func, eval_args)
            }

            Expr::Pipeline { value, function } => {
                let val = self.eval_expr(value)?;
                let func = self.eval_expr(function)?;
                self.call_function(func, vec![val])
            }

            Expr::Try(expr) => {
                let result = self.eval_expr(expr)?;
                match result {
                    Value::ResultOk(value) => Ok(*value),
                    Value::ResultErr(err) => Err(RuntimeError::propagate(Value::ResultErr(err))),
                    _ => Err(RuntimeError::new(
                        "`?` expects Result value (Ok(...) or Err(...))",
                    )),
                }
            }

            Expr::Lambda { params, body } => Ok(Value::Lambda {
                params: params.clone(),
                body: body.clone(),
                closure: Arc::new(std::sync::Mutex::new(self.env.clone())),
            }),

            Expr::StructInit { name, fields } => {
                let mut map = IndexMap::new();
                // Apply defaults first, then override with provided fields
                if let Some(defaults) = self.struct_defaults.get(name).cloned() {
                    for (k, v) in defaults {
                        map.insert(k, v);
                    }
                }
                for (key, expr) in fields {
                    map.insert(key.clone(), self.eval_expr(expr)?);
                }
                map.insert("__type__".to_string(), Value::String(name.clone()));
                Ok(Value::Object(map))
            }

            Expr::Block(stmts) => {
                self.env.push_scope();
                let mut last = Value::Null;
                let patch_err = |mut e: RuntimeError, s: &SpannedStmt| -> RuntimeError {
                    if e.line == 0 {
                        e.line = s.line;
                        e.col = s.col;
                    }
                    e
                };
                for spanned in stmts {
                    self.current_line = spanned.line;
                    let stmt = &spanned.stmt;
                    match stmt {
                        Stmt::If {
                            condition,
                            then_body,
                            else_body,
                        } => {
                            let cond = self
                                .eval_expr(condition)
                                .map_err(|e| patch_err(e, spanned))?;
                            let branch = if cond.is_truthy() {
                                then_body
                            } else if let Some(eb) = else_body {
                                eb
                            } else {
                                &vec![]
                            };
                            for s in branch {
                                self.current_line = s.line;
                                if let Signal::Return(v) =
                                    self.exec_stmt(&s.stmt).map_err(|e| patch_err(e, s))?
                                {
                                    self.env.pop_scope();
                                    return Ok(v);
                                }
                                if let Stmt::Expression(e) = &s.stmt {
                                    last = self.eval_expr(e).map_err(|e| patch_err(e, s))?;
                                }
                            }
                        }
                        _ => match self.exec_stmt(stmt).map_err(|e| patch_err(e, spanned))? {
                            Signal::Return(v) => {
                                self.env.pop_scope();
                                return Ok(v);
                            }
                            Signal::ImplicitReturn(v) => {
                                last = v;
                            }
                            _ => {
                                if let Stmt::Expression(expr) = stmt {
                                    last =
                                        self.eval_expr(expr).map_err(|e| patch_err(e, spanned))?;
                                }
                            }
                        },
                    }
                }
                self.env.pop_scope();
                Ok(last)
            }

            Expr::Spawn(body) => self.spawn_task(body),

            Expr::Await(inner) => {
                let val = self.eval_expr(inner)?;
                match val {
                    Value::TaskHandle(slot) => {
                        let (lock, cvar) = &*slot;
                        let mut guard = lock
                            .lock()
                            .map_err(|_| RuntimeError::new("await: task handle lock poisoned"))?;
                        while guard.is_none() {
                            guard = cvar
                                .wait(guard)
                                .map_err(|_| RuntimeError::new("await: condvar wait failed"))?;
                        }
                        let result = guard.take().unwrap_or(Value::Null);
                        match result {
                            Value::ResultOk(v) => Ok(*v),
                            Value::ResultErr(e) => {
                                Err(RuntimeError::new(&format!("task error: {}", e)))
                            }
                            other => Ok(other),
                        }
                    }
                    // Non-handle values pass through (backward compatible)
                    other => Ok(other),
                }
            }

            Expr::Must(inner) => {
                let val = self.eval_expr(inner)?;
                match val {
                    Value::ResultErr(err) => {
                        Err(RuntimeError::new(&format!("must failed: {}", err)))
                    }
                    Value::ResultOk(inner_val) => Ok(*inner_val),
                    Value::Null => Err(RuntimeError::new("must failed: got null")),
                    other => Ok(other),
                }
            }

            Expr::Freeze(inner) => {
                let val = self.eval_expr(inner)?;
                Ok(Value::Frozen(Box::new(val)))
            }

            Expr::Ask(prompt_expr) => {
                let prompt = self.eval_expr(prompt_expr)?;
                let prompt_str = format!("{}", prompt);
                let api_key = std::env::var("FORGE_AI_KEY")
                    .or_else(|_| std::env::var("OPENAI_API_KEY"))
                    .unwrap_or_default();
                if api_key.is_empty() {
                    return Err(RuntimeError::new(
                        "ask requires FORGE_AI_KEY or OPENAI_API_KEY environment variable",
                    ));
                }
                let body = serde_json::json!({
                    "model": std::env::var("FORGE_AI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
                    "messages": [{"role": "user", "content": prompt_str}],
                    "max_tokens": 1000
                });
                let url = std::env::var("FORGE_AI_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string());
                match crate::runtime::client::fetch_blocking(
                    &url,
                    "POST",
                    Some(body.to_string()),
                    Some(&{
                        let mut h = std::collections::HashMap::new();
                        h.insert("Authorization".to_string(), format!("Bearer {}", api_key));
                        h.insert("Content-Type".to_string(), "application/json".to_string());
                        h
                    }),
                    None,
                    None,
                    None,
                ) {
                    Ok(Value::Object(resp)) => {
                        if let Some(Value::Object(json_body)) = resp.get("json") {
                            if let Some(Value::Array(choices)) = json_body.get("choices") {
                                if let Some(Value::Object(choice)) = choices.first() {
                                    if let Some(Value::Object(msg)) = choice.get("message") {
                                        if let Some(Value::String(content)) = msg.get("content") {
                                            return Ok(Value::String(content.clone()));
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    Ok(_) => Ok(Value::Null),
                    Err(e) => Err(RuntimeError::new(&format!("ask error: {}", e))),
                }
            }

            Expr::WhereFilter {
                source,
                field,
                op,
                value,
            } => {
                let src = self.eval_expr(source)?;
                let cmp_val = self.eval_expr(value)?;
                if let Value::Array(items) = src {
                    let filtered: Vec<Value> = items
                        .into_iter()
                        .filter(|item| {
                            if let Value::Object(map) = item {
                                if let Some(field_val) = map.get(field) {
                                    match (op, field_val, &cmp_val) {
                                        (BinOp::GtEq, Value::Int(a), Value::Int(b)) => a >= b,
                                        (BinOp::Gt, Value::Int(a), Value::Int(b)) => a > b,
                                        (BinOp::Lt, Value::Int(a), Value::Int(b)) => a < b,
                                        (BinOp::LtEq, Value::Int(a), Value::Int(b)) => a <= b,
                                        (BinOp::Eq, a, b) => format!("{}", a) == format!("{}", b),
                                        (BinOp::NotEq, a, b) => {
                                            format!("{}", a) != format!("{}", b)
                                        }
                                        _ => false,
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        })
                        .collect();
                    Ok(Value::Array(filtered))
                } else {
                    Err(RuntimeError::new("where requires an array"))
                }
            }

            Expr::PipeChain { source, steps } => {
                let mut current = self.eval_expr(source)?;
                for step in steps {
                    current = match step {
                        PipeStep::Sort(field_opt) => {
                            if let Value::Array(mut items) = current {
                                items.sort_by(|a, b| match (a, b) {
                                    _ => {
                                        let left = match field_opt {
                                            Some(field) => match a {
                                                Value::Object(map) => {
                                                    map.get(field).cloned().unwrap_or(Value::Null)
                                                }
                                                _ => Value::Null,
                                            },
                                            None => a.clone(),
                                        };
                                        let right = match field_opt {
                                            Some(field) => match b {
                                                Value::Object(map) => {
                                                    map.get(field).cloned().unwrap_or(Value::Null)
                                                }
                                                _ => Value::Null,
                                            },
                                            None => b.clone(),
                                        };

                                        match (&left, &right) {
                                            (Value::Int(x), Value::Int(y)) => x.cmp(y),
                                            (Value::Float(x), Value::Float(y)) => x
                                                .partial_cmp(y)
                                                .unwrap_or(std::cmp::Ordering::Equal),
                                            (Value::String(x), Value::String(y)) => x.cmp(y),
                                            _ => format!("{}", left).cmp(&format!("{}", right)),
                                        }
                                    }
                                });
                                Value::Array(items)
                            } else {
                                current
                            }
                        }
                        PipeStep::Take(n_expr) => {
                            let n = match self.eval_expr(n_expr)? {
                                Value::Int(n) => n as usize,
                                _ => 10,
                            };
                            if let Value::Array(items) = current {
                                Value::Array(items.into_iter().take(n).collect())
                            } else {
                                current
                            }
                        }
                        PipeStep::Keep(pred) => {
                            let func = self.eval_expr(pred)?;
                            if let Value::Array(items) = current {
                                let mut out = Vec::new();
                                for item in items {
                                    let keep =
                                        self.call_function(func.clone(), vec![item.clone()])?;
                                    if keep.is_truthy() {
                                        out.push(item);
                                    }
                                }
                                Value::Array(out)
                            } else {
                                current
                            }
                        }
                        PipeStep::Apply(func_expr) => {
                            let func = self.eval_expr(func_expr)?;
                            self.call_function(func, vec![current])?
                        }
                    };
                }
                Ok(current)
            }

            Expr::Spread(inner) => {
                // Spread is handled at the call site (object/array literal construction)
                self.eval_expr(inner)
            }

            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                // In-place mutation for arr.push(x) / arr.pop() method syntax on mutable variables
                if let Expr::Ident(var_name) = object.as_ref() {
                    if self.env.is_mutable(var_name) == Some(true) {
                        if method == "push" && args.len() == 1 {
                            let arr = self.eval_expr(object)?;
                            let val = self.eval_expr(&args[0])?;
                            if let Value::Array(mut items) = arr {
                                items.push(val);
                                let new_arr = Value::Array(items);
                                self.env.set(var_name, new_arr.clone())?;
                                return Ok(new_arr);
                            }
                            return Err(RuntimeError::new("push() first argument must be array"));
                        }
                        if method == "pop" && args.is_empty() {
                            let arr = self.eval_expr(object)?;
                            if let Value::Array(mut items) = arr {
                                let popped = items.pop().unwrap_or(Value::Null);
                                self.env.set(var_name, Value::Array(items))?;
                                return Ok(popped);
                            }
                            return Err(RuntimeError::new("pop() requires array"));
                        }
                    }
                }
                // NOTE: The parser never constructs `Expr::MethodCall` — `obj.method(args)`
                // is emitted as `Expr::Call { function: FieldAccess, ... }`. All live
                // method dispatch (including sets) lives in that arm. This branch is
                // kept as a fallback for direct AST construction (tests, tools) and
                // simply forwards to the free-function form `method(obj, ...args)`.
                let obj = self.eval_expr(object)?;
                let mut full_args = vec![obj];
                for arg in args {
                    full_args.push(self.eval_expr(arg)?);
                }
                let func = self
                    .env
                    .get(method)
                    .ok_or_else(|| RuntimeError::new(&format!("unknown method: {}", method)))?;
                self.call_function(func, full_args)
            }
        }
    }

    fn eval_binop(&self, left: &Value, op: &BinOp, right: &Value) -> Result<Value, RuntimeError> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => match op {
                BinOp::Add => match a.checked_add(*b) {
                    Some(result) => Ok(Value::Int(result)),
                    None => Ok(Value::Float(*a as f64 + *b as f64)),
                },
                BinOp::Sub => match a.checked_sub(*b) {
                    Some(result) => Ok(Value::Int(result)),
                    None => Ok(Value::Float(*a as f64 - *b as f64)),
                },
                BinOp::Mul => match a.checked_mul(*b) {
                    Some(result) => Ok(Value::Int(result)),
                    None => Ok(Value::Float(*a as f64 * *b as f64)),
                },
                BinOp::Div => {
                    if *b == 0 {
                        return Err(RuntimeError::new("division by zero\n  hint: check that the divisor is not zero before dividing"));
                    }
                    Ok(Value::Int(a / b))
                }
                BinOp::Mod => {
                    if *b == 0 {
                        return Err(RuntimeError::new("modulo by zero\n  hint: check that the divisor is not zero before using %"));
                    }
                    Ok(Value::Int(a % b))
                }
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                BinOp::Lt => Ok(Value::Bool(a < b)),
                BinOp::Gt => Ok(Value::Bool(a > b)),
                BinOp::LtEq => Ok(Value::Bool(a <= b)),
                BinOp::GtEq => Ok(Value::Bool(a >= b)),
                _ => Err(RuntimeError::new("invalid operator for Int")),
            },

            (Value::Float(a), Value::Float(b)) => match op {
                BinOp::Add => Ok(Value::Float(a + b)),
                BinOp::Sub => Ok(Value::Float(a - b)),
                BinOp::Mul => Ok(Value::Float(a * b)),
                BinOp::Div => Ok(Value::Float(a / b)),
                BinOp::Mod => Ok(Value::Float(a % b)),
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                BinOp::Lt => Ok(Value::Bool(a < b)),
                BinOp::Gt => Ok(Value::Bool(a > b)),
                BinOp::LtEq => Ok(Value::Bool(a <= b)),
                BinOp::GtEq => Ok(Value::Bool(a >= b)),
                _ => Err(RuntimeError::new("invalid operator for Float")),
            },

            (Value::Int(a), Value::Float(b)) => {
                self.eval_binop(&Value::Float(*a as f64), op, &Value::Float(*b))
            }
            (Value::Float(a), Value::Int(b)) => {
                self.eval_binop(&Value::Float(*a), op, &Value::Float(*b as f64))
            }

            (Value::String(a), Value::String(b)) => match op {
                BinOp::Add => Ok(Value::String(format!("{}{}", a, b))),
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                BinOp::Lt => Ok(Value::Bool(a < b)),
                BinOp::Gt => Ok(Value::Bool(a > b)),
                BinOp::LtEq => Ok(Value::Bool(a <= b)),
                BinOp::GtEq => Ok(Value::Bool(a >= b)),
                _ => Err(RuntimeError::new("invalid operator for String")),
            },

            (Value::Bool(a), Value::Bool(b)) => match op {
                BinOp::And => Ok(Value::Bool(*a && *b)),
                BinOp::Or => Ok(Value::Bool(*a || *b)),
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                _ => Err(RuntimeError::new("invalid operator for Bool")),
            },

            (Value::String(a), b) => match op {
                BinOp::Add => Ok(Value::String(format!("{}{}", a, b))),
                _ => Err(RuntimeError::new("invalid operator")),
            },
            (a, Value::String(b)) => match op {
                BinOp::Add => Ok(Value::String(format!("{}{}", a, b))),
                _ => Err(RuntimeError::new("invalid operator")),
            },

            (Value::Null, Value::Null) => match op {
                BinOp::Eq => Ok(Value::Bool(true)),
                BinOp::NotEq => Ok(Value::Bool(false)),
                _ => Err(RuntimeError::new("cannot perform arithmetic on null")),
            },
            (Value::Null, _) | (_, Value::Null) => match op {
                BinOp::Eq => Ok(Value::Bool(false)),
                BinOp::NotEq => Ok(Value::Bool(true)),
                _ => Err(RuntimeError::new("cannot perform arithmetic on null")),
            },

            // Option equality
            (Value::Some(a), Value::Some(b)) => match op {
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                _ => Err(RuntimeError::new("invalid operator for Option")),
            },
            (Value::None, Value::None) => match op {
                BinOp::Eq => Ok(Value::Bool(true)),
                BinOp::NotEq => Ok(Value::Bool(false)),
                _ => Err(RuntimeError::new("invalid operator for Option")),
            },
            (Value::Some(_), Value::None) | (Value::None, Value::Some(_)) => match op {
                BinOp::Eq => Ok(Value::Bool(false)),
                BinOp::NotEq => Ok(Value::Bool(true)),
                _ => Err(RuntimeError::new("invalid operator for Option")),
            },

            (Value::Tuple(a), Value::Tuple(b)) => match op {
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                _ => Err(RuntimeError::new("tuples only support == and != operators")),
            },

            (Value::Set(_), Value::Set(_)) => match op {
                BinOp::Eq => Ok(Value::Bool(left == right)),
                BinOp::NotEq => Ok(Value::Bool(left != right)),
                _ => Err(RuntimeError::new("sets only support == and != operators")),
            },

            _ => Err(RuntimeError::new(&format!(
                "cannot apply {:?} to {} and {}",
                op,
                left.type_name(),
                right.type_name()
            ))),
        }
    }

    pub fn call_function(&mut self, func: Value, args: Vec<Value>) -> Result<Value, RuntimeError> {
        self.call_depth += 1;
        if self.call_depth > MAX_CALL_DEPTH {
            self.call_depth = 0;
            return Err(RuntimeError::new(
                "maximum recursion depth exceeded (512 frames)\n  hint: check for infinite recursion, or restructure to use iteration",
            ));
        }
        let frame_name = match &func {
            Value::Function { name, .. } => {
                if name.is_empty() {
                    "<anonymous>".to_string()
                } else {
                    name.clone()
                }
            }
            Value::Lambda { .. } => "<lambda>".to_string(),
            Value::BuiltIn(n) => n.clone(),
            _ => "<call>".to_string(),
        };
        if self.debug_state.is_some() {
            self.call_stack.push(DebugFrame {
                name: frame_name,
                line: self.current_line,
                col: 0,
            });
        }
        let result = self.call_function_inner(func, args);
        if self.debug_state.is_some() {
            self.call_stack.pop();
        }
        self.call_depth = self.call_depth.saturating_sub(1);
        result
    }

    fn call_function_inner(
        &mut self,
        func: Value,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        match func {
            Value::Function {
                name,
                params,
                body,
                closure,
                ..
            } => {
                let is_global_fn = !name.is_empty() && closure.scopes.len() == 1;

                let result = if is_global_fn {
                    self.env.push_scope();
                    for (i, param) in params.iter().enumerate() {
                        let val = args
                            .get(i)
                            .cloned()
                            .or_else(|| param.default.as_ref().and_then(|d| self.eval_expr(d).ok()))
                            .unwrap_or(Value::Null);
                        self.env.define(param.name.clone(), val);
                    }
                    let result = self.exec_stmts(&body);
                    self.env.pop_scope();
                    result
                } else {
                    let saved_env = self.env.clone();
                    self.env = closure;
                    self.env.push_scope();
                    if !name.is_empty() {
                        if let Some(func_val) = saved_env.get(&name) {
                            self.env.define(name.clone(), func_val.clone());
                        }
                    }
                    for (i, param) in params.iter().enumerate() {
                        let val = args
                            .get(i)
                            .cloned()
                            .or_else(|| param.default.as_ref().and_then(|d| self.eval_expr(d).ok()))
                            .unwrap_or(Value::Null);
                        self.env.define(param.name.clone(), val);
                    }
                    let result = self.exec_stmts(&body);
                    self.env.pop_scope();
                    self.env = saved_env;
                    result
                };

                match result {
                    Ok(Signal::Return(v)) => Ok(v),
                    Ok(Signal::ImplicitReturn(v)) => Ok(v),
                    Ok(_) => Ok(Value::Null),
                    Err(e) => {
                        if let Some(value) = e.propagated_value() {
                            Ok(value)
                        } else {
                            Err(e)
                        }
                    }
                }
            }

            Value::Lambda {
                params,
                body,
                closure,
            } => {
                let saved_env = self.env.clone();
                // Lock the shared closure to get the current captured state.
                // Using Arc<Mutex<Environment>> means mutations inside the lambda
                // persist across calls (fixes BUG-005: mutable closure capture).
                let captured_env = closure
                    .lock()
                    .map_err(|_| RuntimeError::new("closure lock poisoned"))?
                    .clone();
                self.env = captured_env;
                self.env.push_scope();

                for (i, param) in params.iter().enumerate() {
                    let val = args.get(i).cloned().unwrap_or(Value::Null);
                    self.env.define(param.name.clone(), val);
                }

                let result = self.exec_stmts(&body);
                self.env.pop_scope();

                // Write the modified closure back through the shared Arc<Mutex>,
                // so the next call to this lambda sees the mutations.
                if let Ok(mut guard) = closure.lock() {
                    *guard = self.env.clone();
                }

                self.env = saved_env;

                match result {
                    Ok(Signal::Return(v)) => Ok(v),
                    Ok(Signal::ImplicitReturn(v)) => Ok(v),
                    Ok(_) => Ok(Value::Null),
                    Err(e) => {
                        if let Some(value) = e.propagated_value() {
                            Ok(value)
                        } else {
                            Err(e)
                        }
                    }
                }
            }

            Value::BuiltIn(name) => self.call_builtin(&name, args),

            _ => Err(RuntimeError::new(&format!(
                "cannot call {}",
                func.type_name()
            ))),
        }
    }

    /// Spawn a block as a concurrent task, returning a TaskHandle.
    fn spawn_task(&mut self, body: &[SpannedStmt]) -> Result<Value, RuntimeError> {
        let body = body.to_vec();
        let result_slot: Arc<(std::sync::Mutex<Option<Value>>, std::sync::Condvar)> =
            Arc::new((std::sync::Mutex::new(None), std::sync::Condvar::new()));
        let slot_clone = result_slot.clone();
        let mut spawn_interp = Interpreter::new();
        spawn_interp.env = self.env.deep_clone();

        // Always use std::thread — simpler, avoids tokio dependency issues
        std::thread::spawn(move || {
            let result = spawn_interp.exec_block(&body);
            let val = match result {
                Ok(Signal::Return(v)) | Ok(Signal::ImplicitReturn(v)) => {
                    Value::ResultOk(Box::new(v))
                }
                Ok(_) => Value::ResultOk(Box::new(Value::Null)),
                Err(e) => Value::ResultErr(Box::new(Value::String(e.message))),
            };
            let (lock, cvar) = &*slot_clone;
            if let Ok(mut guard) = lock.lock() {
                *guard = Some(val);
                cvar.notify_all();
            }
        });
        Ok(Value::TaskHandle(result_slot))
    }

    // call_builtin() is in src/interpreter/builtins.rs (extracted for readability)

    // ========== Pattern Matching ==========

    fn match_pattern(&self, pattern: &Pattern, value: &Value) -> bool {
        match pattern {
            Pattern::Wildcard => true,
            Pattern::Binding(name) => {
                // Native None matches by name
                if name == "None" {
                    return matches!(value, Value::None);
                }
                // If the binding name matches a known ADT unit variant, treat as constructor match
                if let std::option::Option::Some(ref bound_val) = self.env.get(name) {
                    if let Value::Object(ref obj) = bound_val {
                        if let std::option::Option::Some(Value::String(bound_variant)) =
                            obj.get("__variant__")
                        {
                            if let Value::Object(val_obj) = value {
                                if let std::option::Option::Some(Value::String(val_variant)) =
                                    val_obj.get("__variant__")
                                {
                                    return bound_variant == val_variant;
                                }
                            }
                            return false;
                        }
                    }
                    // Native None check via binding
                    if matches!(bound_val, Value::None) && matches!(value, Value::None) {
                        return true;
                    }
                    if matches!(bound_val, Value::None) && !matches!(value, Value::None) {
                        return false;
                    }
                }
                true
            }
            Pattern::Literal(expr) => match (expr, value) {
                (Expr::Int(a), Value::Int(b)) => a == b,
                (Expr::Float(a), Value::Float(b)) => a == b,
                (Expr::StringLit(a), Value::String(b)) => a == b,
                (Expr::Bool(a), Value::Bool(b)) => a == b,
                _ => false,
            },
            Pattern::Constructor { name, fields } => {
                match (name.as_str(), value) {
                    ("Ok", Value::ResultOk(inner)) => {
                        return fields.is_empty()
                            || (fields.len() == 1
                                && self.match_pattern(&fields[0], inner.as_ref()));
                    }
                    ("Err", Value::ResultErr(inner)) => {
                        return fields.is_empty()
                            || (fields.len() == 1
                                && self.match_pattern(&fields[0], inner.as_ref()));
                    }
                    ("Some", Value::Some(inner)) => {
                        return fields.is_empty()
                            || (fields.len() == 1
                                && self.match_pattern(&fields[0], inner.as_ref()));
                    }
                    ("None", Value::None) => {
                        return fields.is_empty();
                    }
                    _ => {}
                }

                if let Value::Object(map) = value {
                    if let Some(Value::String(type_name)) = map.get("__type__") {
                        if type_name == name {
                            if fields.is_empty() {
                                return true;
                            }
                            return fields.iter().enumerate().all(|(i, pat)| {
                                let key = format!("_{}", i);
                                map.get(&key)
                                    .map(|field_val| self.match_pattern(pat, field_val))
                                    .unwrap_or(false)
                            });
                        }
                    }
                    if let Some(Value::String(variant)) = map.get("__variant__") {
                        if variant == name {
                            if fields.is_empty() {
                                return true;
                            }
                            return fields.iter().enumerate().all(|(i, pat)| {
                                let key = format!("_{}", i);
                                map.get(&key)
                                    .map(|field_val| self.match_pattern(pat, field_val))
                                    .unwrap_or(false)
                            });
                        }
                    }
                }
                false
            }
        }
    }

    fn bind_pattern(&mut self, pattern: &Pattern, value: &Value) {
        match pattern {
            Pattern::Binding(name) => {
                self.env.define(name.clone(), value.clone());
            }
            Pattern::Constructor { name, fields } => {
                match (name.as_str(), value) {
                    ("Ok", Value::ResultOk(inner))
                    | ("Err", Value::ResultErr(inner))
                    | ("Some", Value::Some(inner)) => {
                        if let std::option::Option::Some(field_pat) = fields.first() {
                            self.bind_pattern(field_pat, inner.as_ref());
                        }
                        return;
                    }
                    _ => {}
                }

                if let Value::Object(map) = value {
                    for (i, field_pat) in fields.iter().enumerate() {
                        let key = format!("_{}", i);
                        if let Some(val) = map.get(&key) {
                            self.bind_pattern(field_pat, val);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Convert serde_json::Value to Forge Value
fn check_interface_satisfaction(value: &Value, methods: &[Value], env: &Environment) -> bool {
    for method_spec in methods {
        if let Value::Object(spec) = method_spec {
            if let Some(Value::String(method_name)) = spec.get("name") {
                let has_method = match value {
                    Value::Object(obj) => {
                        if let Some(Value::String(type_name)) = obj.get("__type__") {
                            // Check if there's a function named type_name.method_name or just method_name in scope
                            let qualified = format!("{}.{}", type_name, method_name);
                            env.get(&qualified).is_some() || obj.contains_key(method_name)
                        } else {
                            obj.contains_key(method_name)
                        }
                    }
                    _ => false,
                };
                if !has_method {
                    return false;
                }
            }
        }
    }
    true
}

fn json_to_value(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(items) => {
            Value::Array(items.into_iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, json_to_value(v)))
                .collect(),
        ),
    }
}

/// Runtime error
#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
    pub line: usize,
    pub col: usize,
    propagated: Option<Value>,
}

impl RuntimeError {
    pub fn new(msg: &str) -> Self {
        Self {
            message: msg.to_string(),
            line: 0,
            col: 0,
            propagated: None,
        }
    }

    pub fn propagate(value: Value) -> Self {
        let message = match &value {
            Value::ResultErr(err) => format!("unhandled error: {}", err),
            _ => format!("unhandled propagated value: {}", value),
        };
        Self {
            message,
            line: 0,
            col: 0,
            propagated: Some(value),
        }
    }

    pub fn propagated_value(&self) -> Option<Value> {
        self.propagated.clone()
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Runtime error: {}", self.message)
    }
}

#[cfg(test)]
mod tests;
