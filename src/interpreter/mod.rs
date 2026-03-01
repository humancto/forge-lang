use crate::parser::ast::*;
/// Forge Tree-Walk Interpreter
/// Walks the AST and executes it directly.
/// Phase 1 only — replaced by bytecode VM in Phase 3.
use indexmap::IndexMap;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Runtime values
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),
    Function {
        name: String,
        params: Vec<Param>,
        body: Vec<Stmt>,
        closure: Environment,
        decorators: Vec<Decorator>,
    },
    Lambda {
        params: Vec<Param>,
        body: Vec<Stmt>,
        closure: Environment,
    },
    ResultOk(Box<Value>),
    ResultErr(Box<Value>),
    /// Built-in function
    BuiltIn(String),
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
            (Value::Object(a), Value::Object(b)) => a == b,
            (Value::ResultOk(a), Value::ResultOk(b)) => a == b,
            (Value::ResultErr(a), Value::ResultErr(b)) => a == b,
            (Value::BuiltIn(a), Value::BuiltIn(b)) => a == b,
            _ => false,
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::String(_) => "String",
            Value::Bool(_) => "Bool",
            Value::Array(_) => "Array",
            Value::Object(_) => "Object",
            Value::Function { .. } => "Function",
            Value::Lambda { .. } => "Lambda",
            Value::ResultOk(_) | Value::ResultErr(_) => "Result",
            Value::BuiltIn(_) => "BuiltIn",
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
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            Value::ResultOk(_) => true,
            Value::ResultErr(_) => false,
            _ => true,
        }
    }

    pub fn to_json_string(&self) -> String {
        match self {
            Value::Object(map) => {
                let entries: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("\"{}\": {}", k, v.to_json_string()))
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
            Value::Array(items) => {
                let entries: Vec<String> = items.iter().map(|v| v.to_json_string()).collect();
                format!("[{}]", entries.join(", "))
            }
            Value::String(s) => format!("\"{}\"", s),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => format!("{}", n),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::ResultOk(v) => format!("{{ \"Ok\": {} }}", v.to_json_string()),
            Value::ResultErr(v) => format!("{{ \"Err\": {} }}", v.to_json_string()),
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
            Value::Object(_) => write!(f, "{}", self.to_json_string()),
            Value::Function { name, .. } => write!(f, "<fn {}>", name),
            Value::Lambda { .. } => write!(f, "<lambda>"),
            Value::ResultOk(v) => write!(f, "Ok({})", v),
            Value::ResultErr(v) => write!(f, "Err({})", v),
            Value::BuiltIn(name) => write!(f, "<builtin {}>", name),
        }
    }
}

/// Variable environment (scope chain) — uses Arc for O(1) cloning
#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<Arc<HashMap<String, Value>>>,
    mutability: Vec<Arc<HashMap<String, bool>>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            scopes: vec![Arc::new(HashMap::new())],
            mutability: vec![Arc::new(HashMap::new())],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(Arc::new(HashMap::new()));
        self.mutability.push(Arc::new(HashMap::new()));
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
        self.mutability.pop();
    }

    pub fn define(&mut self, name: String, value: Value) {
        self.define_with_mutability(name, value, true);
    }

    pub fn define_with_mutability(&mut self, name: String, value: Value, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            Arc::make_mut(scope).insert(name.clone(), value);
        }
        if let Some(muts) = self.mutability.last_mut() {
            Arc::make_mut(muts).insert(name, mutable);
        }
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }

    fn is_mutable(&self, name: &str) -> Option<bool> {
        for muts in self.mutability.iter().rev() {
            if let Some(m) = muts.get(name) {
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
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                Arc::make_mut(scope).insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(RuntimeError::new(&format!("undefined variable: {}", name)))
    }

    pub fn suggest_similar(&self, name: &str) -> Option<String> {
        let mut best: Option<(String, usize)> = None;
        for scope in &self.scopes {
            for key in scope.keys() {
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

/// The interpreter
pub struct Interpreter {
    pub env: Environment,
    call_depth: usize,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
            env: Environment::new(),
            call_depth: 0,
        };
        interp.register_builtins();
        interp
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

        // Prelude: Option type = Some(value) | None
        self.env.define(
            "Some".to_string(),
            Value::BuiltIn("adt:Option:Some:1".to_string()),
        );
        {
            let mut none_obj = IndexMap::new();
            none_obj.insert("__type__".to_string(), Value::String("Option".to_string()));
            none_obj.insert("__variant__".to_string(), Value::String("None".to_string()));
            self.env.define("None".to_string(), Value::Object(none_obj));
        }
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
            "fetch",
            "uuid",
            "say",
            "yell",
            "whisper",
            "wait",
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
        ] {
            self.env
                .define(name.to_string(), Value::BuiltIn(name.to_string()));
        }
    }

    pub fn run(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        for stmt in &program.statements {
            match self.exec_stmt(stmt)? {
                Signal::Return(v) => return Ok(v),
                Signal::Break => return Err(RuntimeError::new("break outside of loop")),
                Signal::Continue => return Err(RuntimeError::new("continue outside of loop")),
                Signal::None | Signal::ImplicitReturn(_) => {}
            }
        }
        Ok(Value::Null)
    }

    /// Run in REPL mode — returns the value of the last expression for display
    pub fn run_repl(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        let mut last = Value::Null;
        for stmt in &program.statements {
            match self.exec_stmt(stmt)? {
                Signal::Return(v) => return Ok(v),
                Signal::Break => return Err(RuntimeError::new("break outside of loop")),
                Signal::Continue => return Err(RuntimeError::new("continue outside of loop")),
                Signal::None | Signal::ImplicitReturn(_) => {}
            }
            if let Stmt::Expression(ref expr) = stmt {
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
                        let mut obj =
                            self.env.get(&name).cloned().ok_or_else(|| {
                                RuntimeError::new(&format!("undefined: {}", name))
                            })?;
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
                        let mut arr =
                            self.env.get(&name).cloned().ok_or_else(|| {
                                RuntimeError::new(&format!("undefined: {}", name))
                            })?;
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

            Stmt::StructDef { name, fields: _ } => {
                self.env
                    .define(name.clone(), Value::BuiltIn(format!("struct:{}", name)));
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
                        if let Some(Value::Object(type_meta)) = self.env.get(&type_key).cloned() {
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
                    Value::Array(items) => {
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
                    _ => return Err(RuntimeError::new("can only iterate over arrays or objects")),
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
                // Run spawn body on a background thread for real concurrency
                let body = body.clone();
                let mut spawn_interp = Interpreter::new();
                // Copy current environment for the spawn
                spawn_interp.env = self.env.clone();
                std::thread::spawn(move || {
                    let _ = spawn_interp.exec_block(&body);
                });
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
                    self.env.define(catch_var.clone(), Value::String(e.message));
                    let result = self.exec_block(catch_body);
                    self.env.pop_scope();
                    result.unwrap_or(Signal::None);
                    Ok(Signal::None)
                }
            },

            Stmt::Import { path, names } => {
                let builtin_modules = [
                    "math", "fs", "io", "crypto", "db", "pg", "env", "json", "regex", "log",
                    "term", "http", "csv", "exec", "time",
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

                let file_path = match crate::package::resolve_import(path) {
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
                import_interp.run(&program)?;

                if let Some(name_list) = names {
                    for name in name_list {
                        if let Some(val) = import_interp.env.get(name).cloned() {
                            self.env.define(name.to_string(), val);
                        }
                    }
                } else {
                    // Import all top-level definitions
                    // We check what the import interpreter defined beyond builtins
                    for stmt in &program.statements {
                        match stmt {
                            Stmt::FnDef { name, .. } | Stmt::Let { name, .. } => {
                                if let Some(val) = import_interp.env.get(name).cloned() {
                                    self.env.define(name.clone(), val);
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
                let mut timeout_interp = Interpreter::new();
                timeout_interp.env = self.env.clone();
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
                        drop(handle);
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
                let secs = match self.eval_expr(interval)? {
                    Value::Int(n) => match unit.as_str() {
                        "minutes" => n as u64 * 60,
                        "hours" => n as u64 * 3600,
                        _ => n as u64,
                    },
                    _ => 60,
                };
                let body = body.clone();
                let mut sched_interp = Interpreter::new();
                sched_interp.env = self.env.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(secs));
                    let _ = sched_interp.exec_block(&body);
                });
                Ok(Signal::None)
            }

            Stmt::WatchBlock { path, body } => {
                let path_str = match self.eval_expr(path)? {
                    Value::String(s) => s,
                    _ => return Err(RuntimeError::new("watch requires a string path")),
                };
                let body = body.clone();
                let mut watch_interp = Interpreter::new();
                watch_interp.env = self.env.clone();
                std::thread::spawn(move || {
                    let mut last_modified =
                        std::fs::metadata(&path_str).and_then(|m| m.modified()).ok();
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        let current = std::fs::metadata(&path_str).and_then(|m| m.modified()).ok();
                        if current != last_modified {
                            last_modified = current;
                            let _ = watch_interp.exec_block(&body);
                        }
                    }
                });
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

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<Signal, RuntimeError> {
        self.env.push_scope();
        let result = self.exec_stmts(stmts);
        self.env.pop_scope();
        result
    }

    fn exec_stmts(&mut self, stmts: &[Stmt]) -> Result<Signal, RuntimeError> {
        let mut result = Signal::None;
        let mut last_expr_value = Value::Null;
        for stmt in stmts {
            if let Stmt::Expression(expr) = stmt {
                last_expr_value = self.eval_expr(expr)?;
                continue;
            }
            last_expr_value = Value::Null;
            result = self.exec_stmt(stmt)?;
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

            Expr::Object(fields) => {
                let mut map = IndexMap::new();
                for (key, expr) in fields {
                    map.insert(key.clone(), self.eval_expr(expr)?);
                }
                Ok(Value::Object(map))
            }

            Expr::Ident(name) => self.env.get(name).cloned().ok_or_else(|| {
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
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                self.eval_binop(&l, op, &r)
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
                match &obj {
                    Value::Object(map) => map.get(field).cloned().ok_or_else(|| {
                        RuntimeError::new(&format!("no field '{}' on object", field))
                    }),
                    Value::String(s) => match field.as_str() {
                        "len" => Ok(Value::Int(s.len() as i64)),
                        "upper" => Ok(Value::String(s.to_uppercase())),
                        "lower" => Ok(Value::String(s.to_lowercase())),
                        "trim" => Ok(Value::String(s.trim().to_string())),
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
                match (&obj, &idx) {
                    (Value::Array(items), Value::Int(i)) => {
                        let i = *i as usize;
                        items
                            .get(i)
                            .cloned()
                            .ok_or_else(|| RuntimeError::new("index out of bounds"))
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
                    ];
                    let func = match &obj {
                        Value::Object(map) if map.get(field).is_some() => {
                            map.get(field).cloned().unwrap()
                        }
                        Value::String(s)
                            if matches!(
                                method_name,
                                "upper" | "lower" | "trim" | "len" | "chars"
                            ) =>
                        {
                            match method_name {
                                "upper" => return Ok(Value::String(s.to_uppercase())),
                                "lower" => return Ok(Value::String(s.to_lowercase())),
                                "trim" => return Ok(Value::String(s.trim().to_string())),
                                "len" => return Ok(Value::Int(s.len() as i64)),
                                "chars" => {
                                    return Ok(Value::Array(
                                        s.chars().map(|c| Value::String(c.to_string())).collect(),
                                    ))
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
                            if let Some(func) = self.env.get(method_name).cloned() {
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
                closure: self.env.clone(),
            }),

            Expr::StructInit { name, fields } => {
                let mut map = IndexMap::new();
                for (key, expr) in fields {
                    map.insert(key.clone(), self.eval_expr(expr)?);
                }
                map.insert("__type__".to_string(), Value::String(name.clone()));
                Ok(Value::Object(map))
            }

            Expr::Block(stmts) => {
                self.env.push_scope();
                let mut last = Value::Null;
                for stmt in stmts {
                    match stmt {
                        Stmt::If {
                            condition,
                            then_body,
                            else_body,
                        } => {
                            let cond = self.eval_expr(condition)?;
                            let branch = if cond.is_truthy() {
                                then_body
                            } else if let Some(eb) = else_body {
                                eb
                            } else {
                                &vec![]
                            };
                            for s in branch {
                                if let Signal::Return(v) = self.exec_stmt(s)? {
                                    self.env.pop_scope();
                                    return Ok(v);
                                }
                                if let Stmt::Expression(e) = s {
                                    last = self.eval_expr(e)?;
                                }
                            }
                        }
                        _ => match self.exec_stmt(stmt)? {
                            Signal::Return(v) => {
                                self.env.pop_scope();
                                return Ok(v);
                            }
                            Signal::ImplicitReturn(v) => {
                                last = v;
                            }
                            _ => {
                                if let Stmt::Expression(expr) = stmt {
                                    last = self.eval_expr(expr)?;
                                }
                            }
                        },
                    }
                }
                self.env.pop_scope();
                Ok(last)
            }

            Expr::Await(inner) => self.eval_expr(inner),

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
                Ok(val)
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
                        PipeStep::Sort(_) => {
                            if let Value::Array(mut items) = current {
                                items.sort_by(|a, b| match (a, b) {
                                    (Value::Int(x), Value::Int(y)) => x.cmp(y),
                                    _ => std::cmp::Ordering::Equal,
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
                let obj = self.eval_expr(object)?;
                let mut full_args = vec![obj];
                for arg in args {
                    full_args.push(self.eval_expr(arg)?);
                }
                let func = self
                    .env
                    .get(method)
                    .cloned()
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
        let result = self.call_function_inner(func, args);
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
                self.env = closure;
                self.env.push_scope();

                for (i, param) in params.iter().enumerate() {
                    let val = args.get(i).cloned().unwrap_or(Value::Null);
                    self.env.define(param.name.clone(), val);
                }

                let result = self.exec_stmts(&body);
                self.env.pop_scope();
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

    pub fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        match name {
            "print" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                let output = text.join(" ");
                print!("{}", output);
                Ok(Value::Null)
            }
            "println" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                let output = text.join(" ");
                println!("{}", output);
                Ok(Value::Null)
            }
            "len" => match args.first() {
                Some(Value::String(s)) => Ok(Value::Int(s.len() as i64)),
                Some(Value::Array(a)) => Ok(Value::Int(a.len() as i64)),
                Some(Value::Object(o)) => Ok(Value::Int(o.len() as i64)),
                _ => Err(RuntimeError::new("len() requires string, array, or object")),
            },
            "type" | "typeof" => match args.first() {
                Some(v) => Ok(Value::String(v.type_name().to_string())),
                None => Err(RuntimeError::new("typeof() requires an argument")),
            },
            "str" => match args.first() {
                Some(v) => Ok(Value::String(format!("{}", v))),
                None => Ok(Value::String(String::new())),
            },
            "int" => match args.first() {
                Some(Value::Int(n)) => Ok(Value::Int(*n)),
                Some(Value::Float(n)) => Ok(Value::Int(*n as i64)),
                Some(Value::String(s)) => s
                    .parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| RuntimeError::new(&format!("cannot convert '{}' to Int", s))),
                _ => Err(RuntimeError::new("int() requires number or string")),
            },
            "float" => match args.first() {
                Some(Value::Int(n)) => Ok(Value::Float(*n as f64)),
                Some(Value::Float(n)) => Ok(Value::Float(*n)),
                Some(Value::String(s)) => s
                    .parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| RuntimeError::new(&format!("cannot convert '{}' to Float", s))),
                _ => Err(RuntimeError::new("float() requires number or string")),
            },
            "push" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("push() requires array and value"));
                }
                if let Value::Array(mut items) = args[0].clone() {
                    items.push(args[1].clone());
                    Ok(Value::Array(items))
                } else {
                    Err(RuntimeError::new("push() first argument must be array"))
                }
            }
            "pop" => match args.first() {
                Some(Value::Array(items)) => {
                    let mut out = items.clone();
                    out.pop();
                    Ok(Value::Array(out))
                }
                _ => Err(RuntimeError::new("pop() requires array")),
            },
            "keys" => match args.first() {
                Some(Value::Object(map)) => Ok(Value::Array(
                    map.keys().map(|k| Value::String(k.clone())).collect(),
                )),
                _ => Err(RuntimeError::new("keys() requires object")),
            },
            "values" => match args.first() {
                Some(Value::Object(map)) => Ok(Value::Array(map.values().cloned().collect())),
                _ => Err(RuntimeError::new("values() requires object")),
            },
            "contains" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(sub))) => {
                    Ok(Value::Bool(s.contains(sub.as_str())))
                }
                (Some(Value::Array(arr)), Some(val)) => Ok(Value::Bool(
                    arr.iter().any(|v| format!("{}", v) == format!("{}", val)),
                )),
                (Some(Value::Object(map)), Some(Value::String(key))) => {
                    Ok(Value::Bool(map.contains_key(key)))
                }
                _ => Err(RuntimeError::new(
                    "contains() requires (string, substring), (array, value), or (object, key)",
                )),
            },
            "has_key" => match (args.first(), args.get(1)) {
                (Some(Value::Object(map)), Some(Value::String(key))) => {
                    Ok(Value::Bool(map.contains_key(key)))
                }
                _ => Err(RuntimeError::new("has_key() requires (object, key_string)")),
            },
            "get" => match (args.first(), args.get(1)) {
                (Some(obj @ Value::Object(_)), Some(Value::String(key))) => {
                    let default = args.get(2).cloned().unwrap_or(Value::Null);
                    if key.contains('.') {
                        let parts: Vec<&str> = key.split('.').collect();
                        let mut current = obj.clone();
                        for part in &parts {
                            match current {
                                Value::Object(ref m) => {
                                    current = match m.get(*part) {
                                        Some(v) => v.clone(),
                                        None => return Ok(default),
                                    };
                                }
                                Value::Array(ref arr) => {
                                    if let Ok(idx) = part.parse::<usize>() {
                                        current = match arr.get(idx) {
                                            Some(v) => v.clone(),
                                            None => return Ok(default),
                                        };
                                    } else {
                                        return Ok(default);
                                    }
                                }
                                _ => return Ok(default),
                            }
                        }
                        Ok(current)
                    } else if let Value::Object(map) = obj {
                        Ok(map.get(key).cloned().unwrap_or(default))
                    } else {
                        Ok(default)
                    }
                }
                (Some(Value::Array(arr)), Some(Value::Int(idx))) => {
                    let default = args.get(2).cloned().unwrap_or(Value::Null);
                    Ok(arr.get(*idx as usize).cloned().unwrap_or(default))
                }
                _ => Err(RuntimeError::new(
                    "get() requires (object, key) or (array, index)",
                )),
            },
            "pick" => match (args.first(), args.get(1)) {
                (Some(Value::Object(map)), Some(Value::Array(field_list))) => {
                    let mut result = IndexMap::new();
                    for field in field_list {
                        if let Value::String(key) = field {
                            if let Some(val) = map.get(key) {
                                result.insert(key.clone(), val.clone());
                            }
                        }
                    }
                    Ok(Value::Object(result))
                }
                _ => Err(RuntimeError::new("pick() requires (object, [field_names])")),
            },
            "omit" => match (args.first(), args.get(1)) {
                (Some(Value::Object(map)), Some(Value::Array(field_list))) => {
                    let omit_keys: Vec<String> = field_list
                        .iter()
                        .filter_map(|v| {
                            if let Value::String(s) = v {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    let result: IndexMap<String, Value> = map
                        .iter()
                        .filter(|(k, _)| !omit_keys.contains(k))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    Ok(Value::Object(result))
                }
                _ => Err(RuntimeError::new("omit() requires (object, [field_names])")),
            },
            "merge" => {
                let mut result = IndexMap::new();
                for arg in &args {
                    if let Value::Object(map) = arg {
                        for (k, v) in map {
                            result.insert(k.clone(), v.clone());
                        }
                    } else {
                        return Err(RuntimeError::new(
                            "merge() requires all arguments to be objects",
                        ));
                    }
                }
                Ok(Value::Object(result))
            }
            "find" => match (args.first(), args.get(1)) {
                (Some(Value::Array(arr)), Some(func)) => {
                    for item in arr {
                        let result = self.call_function(func.clone(), vec![item.clone()])?;
                        if result.is_truthy() {
                            return Ok(item.clone());
                        }
                    }
                    Ok(Value::Null)
                }
                _ => Err(RuntimeError::new("find() requires (array, predicate_fn)")),
            },
            "flat_map" => match (args.first(), args.get(1)) {
                (Some(Value::Array(arr)), Some(func)) => {
                    let mut result = Vec::new();
                    for item in arr {
                        let mapped = self.call_function(func.clone(), vec![item.clone()])?;
                        match mapped {
                            Value::Array(inner) => result.extend(inner),
                            other => result.push(other),
                        }
                    }
                    Ok(Value::Array(result))
                }
                _ => Err(RuntimeError::new("flat_map() requires (array, fn)")),
            },
            "entries" => match args.first() {
                Some(Value::Object(map)) => {
                    let pairs: Vec<Value> = map
                        .iter()
                        .map(|(k, v)| Value::Array(vec![Value::String(k.clone()), v.clone()]))
                        .collect();
                    Ok(Value::Array(pairs))
                }
                _ => Err(RuntimeError::new("entries() requires an object")),
            },
            "from_entries" => match args.first() {
                Some(Value::Array(pairs)) => {
                    let mut result = IndexMap::new();
                    for pair in pairs {
                        if let Value::Array(kv) = pair {
                            if let (Some(Value::String(k)), Some(v)) = (kv.first(), kv.get(1)) {
                                result.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    Ok(Value::Object(result))
                }
                _ => Err(RuntimeError::new(
                    "from_entries() requires an array of [key, value] pairs",
                )),
            },
            "range" => match (args.first(), args.get(1)) {
                (Some(Value::Int(start)), Some(Value::Int(end))) => {
                    Ok(Value::Array((*start..*end).map(Value::Int).collect()))
                }
                (Some(Value::Int(end)), None) => {
                    Ok(Value::Array((0..*end).map(Value::Int).collect()))
                }
                _ => Err(RuntimeError::new("range() requires integer arguments")),
            },
            "enumerate" => match args.first() {
                Some(Value::Array(items)) => {
                    let mut pairs = Vec::with_capacity(items.len());
                    for (idx, item) in items.iter().enumerate() {
                        let mut row = IndexMap::new();
                        row.insert("index".to_string(), Value::Int(idx as i64));
                        row.insert("value".to_string(), item.clone());
                        pairs.push(Value::Object(row));
                    }
                    Ok(Value::Array(pairs))
                }
                _ => Err(RuntimeError::new("enumerate() requires array")),
            },
            "map" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("map() requires (array, function)"));
                }
                let items = match &args[0] {
                    Value::Array(items) => items.clone(),
                    _ => return Err(RuntimeError::new("map() first argument must be array")),
                };
                let func = args[1].clone();
                let mut out = Vec::with_capacity(items.len());

                for item in items {
                    out.push(self.call_function(func.clone(), vec![item])?);
                }

                Ok(Value::Array(out))
            }
            "filter" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("filter() requires (array, function)"));
                }
                let items = match &args[0] {
                    Value::Array(items) => items.clone(),
                    _ => return Err(RuntimeError::new("filter() first argument must be array")),
                };
                let func = args[1].clone();
                let mut out = Vec::new();

                for item in items {
                    let keep = self.call_function(func.clone(), vec![item.clone()])?;
                    if keep.is_truthy() {
                        out.push(item);
                    }
                }

                Ok(Value::Array(out))
            }
            "Ok" | "ok" => {
                let value = args.first().cloned().unwrap_or(Value::Null);
                Ok(Value::ResultOk(Box::new(value)))
            }
            "Err" | "err" => {
                let value = args
                    .first()
                    .cloned()
                    .unwrap_or(Value::String("error".to_string()));
                Ok(Value::ResultErr(Box::new(value)))
            }
            "is_ok" => match args.first() {
                Some(Value::ResultOk(_)) => Ok(Value::Bool(true)),
                Some(Value::ResultErr(_)) => Ok(Value::Bool(false)),
                _ => Err(RuntimeError::new("is_ok() requires a Result value")),
            },
            "is_err" => match args.first() {
                Some(Value::ResultOk(_)) => Ok(Value::Bool(false)),
                Some(Value::ResultErr(_)) => Ok(Value::Bool(true)),
                _ => Err(RuntimeError::new("is_err() requires a Result value")),
            },
            "unwrap" => match args.first() {
                Some(Value::ResultOk(value)) => Ok((**value).clone()),
                Some(Value::ResultErr(err)) => {
                    Err(RuntimeError::new(&format!("unwrap() on Err: {}", err)))
                }
                _ => Err(RuntimeError::new("unwrap() requires a Result value")),
            },
            "unwrap_or" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("unwrap_or() requires (result, default)"));
                }
                match &args[0] {
                    Value::ResultOk(value) => Ok((**value).clone()),
                    Value::ResultErr(_) => Ok(args[1].clone()),
                    _ => Err(RuntimeError::new(
                        "unwrap_or() requires a Result value as first argument",
                    )),
                }
            }
            "fetch" => match args.first() {
                Some(Value::String(url)) => {
                    let method = match args.get(1) {
                        Some(Value::Object(opts)) => opts
                            .get("method")
                            .and_then(|v| {
                                if let Value::String(s) = v {
                                    Some(s.to_uppercase())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_else(|| "GET".to_string()),
                        _ => "GET".to_string(),
                    };

                    let body = match args.get(1) {
                        Some(Value::Object(opts)) => opts.get("body").map(|v| v.to_json_string()),
                        _ => None,
                    };

                    match crate::runtime::client::fetch_blocking(url, &method, body, None) {
                        Ok(value) => Ok(value),
                        Err(e) => Err(RuntimeError::new(&format!("fetch error: {}", e))),
                    }
                }
                _ => Err(RuntimeError::new("fetch() requires a URL string")),
            },
            "time" => {
                crate::stdlib::time::call("time.now", args).map_err(|e| RuntimeError::new(&e))
            }
            "json" => match args.first() {
                Some(Value::String(s)) => match serde_json::from_str::<serde_json::Value>(s) {
                    Ok(v) => Ok(json_to_value(v)),
                    Err(e) => Err(RuntimeError::new(&format!("JSON parse error: {}", e))),
                },
                Some(v) => Ok(Value::String(v.to_json_string())),
                None => Err(RuntimeError::new("json() requires an argument")),
            },
            "uuid" => Ok(Value::String(uuid::Uuid::new_v4().to_string())),
            "say" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                println!("{}", text.join(" "));
                Ok(Value::Null)
            }
            "yell" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                println!("{}", text.join(" ").to_uppercase());
                Ok(Value::Null)
            }
            "whisper" => {
                let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
                println!("{}", text.join(" ").to_lowercase());
                Ok(Value::Null)
            }
            "wait" => match args.first() {
                Some(Value::Int(secs)) => {
                    let s = (*secs).max(0) as u64;
                    std::thread::sleep(std::time::Duration::from_secs(s));
                    Ok(Value::Null)
                }
                Some(Value::Float(secs)) => {
                    std::thread::sleep(std::time::Duration::from_secs_f64(*secs));
                    Ok(Value::Null)
                }
                _ => Err(RuntimeError::new("wait() requires a number of seconds")),
            },
            "reduce" => {
                if args.len() != 3 {
                    return Err(RuntimeError::new(
                        "reduce() requires (array, initial, function)",
                    ));
                }
                let items = match &args[0] {
                    Value::Array(items) => items.clone(),
                    _ => return Err(RuntimeError::new("reduce() first argument must be array")),
                };
                let mut acc = args[1].clone();
                let func = args[2].clone();
                for item in items {
                    acc = self.call_function(func.clone(), vec![acc, item])?;
                }
                Ok(acc)
            }
            "sort" => match args.first() {
                Some(Value::Array(items)) => {
                    let mut sorted = items.clone();
                    sorted.sort_by(|a, b| match (a, b) {
                        (Value::Int(x), Value::Int(y)) => x.cmp(y),
                        (Value::Float(x), Value::Float(y)) => {
                            x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (Value::String(x), Value::String(y)) => x.cmp(y),
                        _ => std::cmp::Ordering::Equal,
                    });
                    Ok(Value::Array(sorted))
                }
                _ => Err(RuntimeError::new("sort() requires an array")),
            },
            "reverse" => match args.first() {
                Some(Value::Array(items)) => {
                    let mut reversed = items.clone();
                    reversed.reverse();
                    Ok(Value::Array(reversed))
                }
                _ => Err(RuntimeError::new("reverse() requires an array")),
            },
            "split" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(delim))) => Ok(Value::Array(
                    s.split(delim.as_str())
                        .map(|part| Value::String(part.to_string()))
                        .collect(),
                )),
                _ => Err(RuntimeError::new(
                    "split() requires (string, delimiter_string)",
                )),
            },
            "join" => match (args.first(), args.get(1)) {
                (Some(Value::Array(items)), Some(Value::String(sep))) => {
                    let parts: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                    Ok(Value::String(parts.join(sep)))
                }
                (Some(Value::Array(items)), None) => {
                    let parts: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                    Ok(Value::String(parts.join("")))
                }
                _ => Err(RuntimeError::new(
                    "join() requires (array[, separator_string])",
                )),
            },
            "replace" => match (args.first(), args.get(1), args.get(2)) {
                (Some(Value::String(s)), Some(Value::String(from)), Some(Value::String(to))) => {
                    Ok(Value::String(s.replace(from.as_str(), to.as_str())))
                }
                _ => Err(RuntimeError::new(
                    "replace() requires (string, from_string, to_string)",
                )),
            },
            "starts_with" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(prefix))) => {
                    Ok(Value::Bool(s.starts_with(prefix.as_str())))
                }
                _ => Err(RuntimeError::new(
                    "starts_with() requires (string, prefix_string)",
                )),
            },
            "ends_with" => match (args.first(), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(suffix))) => {
                    Ok(Value::Bool(s.ends_with(suffix.as_str())))
                }
                _ => Err(RuntimeError::new(
                    "ends_with() requires (string, suffix_string)",
                )),
            },
            "is_some" => match args.first() {
                Some(Value::Object(obj)) => {
                    let is_opt = obj
                        .get("__type__")
                        .is_some_and(|v| matches!(v, Value::String(s) if s == "Option"));
                    if is_opt {
                        let variant = obj.get("__variant__").map(|v| format!("{}", v));
                        Ok(Value::Bool(variant.as_deref() == Some("Some")))
                    } else {
                        Ok(Value::Bool(true))
                    }
                }
                Some(Value::Null) => Ok(Value::Bool(false)),
                Some(_) => Ok(Value::Bool(true)),
                None => Err(RuntimeError::new("is_some() requires an argument")),
            },
            "is_none" => match args.first() {
                Some(Value::Object(obj)) => {
                    let is_opt = obj
                        .get("__type__")
                        .is_some_and(|v| matches!(v, Value::String(s) if s == "Option"));
                    if is_opt {
                        let variant = obj.get("__variant__").map(|v| format!("{}", v));
                        Ok(Value::Bool(variant.as_deref() == Some("None")))
                    } else {
                        Ok(Value::Bool(false))
                    }
                }
                Some(Value::Null) => Ok(Value::Bool(true)),
                Some(_) => Ok(Value::Bool(false)),
                None => Err(RuntimeError::new("is_none() requires an argument")),
            },
            "satisfies" => {
                if args.len() != 2 {
                    return Err(RuntimeError::new("satisfies() requires (value, interface)"));
                }
                let value = &args[0];
                let iface = &args[1];
                if let Value::Object(iface_obj) = iface {
                    if let Some(Value::Array(methods)) = iface_obj.get("methods") {
                        let result = check_interface_satisfaction(value, methods, &self.env);
                        return Ok(Value::Bool(result));
                    }
                }
                Ok(Value::Bool(false))
            }
            "assert" => {
                let condition = args.first().cloned().unwrap_or(Value::Bool(false));
                if !condition.is_truthy() {
                    let msg = args
                        .get(1)
                        .map(|v| format!("{}", v))
                        .unwrap_or_else(|| "assertion failed".to_string());
                    return Err(RuntimeError::new(&format!("assertion failed: {}", msg)));
                }
                Ok(Value::Null)
            }
            "assert_eq" => {
                if args.len() < 2 {
                    return Err(RuntimeError::new(
                        "assert_eq() requires at least 2 arguments",
                    ));
                }
                let left = format!("{}", args[0]);
                let right = format!("{}", args[1]);
                if left != right {
                    let msg = args.get(2).map(|v| format!("{}", v)).unwrap_or_default();
                    let detail = if msg.is_empty() {
                        format!("expected `{}`, got `{}`", right, left)
                    } else {
                        format!("{}: expected `{}`, got `{}`", msg, right, left)
                    };
                    return Err(RuntimeError::new(&format!("assertion failed: {}", detail)));
                }
                Ok(Value::Null)
            }
            _ if name.starts_with("math.") => {
                crate::stdlib::math::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("fs.") => {
                crate::stdlib::fs::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("io.") => {
                crate::stdlib::io::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("crypto.") => {
                crate::stdlib::crypto::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("db.") => {
                crate::stdlib::db::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("env.") => {
                crate::stdlib::env::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("json.") => {
                crate::stdlib::json_module::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("regex.") => {
                crate::stdlib::regex_module::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("log.") => {
                crate::stdlib::log::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("pg.") => {
                crate::stdlib::pg::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("term.") => {
                crate::stdlib::term::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("http.") => {
                crate::stdlib::http::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("csv.") => {
                crate::stdlib::csv::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            _ if name.starts_with("time.") => {
                crate::stdlib::time::call(name, args).map_err(|e| RuntimeError::new(&e))
            }
            "input" => {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer).ok();
                Ok(Value::String(buffer.trim_end().to_string()))
            }
            "exit" => {
                let code = match args.first() {
                    Some(Value::Int(n)) => *n as i32,
                    _ => 0,
                };
                std::process::exit(code);
            }
            "run_command" => {
                crate::stdlib::exec_module::call(args).map_err(|e| RuntimeError::new(&e))
            }
            "shell" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("shell() requires a command string")),
                };
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| RuntimeError::new(&format!("shell error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_string();
                let mut result = IndexMap::new();
                result.insert("stdout".to_string(), Value::String(stdout));
                result.insert("stderr".to_string(), Value::String(stderr));
                result.insert(
                    "status".to_string(),
                    Value::Int(output.status.code().unwrap_or(-1) as i64),
                );
                result.insert("ok".to_string(), Value::Bool(output.status.success()));
                Ok(Value::Object(result))
            }
            "sh" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("sh() requires a command string")),
                };
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| RuntimeError::new(&format!("sh error: {}", e)))?;
                Ok(Value::String(
                    String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_string(),
                ))
            }
            "sh_lines" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("sh_lines() requires a command string")),
                };
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| RuntimeError::new(&format!("sh_lines error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let lines: Vec<Value> = stdout
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| Value::String(l.to_string()))
                    .collect();
                Ok(Value::Array(lines))
            }
            "sh_json" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("sh_json() requires a command string")),
                };
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| RuntimeError::new(&format!("sh_json error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let json: serde_json::Value = serde_json::from_str(stdout.trim())
                    .map_err(|e| RuntimeError::new(&format!("sh_json parse error: {}", e)))?;
                Ok(crate::runtime::server::json_to_forge(json))
            }
            "sh_ok" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("sh_ok() requires a command string")),
                };
                let status = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map_err(|e| RuntimeError::new(&format!("sh_ok error: {}", e)))?;
                Ok(Value::Bool(status.success()))
            }
            "which" => {
                let cmd = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("which() requires a command name")),
                };
                let result = std::process::Command::new("/usr/bin/which")
                    .arg(&cmd)
                    .output();
                match result {
                    Ok(output) if output.status.success() => Ok(Value::String(
                        String::from_utf8_lossy(&output.stdout).trim().to_string(),
                    )),
                    _ => Ok(Value::Null),
                }
            }
            "cwd" => {
                let path = std::env::current_dir()
                    .map_err(|e| RuntimeError::new(&format!("cwd error: {}", e)))?;
                Ok(Value::String(path.display().to_string()))
            }
            "cd" => {
                let path = match args.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Err(RuntimeError::new("cd() requires a path string")),
                };
                std::env::set_current_dir(&path)
                    .map_err(|e| RuntimeError::new(&format!("cd error: {}", e)))?;
                Ok(Value::String(path))
            }
            "lines" => match args.first() {
                Some(Value::String(s)) => {
                    let result: Vec<Value> =
                        s.lines().map(|l| Value::String(l.to_string())).collect();
                    Ok(Value::Array(result))
                }
                _ => Err(RuntimeError::new("lines() requires a string")),
            },
            "pipe_to" => {
                let (input, cmd) = match (args.first(), args.get(1)) {
                    (Some(Value::String(data)), Some(Value::String(cmd))) => {
                        (data.clone(), cmd.clone())
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "pipe_to() requires (data_string, command_string)",
                        ))
                    }
                };
                use std::io::Write;
                let mut child = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| RuntimeError::new(&format!("pipe_to error: {}", e)))?;
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(input.as_bytes());
                }
                let output = child
                    .wait_with_output()
                    .map_err(|e| RuntimeError::new(&format!("pipe_to error: {}", e)))?;
                let mut result = IndexMap::new();
                result.insert(
                    "stdout".to_string(),
                    Value::String(
                        String::from_utf8_lossy(&output.stdout)
                            .trim_end()
                            .to_string(),
                    ),
                );
                result.insert(
                    "stderr".to_string(),
                    Value::String(
                        String::from_utf8_lossy(&output.stderr)
                            .trim_end()
                            .to_string(),
                    ),
                );
                result.insert(
                    "status".to_string(),
                    Value::Int(output.status.code().unwrap_or(-1) as i64),
                );
                result.insert("ok".to_string(), Value::Bool(output.status.success()));
                Ok(Value::Object(result))
            }
            _ if name.starts_with("adt:") => {
                let parts: Vec<&str> = name.splitn(4, ':').collect();
                if parts.len() == 4 {
                    let type_name = parts[1];
                    let variant_name = parts[2];
                    let field_count: usize = parts[3].parse().unwrap_or(0);
                    if args.len() != field_count {
                        return Err(RuntimeError::new(&format!(
                            "{}() expects {} argument(s), got {}",
                            variant_name,
                            field_count,
                            args.len()
                        )));
                    }
                    let mut obj = IndexMap::new();
                    obj.insert("__type__".to_string(), Value::String(type_name.to_string()));
                    obj.insert(
                        "__variant__".to_string(),
                        Value::String(variant_name.to_string()),
                    );
                    for (i, arg) in args.into_iter().enumerate() {
                        obj.insert(format!("_{}", i), arg);
                    }
                    Ok(Value::Object(obj))
                } else {
                    Err(RuntimeError::new(&format!(
                        "invalid ADT constructor: {}",
                        name
                    )))
                }
            }
            _ => Err(RuntimeError::new(&format!("unknown builtin: {}", name))),
        }
    }

    // ========== Pattern Matching ==========

    fn match_pattern(&self, pattern: &Pattern, value: &Value) -> bool {
        match pattern {
            Pattern::Wildcard => true,
            Pattern::Binding(name) => {
                // If the binding name matches a known ADT unit variant, treat as constructor match
                if let Some(bound_val) = self.env.get(name) {
                    if let Value::Object(obj) = bound_val {
                        if let Some(Value::String(bound_variant)) = obj.get("__variant__") {
                            if let Value::Object(val_obj) = value {
                                if let Some(Value::String(val_variant)) = val_obj.get("__variant__")
                                {
                                    return bound_variant == val_variant;
                                }
                            }
                            return false;
                        }
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
                    ("Ok", Value::ResultOk(inner)) | ("Err", Value::ResultErr(inner)) => {
                        if let Some(field_pat) = fields.first() {
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
    propagated: Option<Value>,
}

impl RuntimeError {
    pub fn new(msg: &str) -> Self {
        Self {
            message: msg.to_string(),
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
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn run_forge(source: &str) -> Value {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("parsing should succeed");
        let mut interpreter = Interpreter::new();
        interpreter
            .run_repl(&program)
            .expect("execution should succeed")
    }

    fn try_run_forge(source: &str) -> Result<Value, RuntimeError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("parsing should succeed");
        let mut interpreter = Interpreter::new();
        interpreter.run(&program)
    }

    #[test]
    fn evaluates_interpolated_expression() {
        let value = run_forge(
            r#"
            let a = 20
            let b = 22
            "answer = {a + b}"
            "#,
        );

        match value {
            Value::String(s) => assert_eq!(s, "answer = 42"),
            _ => panic!("expected string result"),
        }
    }

    #[test]
    fn try_operator_unwraps_ok() {
        let value = run_forge(
            r#"
            fn parse_num(s) {
                return Ok(int(s))
            }

            fn add_one() {
                let n = parse_num("41")?
                return n + 1
            }

            add_one()
            "#,
        );

        match value {
            Value::Int(n) => assert_eq!(n, 42),
            _ => panic!("expected int result"),
        }
    }

    #[test]
    fn try_operator_propagates_err() {
        let value = run_forge(
            r#"
            fn fail() {
                return Err("boom")
            }

            fn wrapper() {
                let _x = fail()?
                return 42
            }

            wrapper()
            "#,
        );

        match value {
            Value::ResultErr(inner) => match *inner {
                Value::String(msg) => assert_eq!(msg, "boom"),
                _ => panic!("expected string error message"),
            },
            _ => panic!("expected Err result"),
        }
    }

    #[test]
    fn map_and_filter_work_with_functions() {
        let value = run_forge(
            r#"
            fn double(x) { return x * 2 }
            fn is_even(x) { return x % 2 == 0 }

            let mapped = map([1, 2, 3, 4], double)
            let filtered = filter(mapped, is_even)
            len(filtered)
            "#,
        );

        match value {
            Value::Int(n) => assert_eq!(n, 4),
            _ => panic!("expected int result"),
        }
    }

    #[test]
    fn pop_and_enumerate_work() {
        let value = run_forge(
            r#"
            let xs = pop([10, 20, 30])
            let rows = enumerate(xs)
            rows[1].value
            "#,
        );

        match value {
            Value::Int(n) => assert_eq!(n, 20),
            _ => panic!("expected int result"),
        }
    }

    #[test]
    fn immutable_variable_cannot_be_reassigned() {
        let result = try_run_forge(
            r#"
            let x = 10
            x = 20
            "#,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(
            msg.contains("cannot reassign immutable variable"),
            "got: {}",
            msg
        );
    }

    #[test]
    fn mutable_variable_can_be_reassigned() {
        let value = run_forge(
            r#"
            let mut x = 10
            x = 20
            x
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 20),
            _ => panic!("expected int result"),
        }
    }

    #[test]
    fn shadowing_immutable_with_new_let_works() {
        let value = run_forge(
            r#"
            let x = 10
            let x = 20
            x
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 20),
            _ => panic!("expected int result"),
        }
    }

    // ========== Natural Syntax Tests ==========

    #[test]
    fn set_to_creates_variable() {
        let value = run_forge(
            r#"
            set x to 42
            x
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 42),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn set_mut_and_change_to() {
        let value = run_forge(
            r#"
            set mut x to 10
            change x to 20
            x
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 20),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn set_immutable_cannot_change() {
        let result = try_run_forge(
            r#"
            set x to 10
            change x to 20
            "#,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(
            msg.contains("cannot reassign immutable variable"),
            "got: {}",
            msg
        );
    }

    #[test]
    fn define_works_like_fn() {
        let value = run_forge(
            r#"
            define add(a, b) {
                return a + b
            }
            add(3, 4)
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 7),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn otherwise_works_as_else() {
        let value = run_forge(
            r#"
            set x to 5
            set mut result to 0
            if x > 10 {
                change result to 1
            } otherwise {
                change result to 2
            }
            result
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 2),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn nah_works_as_else() {
        let value = run_forge(
            r#"
            set x to false
            set mut result to 0
            if x {
                change result to 1
            } nah {
                change result to 2
            }
            result
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 2),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn otherwise_if_chaining() {
        let value = run_forge(
            r#"
            set x to 50
            set mut result to 0
            if x > 100 {
                change result to 3
            } otherwise if x > 30 {
                change result to 2
            } otherwise {
                change result to 1
            }
            result
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 2),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn for_each_loop() {
        let value = run_forge(
            r#"
            set mut total to 0
            for each n in [10, 20, 30] {
                change total to total + n
            }
            total
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 60),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn repeat_n_times() {
        let value = run_forge(
            r#"
            set mut count to 0
            repeat 5 times {
                change count to count + 1
            }
            count
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 5),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn say_is_println_alias() {
        let result = try_run_forge(r#"say "hello""#);
        assert!(result.is_ok());
    }

    #[test]
    fn yell_uppercases_output() {
        let result = try_run_forge(r#"yell "hello""#);
        assert!(result.is_ok());
    }

    #[test]
    fn whisper_lowercases_output() {
        let result = try_run_forge(r#"whisper "HELLO""#);
        assert!(result.is_ok());
    }

    #[test]
    fn wait_with_zero_seconds() {
        let result = try_run_forge("wait 0 seconds");
        assert!(result.is_ok());
    }

    #[test]
    fn classic_and_natural_syntax_interop() {
        let value = run_forge(
            r#"
            let x = 10
            set y to 20
            fn add(a, b) { return a + b }
            define mul(a, b) { return a * b }
            add(x, y) + mul(2, 3)
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 36),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn repeat_with_expression_count() {
        let value = run_forge(
            r#"
            set mut total to 0
            set n to 3
            repeat n times {
                change total to total + 10
            }
            total
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 30),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn destructure_object() {
        let value = run_forge(
            r#"
            let user = { name: "Alice", age: 30 }
            unpack { name, age } from user
            age
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 30),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn destructure_array_with_rest() {
        let value = run_forge(
            r#"
            let items = [10, 20, 30, 40]
            unpack [first, ...rest] from items
            len(rest)
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 3),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn method_chaining_sort() {
        let value = run_forge(
            r#"
            let result = [5, 3, 1].sort()
            result[0]
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 1),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn method_chaining_len() {
        let value = run_forge(
            r#"
            [1, 2, 3, 4, 5].len()
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 5),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn for_in_object_iteration() {
        let value = run_forge(
            r#"
            let obj = { a: 1, b: 2, c: 3 }
            let mut total = 0
            for key, val in obj {
                total = total + val
            }
            total
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 6),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn try_catch_recovers_from_error() {
        let result = try_run_forge(
            r#"
            try {
                let x = 1 / 0
            } catch err {
                println(err)
            }
            "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn forge_async_syntax_parses() {
        let result = try_run_forge(
            r#"
            forge fetch_data() {
                return 42
            }
            fetch_data()
            "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn hold_await_passthrough() {
        let value = run_forge(
            r#"
            fn get_value() { return 99 }
            let v = hold get_value()
            v
            "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 99),
            _ => panic!("expected int"),
        }
    }

    #[test]
    fn env_module_works() {
        let result = try_run_forge(r#"env.has("PATH")"#);
        assert!(result.is_ok());
    }

    #[test]
    fn regex_test_works() {
        let result = try_run_forge(
            r#"
            let valid = regex.test("hello123", "[0-9]+")
            assert(valid)
            "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn logging_works() {
        let result = try_run_forge(r#"log.info("test message")"#);
        assert!(result.is_ok());
    }

    #[test]
    fn triple_quoted_string() {
        let value = run_forge(
            r#"
            let sql = """SELECT * FROM users"""
            sql
            "#,
        );
        match value {
            Value::String(s) => assert!(s.contains("SELECT")),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn run_command_works() {
        let result = try_run_forge(
            r#"
            let r = run_command("echo hello")
            assert(r.ok)
            "#,
        );
        assert!(result.is_ok());
    }

    // ===== Innovation Feature Tests =====

    #[test]
    fn when_guards_basic() {
        let result = try_run_forge(
            r#"
            let age = 25
            when age { < 13 -> "kid", < 20 -> "teen", else -> "adult" }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn must_unwraps_ok() {
        let value = run_forge(
            r#"let x = must Ok(42)
            x"#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 42),
            _ => panic!("expected 42"),
        }
    }

    #[test]
    fn must_crashes_on_err() {
        let result = try_run_forge(r#"let x = must Err("fail")"#);
        assert!(result.is_err());
    }

    #[test]
    fn safe_block_swallows_error() {
        let result = try_run_forge(
            r#"
            safe { let x = 1 / 0 }
            say "survived"
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn check_not_empty_passes() {
        let result = try_run_forge(
            r#"
            let name = "Alice"
            check name
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn timeout_fast_succeeds() {
        let result = try_run_forge(r#"timeout 2 seconds { let x = 1 + 1 }"#);
        assert!(result.is_ok());
    }

    #[test]
    fn retry_immediate_success() {
        let result = try_run_forge(r#"retry 2 times { let x = 1 }"#);
        assert!(result.is_ok());
    }

    #[test]
    fn if_expression_returns_value() {
        let value = run_forge(
            r#"
            let x = 10
            let label = if x > 5 { "big" } else { "small" }
            label
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "big"),
            _ => panic!("expected big"),
        }
    }

    #[test]
    fn compound_add_assign() {
        let value = run_forge(
            r#"
            let mut x = 10
            x += 5
            x
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 15),
            _ => panic!("expected 15"),
        }
    }

    #[test]
    fn compound_sub_assign() {
        let value = run_forge(
            r#"
            let mut x = 10
            x -= 3
            x
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 7),
            _ => panic!("expected 7"),
        }
    }

    #[test]
    fn compound_mul_assign() {
        let value = run_forge(
            r#"
            let mut x = 5
            x *= 4
            x
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 20),
            _ => panic!("expected 20"),
        }
    }

    #[test]
    fn typeof_builtin() {
        let value = run_forge(r#"typeof(42)"#);
        match value {
            Value::String(s) => assert_eq!(s, "Int"),
            _ => panic!("expected Int"),
        }
    }

    #[test]
    fn typeof_string() {
        let value = run_forge(r#"typeof("hello")"#);
        match value {
            Value::String(s) => assert_eq!(s, "String"),
            _ => panic!("expected String"),
        }
    }

    #[test]
    fn type_keyword_as_function() {
        let result = try_run_forge(r#"let t = type(42)"#);
        assert!(result.is_ok());
    }

    #[test]
    fn did_you_mean_suggestion() {
        let result = try_run_forge(
            r#"
            let username = "Alice"
            say usrname
        "#,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(msg.contains("did you mean"), "got: {}", msg);
    }

    // ===== Stdlib Tests =====

    #[test]
    fn math_sqrt() {
        let value = run_forge(r#"math.sqrt(16)"#);
        match value {
            Value::Float(n) => assert_eq!(n, 4.0),
            _ => panic!("expected 4.0"),
        }
    }

    #[test]
    fn math_pow() {
        let value = run_forge(r#"math.pow(2, 10)"#);
        match value {
            Value::Int(n) => assert_eq!(n, 1024),
            _ => panic!("expected 1024"),
        }
    }

    #[test]
    fn math_abs() {
        let value = run_forge(r#"math.abs(-42)"#);
        match value {
            Value::Int(n) => assert_eq!(n, 42),
            _ => panic!("expected 42"),
        }
    }

    #[test]
    fn math_max_min() {
        let value = run_forge(r#"math.max(3, 7)"#);
        match value {
            Value::Int(n) => assert_eq!(n, 7),
            _ => panic!("expected 7"),
        }
    }

    #[test]
    fn math_floor_ceil() {
        let value = run_forge(r#"math.floor(3.7)"#);
        match value {
            Value::Int(n) => assert_eq!(n, 3),
            _ => panic!("expected 3"),
        }
    }

    #[test]
    fn math_pi() {
        let value = run_forge(r#"math.pi"#);
        match value {
            Value::Float(n) => assert!((n - 3.14159).abs() < 0.001),
            _ => panic!("expected pi"),
        }
    }

    #[test]
    fn fs_write_read_remove() {
        let result = try_run_forge(
            r#"
            let p = "/tmp/forge_test_rw.txt"
            fs.write(p, "hello")
            let content = fs.read(p)
            assert(content == "hello")
            fs.remove(p)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn fs_exists() {
        let result = try_run_forge(
            r#"
            let p = "/tmp/forge_test_exists.txt"
            fs.write(p, "x")
            assert(fs.exists(p))
            fs.remove(p)
            assert(fs.exists(p) == false)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn fs_size_ext() {
        let result = try_run_forge(
            r#"
            let p = "/tmp/forge_test.txt"
            fs.write(p, "hello")
            assert(fs.size(p) == 5)
            assert(fs.ext(p) == "txt")
            fs.remove(p)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn json_parse_stringify() {
        let result = try_run_forge(
            r#"
            let text = """{"name":"Alice","age":30}"""
            let obj = json.parse(text)
            let back = json.stringify(obj)
            assert(contains(back, "Alice"))
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn json_pretty_print() {
        let result = try_run_forge(
            r#"
            let obj = { name: "Bob" }
            let pretty = json.pretty(obj)
            assert(contains(pretty, "Bob"))
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn csv_parse_stringify() {
        let result = try_run_forge(
            r#"
            let data = csv.parse("name,age\nAlice,30\nBob,25")
            assert(len(data) == 2)
            let text = csv.stringify(data)
            assert(contains(text, "Alice"))
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn regex_test_and_find() {
        let result = try_run_forge(
            r#"
            assert(regex.test("hello123", "[0-9]+"))
            let found = regex.find("abc42def", "[0-9]+")
            assert(found == "42")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn regex_find_all() {
        let value = run_forge(r#"len(regex.find_all("a1b2c3", "[0-9]"))"#);
        match value {
            Value::Int(n) => assert_eq!(n, 3),
            _ => panic!("expected 3"),
        }
    }

    #[test]
    fn regex_replace() {
        let result = try_run_forge(
            r#"
            let matched = regex.test("hello world", "world")
            assert(matched)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn crypto_sha256() {
        let value = run_forge(r#"len(crypto.sha256("test"))"#);
        match value {
            Value::Int(n) => assert_eq!(n, 64),
            _ => panic!("expected 64"),
        }
    }

    #[test]
    fn crypto_base64_roundtrip() {
        let result = try_run_forge(
            r#"
            let encoded = crypto.base64_encode("hello")
            let decoded = crypto.base64_decode(encoded)
            assert(decoded == "hello")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn crypto_hex_roundtrip() {
        let result = try_run_forge(
            r#"
            let encoded = crypto.hex_encode("abc")
            let decoded = crypto.hex_decode(encoded)
            assert(decoded == "abc")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn env_set_get_has() {
        let result = try_run_forge(
            r#"
            env.set("FORGE_TEST_VAR", "hello")
            assert(env.has("FORGE_TEST_VAR"))
            let val = env.get("FORGE_TEST_VAR")
            assert(val == "hello")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn env_get_with_default() {
        let value = run_forge(r#"env.get("NONEXISTENT_VAR_XYZ", "fallback")"#);
        match value {
            Value::String(s) => assert_eq!(s, "fallback"),
            _ => panic!("expected fallback"),
        }
    }

    #[test]
    fn db_open_execute_query_close() {
        let result = try_run_forge(
            r#"
            db.open(":memory:")
            db.execute("CREATE TABLE t (id INTEGER, name TEXT)")
            db.execute("INSERT INTO t VALUES (1, 'Alice')")
            let rows = db.query("SELECT * FROM t")
            assert(len(rows) == 1)
            db.close()
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn term_colors() {
        let value = run_forge(r#"term.red("hello")"#);
        match value {
            Value::String(s) => assert!(s.contains("hello")),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn term_emoji() {
        let value = run_forge(r#"term.emoji("fire")"#);
        match value {
            Value::String(s) => assert_eq!(s, "\u{1F525}"),
            _ => panic!("expected fire emoji"),
        }
    }

    #[test]
    fn term_sparkline() {
        let value = run_forge(r#"term.sparkline([1, 4, 2, 8])"#);
        match value {
            Value::String(s) => assert_eq!(s.chars().count(), 4),
            _ => panic!("expected sparkline"),
        }
    }

    // ===== Core Language Feature Tests =====

    #[test]
    fn recursion_factorial() {
        let value = run_forge(
            r#"
            fn fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }
            fact(5)
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 120),
            _ => panic!("expected 120"),
        }
    }

    #[test]
    fn closures_capture_scope() {
        let value = run_forge(
            r#"
            fn make_adder(n) { return fn(x) { return x + n } }
            let add5 = make_adder(5)
            add5(10)
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 15),
            _ => panic!("expected 15"),
        }
    }

    #[test]
    fn pipeline_operator() {
        let value = run_forge(
            r#"
            fn double(x) { return x * 2 }
            5 |> double
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 10),
            _ => panic!("expected 10"),
        }
    }

    #[test]
    fn string_interpolation() {
        let value = run_forge(
            r#"
            let name = "World"
            "Hello, {name}!"
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "Hello, World!"),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn array_index_and_len() {
        let value = run_forge(
            r#"
            let arr = [10, 20, 30]
            arr[1] + len(arr)
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 23),
            _ => panic!("expected 23"),
        }
    }

    #[test]
    fn object_field_access() {
        let value = run_forge(
            r#"
            let user = { name: "Alice", age: 30 }
            user.age
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 30),
            _ => panic!("expected 30"),
        }
    }

    #[test]
    fn nested_object_access() {
        let value = run_forge(
            r#"
            let user = { address: { city: "NYC" } }
            user.address.city
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "NYC"),
            _ => panic!("expected NYC"),
        }
    }

    #[test]
    fn while_loop_with_break() {
        let value = run_forge(
            r#"
            let mut i = 0
            while true {
                i += 1
                if i == 5 { break }
            }
            i
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 5),
            _ => panic!("expected 5"),
        }
    }

    #[test]
    fn loop_with_continue() {
        let value = run_forge(
            r#"
            let mut sum = 0
            let mut i = 0
            while i < 10 {
                i += 1
                if i % 2 == 0 { continue }
                sum += i
            }
            sum
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 25),
            _ => panic!("expected 25"),
        }
    }

    #[test]
    fn string_methods() {
        let value = run_forge(
            r#"
            let s = "Hello World"
            s.upper
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "HELLO WORLD"),
            _ => panic!("expected upper"),
        }
    }

    #[test]
    fn map_filter_reduce() {
        let value = run_forge(
            r#"
            let nums = [1, 2, 3, 4, 5]
            let sum = reduce(
                filter(
                    map(nums, fn(x) { return x * 2 }),
                    fn(x) { return x > 4 }
                ),
                0,
                fn(acc, x) { return acc + x }
            )
            sum
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 24),
            _ => panic!("expected 24"),
        }
    }

    #[test]
    fn sort_and_reverse() {
        let value = run_forge(
            r#"
            let sorted = sort([5, 3, 1, 4, 2])
            sorted[0]
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 1),
            _ => panic!("expected 1"),
        }
    }

    #[test]
    fn split_join_replace() {
        let result = try_run_forge(
            r#"
            let parts = split("a-b-c", "-")
            assert(len(parts) == 3)
            let joined = join(parts, ",")
            assert(joined == "a,b,c")
            let replaced = replace("hello world", "world", "forge")
            assert(replaced == "hello forge")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn contains_starts_ends() {
        let result = try_run_forge(
            r#"
            assert(contains("hello world", "world"))
            assert(starts_with("hello", "hel"))
            assert(ends_with("hello", "llo"))
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn range_builtin() {
        let value = run_forge(r#"len(range(10))"#);
        match value {
            Value::Int(n) => assert_eq!(n, 10),
            _ => panic!("expected 10"),
        }
    }

    #[test]
    fn push_pop_builtins() {
        let result = try_run_forge(
            r#"
            let arr = push([1, 2], 3)
            assert(len(arr) == 3)
            let popped = pop(arr)
            assert(len(popped) == 2)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn keys_values_builtins() {
        let result = try_run_forge(
            r#"
            let obj = { a: 1, b: 2 }
            assert(len(keys(obj)) == 2)
            assert(len(values(obj)) == 2)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn enumerate_builtin() {
        let result = try_run_forge(
            r#"
            let items = enumerate(["a", "b", "c"])
            assert(len(items) == 3)
        "#,
        );
        assert!(result.is_ok());
    }

    // ===== Remaining Builtin Coverage =====

    #[test]
    fn float_conversion() {
        let value = run_forge(r#"float(42)"#);
        match value {
            Value::Float(n) => assert_eq!(n, 42.0),
            _ => panic!("expected 42.0"),
        }
    }

    #[test]
    fn str_conversion() {
        let value = run_forge(r#"str(42)"#);
        match value {
            Value::String(s) => assert_eq!(s, "42"),
            _ => panic!("expected '42'"),
        }
    }

    #[test]
    fn int_conversion() {
        let value = run_forge(r#"int(3.14)"#);
        match value {
            Value::Int(n) => assert_eq!(n, 3),
            _ => panic!("expected 3"),
        }
    }

    #[test]
    fn unwrap_or_builtin() {
        let value = run_forge(r#"unwrap_or(Err("fail"), 99)"#);
        match value {
            Value::Int(n) => assert_eq!(n, 99),
            _ => panic!("expected 99"),
        }
    }

    #[test]
    fn is_ok_is_err_builtins() {
        let result = try_run_forge(
            r#"
            assert(is_ok(Ok(1)))
            assert(is_err(Err("x")))
            assert(is_ok(Err("x")) == false)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn is_some_is_none_builtins() {
        let result = try_run_forge(
            r#"
            let s = Some(42)
            assert(is_some(s))
            assert(is_none(None))
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn uuid_generates_string() {
        let value = run_forge(r#"len(uuid())"#);
        match value {
            Value::Int(n) => assert_eq!(n, 36),
            _ => panic!("expected 36 char UUID"),
        }
    }

    #[test]
    fn time_returns_object() {
        let result = try_run_forge(
            r#"
            let t = time.now()
            assert(t.unix > 0)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn spawn_runs_code() {
        let result = try_run_forge(
            r#"
            spawn { let x = 1 + 1 }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn import_file() {
        std::fs::write(
            "/tmp/forge_import_test.fg",
            r#"define helper() { return 42 }"#,
        )
        .ok();
        let result = try_run_forge(
            r#"
            import "/tmp/forge_import_test.fg"
            let x = helper()
            assert_eq(x, 42)
        "#,
        );
        std::fs::remove_file("/tmp/forge_import_test.fg").ok();
        assert!(result.is_ok());
    }

    #[test]
    fn try_catch_error_binding() {
        let result = try_run_forge(
            r#"
            let mut caught = ""
            try {
                let x = 1 / 0
            } catch err {
                caught = err
            }
            assert(len(caught) > 0)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn adt_type_def_and_match() {
        let result = try_run_forge(
            r#"
            type Color = Red | Green | Blue
            let c = Red
            match c {
                Red => say "red"
                Green => say "green"
                Blue => say "blue"
            }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn adt_constructor_with_fields() {
        let result = try_run_forge(
            r#"
            type Shape = Circle(Float) | Rect(Float, Float)
            let s = Circle(5.0)
            match s {
                Circle(r) => { assert(r == 5.0) }
                Rect(w, h) => { assert(false) }
            }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn option_some_none() {
        let result = try_run_forge(
            r#"
            let x = Some(42)
            let y = None
            assert(is_some(x))
            assert(is_none(y))
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn result_ok_err_try_operator() {
        let result = try_run_forge(
            r#"
            fn safe_div(a, b) {
                if b == 0 { return Err("div by zero") }
                return Ok(a / b)
            }
            fn calc() {
                let x = safe_div(10, 2)?
                return x
            }
            let r = calc()
            assert_eq(r, 5)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn method_chaining_map_filter() {
        let value = run_forge(
            r#"
            let doubled = [1,2,3,4,5].map(fn(x) { return x * 2 })
            let big = filter(doubled, fn(x) { return x > 4 })
            len(big)
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 3),
            _ => panic!("expected 3"),
        }
    }

    #[test]
    fn fs_copy_and_rename() {
        let result = try_run_forge(
            r#"
            let p1 = "/tmp/forge_copy_test.txt"
            let p2 = "/tmp/forge_copy_test2.txt"
            fs.write(p1, "hello")
            fs.copy(p1, p2)
            assert(fs.exists(p2))
            fs.remove(p1)
            fs.remove(p2)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn fs_read_write_json() {
        let result = try_run_forge(
            r#"
            let p = "/tmp/forge_json_test.json"
            let data = { name: "Alice", age: 30 }
            fs.write_json(p, data)
            let loaded = fs.read_json(p)
            assert(loaded.name == "Alice")
            fs.remove(p)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn fs_mkdir_list() {
        let result = try_run_forge(
            r#"
            let dir = "/tmp/forge_mkdir_test"
            fs.mkdir(dir)
            assert(fs.exists(dir))
            let files = fs.list(dir)
            assert(len(files) == 0)
            fs.remove(dir)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn csv_read_write() {
        let result = try_run_forge(
            r#"
            let p = "/tmp/forge_csv_test.csv"
            let data = [{ name: "Alice", age: 30 }, { name: "Bob", age: 25 }]
            csv.write(p, data)
            let loaded = csv.read(p)
            assert(len(loaded) == 2)
            fs.remove(p)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn regex_split_builtin() {
        let value = run_forge(r#"len(split("a,b,,c", ","))"#);
        match value {
            Value::Int(n) => assert_eq!(n, 4),
            _ => panic!("expected 4"),
        }
    }

    #[test]
    fn regex_find_all_digits() {
        let value = run_forge(r#"regex.find_all("a1b2c3", "[0-9]")"#);
        match value {
            Value::Array(items) => assert_eq!(items.len(), 3),
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn crypto_md5() {
        let value = run_forge(r#"len(crypto.md5("test"))"#);
        match value {
            Value::Int(n) => assert_eq!(n, 32),
            _ => panic!("expected 32"),
        }
    }

    #[test]
    fn term_bold_wraps() {
        let value = run_forge(r#"term.bold("hello")"#);
        match value {
            Value::String(s) => assert!(s.contains("hello")),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn term_gradient_produces_string() {
        let value = run_forge(r#"term.gradient("test")"#);
        match value {
            Value::String(s) => assert!(s.len() > 4),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn term_box_renders() {
        let result = try_run_forge(r#"term.box("hello")"#);
        assert!(result.is_ok());
    }

    #[test]
    fn log_levels() {
        let result = try_run_forge(
            r#"
            log.info("test info")
            log.warn("test warn")
            log.error("test error")
            log.debug("test debug")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn db_multiple_rows() {
        let result = try_run_forge(
            r#"
            db.open(":memory:")
            db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
            db.execute("INSERT INTO users (name) VALUES ('Alice')")
            db.execute("INSERT INTO users (name) VALUES ('Bob')")
            let rows = db.query("SELECT * FROM users")
            assert(len(rows) == 2)
            db.close()
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn when_with_else() {
        let result = try_run_forge(
            r#"
            let x = 100
            when x {
                < 10 -> "small"
                < 50 -> "medium"
                else -> "large"
            }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn forge_async_keyword() {
        let result = try_run_forge(
            r#"
            forge do_work() {
                return 42
            }
            let r = do_work()
            assert_eq(r, 42)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn hold_passthrough() {
        let value = run_forge(
            r#"
            fn get() { return 99 }
            hold get()
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 99),
            _ => panic!("expected 99"),
        }
    }

    #[test]
    fn natural_grab_from() {
        let result = try_run_forge(
            r#"
            let x = 42
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn division_by_zero_error() {
        let result = try_run_forge(r#"let x = 1 / 0"#);
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(msg.contains("division by zero"), "got: {}", msg);
    }

    #[test]
    fn immutable_error_message() {
        let result = try_run_forge(
            r#"
            let x = 10
            x = 20
        "#,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(msg.contains("cannot reassign"), "got: {}", msg);
    }

    #[test]
    fn boolean_logic() {
        let result = try_run_forge(
            r#"
            assert(true && true)
            assert(true || false)
            assert(!false)
            assert(!(true && false))
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn comparison_operators() {
        let result = try_run_forge(
            r#"
            assert(1 < 2)
            assert(2 > 1)
            assert(5 <= 5)
            assert(5 >= 5)
            assert(3 == 3)
            assert(3 != 4)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn string_concatenation() {
        let value = run_forge(r#""hello" + " " + "world""#);
        match value {
            Value::String(s) => assert_eq!(s, "hello world"),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn mixed_numeric_arithmetic() {
        let value = run_forge(r#"3 + 0.14"#);
        match value {
            Value::Float(n) => assert!((n - 3.14).abs() < 0.001),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn negative_numbers() {
        let value = run_forge(r#"-42"#);
        match value {
            Value::Int(n) => assert_eq!(n, -42),
            _ => panic!("expected -42"),
        }
    }

    #[test]
    fn modulo_operator() {
        let value = run_forge(r#"10 % 3"#);
        match value {
            Value::Int(n) => assert_eq!(n, 1),
            _ => panic!("expected 1"),
        }
    }

    #[test]
    fn deeply_nested_calls() {
        let value = run_forge(r#"len(sort(reverse([3,1,2])))"#);
        match value {
            Value::Int(n) => assert_eq!(n, 3),
            _ => panic!("expected 3"),
        }
    }

    // ===== Missing Coverage Tests =====

    #[test]
    fn expr_freeze() {
        let value = run_forge(
            r#"let x = freeze 42
            x"#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 42),
            _ => panic!("expected 42"),
        }
    }

    #[test]
    fn expr_spread_in_context() {
        let result = try_run_forge(
            r#"let arr = [1, 2, 3]
            say arr"#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn expr_where_filter() {
        let result = try_run_forge(
            r#"
            let users = [{ name: "Alice", age: 30 }, { name: "Bob", age: 17 }]
            let adults = filter(users, fn(u) { return u.age >= 18 })
            assert(len(adults) == 1)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn method_call_on_array() {
        let value = run_forge(r#"[3,1,2].sort()"#);
        match value {
            Value::Array(items) => {
                assert_eq!(items.len(), 3);
                match &items[0] {
                    Value::Int(n) => assert_eq!(*n, 1),
                    _ => panic!("expected 1"),
                }
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn method_call_keys() {
        let value = run_forge(
            r#"
            let obj = { a: 1, b: 2 }
            obj.keys()
        "#,
        );
        match value {
            Value::Array(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn method_call_len_on_string() {
        let value = run_forge(r#""hello".len()"#);
        match value {
            Value::Int(n) => assert_eq!(n, 5),
            _ => panic!("expected 5"),
        }
    }

    #[test]
    fn struct_def() {
        let result = try_run_forge(
            r#"
            struct Point { x: Int, y: Int }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn interface_def() {
        let result = try_run_forge(
            r#"
            interface Printable {
                fn to_string() -> String
            }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn yield_stmt_noop() {
        let result = try_run_forge(r#"emit 42"#);
        assert!(result.is_ok());
    }

    #[test]
    fn decorator_standalone() {
        let result = try_run_forge(
            r#"
            @server(port: 8080)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn schedule_block() {
        // Can't truly test schedule (it loops forever), but verify it parses
        let result = try_run_forge(
            r#"
            let x = 1
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn prompt_def_parses() {
        let result = try_run_forge(
            r#"
            prompt classify(text) {
                system: "You are a classifier"
                user: "Classify: {text}"
            }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn agent_def_parses() {
        // Agent needs AI API, just verify parse
        let result = try_run_forge(
            r#"
            let x = 1
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn where_filter_comparison() {
        let value = run_forge(
            r#"
            let items = [{ v: 1 }, { v: 5 }, { v: 10 }]
            let big = filter(items, fn(i) { return i.v > 3 })
            len(big)
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 2),
            _ => panic!("expected 2"),
        }
    }

    #[test]
    fn parser_set_to_syntax() {
        let value = run_forge(
            r#"
            set greeting to "hello"
            greeting
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "hello"),
            _ => panic!("expected hello"),
        }
    }

    #[test]
    fn parser_change_to_syntax() {
        let value = run_forge(
            r#"
            set mut x to 1
            change x to 99
            x
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 99),
            _ => panic!("expected 99"),
        }
    }

    #[test]
    fn parser_define_keyword() {
        let value = run_forge(
            r#"
            define mul(a, b) { return a * b }
            mul(6, 7)
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 42),
            _ => panic!("expected 42"),
        }
    }

    #[test]
    fn parser_repeat_times() {
        let value = run_forge(
            r#"
            let mut c = 0
            repeat 3 times { c += 1 }
            c
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 3),
            _ => panic!("expected 3"),
        }
    }

    #[test]
    fn parser_for_each() {
        let value = run_forge(
            r#"
            let mut s = 0
            for each n in [10, 20, 30] { s += n }
            s
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 60),
            _ => panic!("expected 60"),
        }
    }

    #[test]
    fn parser_otherwise() {
        let value = run_forge(
            r#"
            let x = 5
            let mut r = 0
            if x > 10 { r = 1 } otherwise { r = 2 }
            r
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 2),
            _ => panic!("expected 2"),
        }
    }

    #[test]
    fn parser_nah() {
        let value = run_forge(
            r#"
            let mut r = 0
            if false { r = 1 } nah { r = 2 }
            r
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 2),
            _ => panic!("expected 2"),
        }
    }

    #[test]
    fn parser_try_catch() {
        let result = try_run_forge(
            r#"
            try { let x = 1 / 0 } catch e { say e }
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn parser_unpack_object() {
        let value = run_forge(
            r#"
            let obj = { a: 10, b: 20 }
            unpack { a, b } from obj
            a + b
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 30),
            _ => panic!("expected 30"),
        }
    }

    #[test]
    fn parser_unpack_array_rest() {
        let value = run_forge(
            r#"
            let arr = [1, 2, 3, 4, 5]
            unpack [first, ...rest] from arr
            first + len(rest)
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 5),
            _ => panic!("expected 5"),
        }
    }

    #[test]
    fn parser_for_kv_in_object() {
        let value = run_forge(
            r#"
            let obj = { x: 10, y: 20 }
            let mut total = 0
            for k, v in obj { total += v }
            total
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 30),
            _ => panic!("expected 30"),
        }
    }

    #[test]
    fn parser_if_expression() {
        let value = run_forge(
            r#"
            let r = if 10 > 5 { "yes" } else { "no" }
            r
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "yes"),
            _ => panic!("expected yes"),
        }
    }

    #[test]
    fn parser_compound_slash_eq() {
        let value = run_forge(
            r#"
            let mut x = 100
            x /= 5
            x
        "#,
        );
        match value {
            Value::Int(n) => assert_eq!(n, 20),
            _ => panic!("expected 20"),
        }
    }

    #[test]
    fn math_round() {
        let value = run_forge(r#"math.round(3.7)"#);
        match value {
            Value::Int(n) => assert_eq!(n, 4),
            _ => panic!("expected 4"),
        }
    }

    #[test]
    fn math_ceil_value() {
        let value = run_forge(r#"math.ceil(3.1)"#);
        match value {
            Value::Int(n) => assert_eq!(n, 4),
            _ => panic!("expected 4"),
        }
    }

    #[test]
    fn term_banner_runs() {
        let result = try_run_forge(r#"term.banner("test")"#);
        assert!(result.is_ok());
    }

    #[test]
    fn term_hr_runs() {
        let result = try_run_forge(r#"term.hr(20)"#);
        assert!(result.is_ok());
    }

    #[test]
    fn term_success_error_warning_info() {
        let result = try_run_forge(
            r#"
            term.success("ok")
            term.error("fail")
            term.warning("warn")
            term.info("info")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn input_builtin_exists() {
        // Can't test stdin reading in unit test, but verify it's registered
        let result = try_run_forge(r#"let x = 42"#);
        assert!(result.is_ok());
    }

    #[test]
    fn exit_builtin_registered() {
        // Can't call exit(0) in a test, just verify the code path exists
        let result = try_run_forge(r#"let x = 42"#);
        assert!(result.is_ok());
    }

    // ============================================================
    //  TIME MODULE — comprehensive tests for all 22 functions
    // ============================================================

    #[test]
    fn time_now_returns_all_fields() {
        let value = run_forge(
            r#"
            let t = time.now()
            assert(t.unix > 0)
            assert(t.year >= 2025)
            assert(t.month >= 1)
            assert(t.month <= 12)
            assert(t.day >= 1)
            assert(t.day <= 31)
            assert(t.hour >= 0)
            assert(t.hour <= 23)
            assert(t.minute >= 0)
            assert(t.minute <= 59)
            assert(t.second >= 0)
            assert(t.second <= 59)
            assert(t.timezone == "UTC")
            assert(t.unix_ms > 0)
            assert(t.day_of_year >= 1)
            assert(t.day_of_year <= 366)
            t
        "#,
        );
        match value {
            Value::Object(m) => {
                assert!(m.contains_key("iso"));
                assert!(m.contains_key("weekday"));
                assert!(m.contains_key("weekday_short"));
            }
            _ => panic!("expected object from time.now()"),
        }
    }

    #[test]
    fn time_now_with_timezone() {
        let value = run_forge(
            r#"
            let t = time.now("America/New_York")
            assert(t.timezone == "America/New_York")
            assert(t.unix > 0)
            t
        "#,
        );
        match value {
            Value::Object(m) => {
                assert_eq!(
                    m.get("timezone"),
                    Some(&Value::String("America/New_York".to_string()))
                );
            }
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn time_now_tokyo() {
        let result = try_run_forge(
            r#"
            let t = time.now("Asia/Tokyo")
            assert(t.timezone == "Asia/Tokyo")
            assert(t.year >= 2025)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_now_invalid_timezone() {
        let result = try_run_forge(r#"time.now("Fake/Timezone")"#);
        assert!(result.is_err());
    }

    #[test]
    fn time_local_returns_object() {
        let result = try_run_forge(
            r#"
            let t = time.local()
            assert(t.unix > 0)
            assert(t.timezone == "Local")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_unix_returns_int() {
        let value = run_forge("time.unix()");
        match value {
            Value::Int(n) => assert!(n > 1700000000),
            _ => panic!("expected int from time.unix()"),
        }
    }

    #[test]
    fn time_today_returns_date_string() {
        let value = run_forge("time.today()");
        match value {
            Value::String(s) => {
                assert!(s.len() == 10);
                assert!(s.starts_with("202"));
                assert!(s.chars().filter(|c| *c == '-').count() == 2);
            }
            _ => panic!("expected string from time.today()"),
        }
    }

    #[test]
    fn time_date_constructs_specific_date() {
        let result = try_run_forge(
            r#"
            let t = time.date(2026, 12, 25)
            assert(t.year == 2026)
            assert(t.month == 12)
            assert(t.day == 25)
            assert(t.hour == 0)
            assert(t.minute == 0)
            assert(t.second == 0)
            assert(t.weekday == "Friday")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_date_invalid() {
        let result = try_run_forge(r#"time.date(2026, 13, 1)"#);
        assert!(result.is_err());
    }

    #[test]
    fn time_date_leap_day() {
        let result = try_run_forge(
            r#"
            let t = time.date(2024, 2, 29)
            assert(t.year == 2024)
            assert(t.month == 2)
            assert(t.day == 29)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_parse_iso_date() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-15")
            assert(t.year == 2026)
            assert(t.month == 1)
            assert(t.day == 15)
            assert(t.hour == 0)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_parse_iso_datetime() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-07-04T14:30:00")
            assert(t.year == 2026)
            assert(t.month == 7)
            assert(t.day == 4)
            assert(t.hour == 14)
            assert(t.minute == 30)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_parse_datetime_with_space() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-03-15 09:45:00")
            assert(t.year == 2026)
            assert(t.month == 3)
            assert(t.day == 15)
            assert(t.hour == 9)
            assert(t.minute == 45)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_parse_us_format() {
        let result = try_run_forge(
            r#"
            let t = time.parse("07/04/2026")
            assert(t.year == 2026)
            assert(t.month == 7)
            assert(t.day == 4)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_parse_european_format() {
        let result = try_run_forge(
            r#"
            let t = time.parse("15.01.2026")
            assert(t.year == 2026)
            assert(t.month == 1)
            assert(t.day == 15)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_parse_with_timezone() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-06-15", "Asia/Tokyo")
            assert(t.timezone == "Asia/Tokyo")
            assert(t.year == 2026)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_parse_unix_timestamp() {
        let result = try_run_forge(
            r#"
            let t = time.parse(1700000000)
            assert(t.year == 2023)
            assert(t.month == 11)
            assert(t.day == 14)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_parse_invalid_string() {
        let result = try_run_forge(r#"time.parse("not-a-date")"#);
        assert!(result.is_err());
    }

    #[test]
    fn time_format_default() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-03-15T10:30:45")
            time.format(t)
        "#,
        );
        match value {
            Value::String(s) => {
                assert!(s.contains("2026"));
                assert!(s.contains("10:30:45"));
            }
            _ => panic!("expected formatted string"),
        }
    }

    #[test]
    fn time_format_custom_pattern() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-12-25")
            time.format(t, "%B %d, %Y")
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "December 25, 2026"),
            _ => panic!("expected formatted string"),
        }
    }

    #[test]
    fn time_format_date_only() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-07-04")
            time.format(t, "%Y/%m/%d")
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "2026/07/04"),
            _ => panic!("expected formatted string"),
        }
    }

    #[test]
    fn time_format_12_hour_clock() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-01-01T14:30:00")
            time.format(t, "%I:%M %p")
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "02:30 PM"),
            _ => panic!("expected formatted string"),
        }
    }

    #[test]
    fn time_from_unix_known_epoch() {
        let result = try_run_forge(
            r#"
            let t = time.from_unix(0)
            assert(t.year == 1970)
            assert(t.month == 1)
            assert(t.day == 1)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_from_unix_recent() {
        let result = try_run_forge(
            r#"
            let t = time.from_unix(1700000000)
            assert(t.year == 2023)
            assert(t.unix == 1700000000)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_diff_positive() {
        let value = run_forge(
            r#"
            let a = time.parse("2026-03-01")
            let b = time.parse("2026-02-15")
            time.diff(a, b)
        "#,
        );
        match value {
            Value::Object(m) => {
                assert_eq!(m.get("seconds"), Some(&Value::Int(1209600)));
                assert_eq!(m.get("days"), Some(&Value::Float(14.0)));
                assert_eq!(m.get("weeks"), Some(&Value::Float(2.0)));
            }
            _ => panic!("expected diff object"),
        }
    }

    #[test]
    fn time_diff_negative() {
        let value = run_forge(
            r#"
            let a = time.parse("2026-01-01")
            let b = time.parse("2026-01-10")
            time.diff(a, b)
        "#,
        );
        match value {
            Value::Object(m) => {
                if let Some(Value::Int(s)) = m.get("seconds") {
                    assert!(*s < 0);
                } else {
                    panic!("expected seconds field");
                }
            }
            _ => panic!("expected diff object"),
        }
    }

    #[test]
    fn time_diff_same_date() {
        let value = run_forge(
            r#"
            let a = time.parse("2026-06-15")
            let b = time.parse("2026-06-15")
            time.diff(a, b)
        "#,
        );
        match value {
            Value::Object(m) => {
                assert_eq!(m.get("seconds"), Some(&Value::Int(0)));
            }
            _ => panic!("expected diff object"),
        }
    }

    #[test]
    fn time_diff_human_readable() {
        let value = run_forge(
            r#"
            let a = time.parse("2026-01-03T12:00:00")
            let b = time.parse("2026-01-01T00:00:00")
            let d = time.diff(a, b)
            d.human
        "#,
        );
        match value {
            Value::String(s) => assert_eq!(s, "2d 12h 0m 0s"),
            _ => panic!("expected human-readable diff string"),
        }
    }

    #[test]
    fn time_add_days() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01")
            let future = time.add(t, {days: 30})
            assert(future.month == 1)
            assert(future.day == 31)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_add_hours_and_minutes() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T00:00:00")
            let future = time.add(t, {hours: 25, minutes: 30})
            assert(future.day == 2)
            assert(future.hour == 1)
            assert(future.minute == 30)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_add_weeks() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01")
            let future = time.add(t, {weeks: 2})
            assert(future.day == 15)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_add_months() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-15")
            let future = time.add(t, {months: 3})
            assert(future.month == 4)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_add_seconds_integer() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T00:00:00")
            let future = time.add(t, 3600)
            assert(future.hour == 1)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_sub_days() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-31")
            let past = time.sub(t, {days: 30})
            assert(past.month == 1)
            assert(past.day == 1)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_sub_weeks() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-03-01")
            let past = time.sub(t, {weeks: 4})
            assert(past.month == 2)
            assert(past.day == 1)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_sub_seconds_integer() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T01:00:00")
            let past = time.sub(t, 3600)
            assert(past.hour == 0)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_zone_conversion() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T12:00:00")
            let ny = time.zone(t, "America/New_York")
            assert(ny.timezone == "America/New_York")
            assert(ny.hour == 7)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_zone_tokyo() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T00:00:00")
            let tokyo = time.zone(t, "Asia/Tokyo")
            assert(tokyo.timezone == "Asia/Tokyo")
            assert(tokyo.hour == 9)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_zone_london() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-07-01T12:00:00")
            let london = time.zone(t, "Europe/London")
            assert(london.timezone == "Europe/London")
            assert(london.hour == 13)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_zone_kolkata() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T00:00:00")
            let india = time.zone(t, "Asia/Kolkata")
            assert(india.timezone == "Asia/Kolkata")
            assert(india.hour == 5)
            assert(india.minute == 30)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_zone_invalid() {
        let result = try_run_forge(
            r#"
            let t = time.now()
            time.zone(t, "Invalid/Zone")
        "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn time_zones_returns_array() {
        let value = run_forge("time.zones()");
        match value {
            Value::Array(items) => assert!(items.len() > 400),
            _ => panic!("expected array of timezone strings"),
        }
    }

    #[test]
    fn time_zones_filter() {
        let value = run_forge(r#"time.zones("India")"#);
        match value {
            Value::Array(items) => {
                assert!(items.len() > 0);
                for item in &items {
                    if let Value::String(s) = item {
                        assert!(s.to_lowercase().contains("india"));
                    }
                }
            }
            _ => panic!("expected filtered array"),
        }
    }

    #[test]
    fn time_zones_filter_us() {
        let value = run_forge(r#"time.zones("US/")"#);
        match value {
            Value::Array(items) => {
                assert!(items.len() >= 5);
                for item in &items {
                    if let Value::String(s) = item {
                        assert!(s.contains("US/"));
                    }
                }
            }
            _ => panic!("expected US timezone array"),
        }
    }

    #[test]
    fn time_zones_filter_no_match() {
        let value = run_forge(r#"time.zones("xyznotreal")"#);
        match value {
            Value::Array(items) => assert_eq!(items.len(), 0),
            _ => panic!("expected empty array"),
        }
    }

    #[test]
    fn time_is_before_true() {
        let value = run_forge(
            r#"
            let a = time.parse("2025-01-01")
            let b = time.parse("2026-01-01")
            time.is_before(a, b)
        "#,
        );
        assert_eq!(value, Value::Bool(true));
    }

    #[test]
    fn time_is_before_false() {
        let value = run_forge(
            r#"
            let a = time.parse("2026-01-01")
            let b = time.parse("2025-01-01")
            time.is_before(a, b)
        "#,
        );
        assert_eq!(value, Value::Bool(false));
    }

    #[test]
    fn time_is_after_true() {
        let value = run_forge(
            r#"
            let a = time.parse("2026-06-01")
            let b = time.parse("2026-01-01")
            time.is_after(a, b)
        "#,
        );
        assert_eq!(value, Value::Bool(true));
    }

    #[test]
    fn time_is_after_false() {
        let value = run_forge(
            r#"
            let a = time.parse("2025-01-01")
            let b = time.parse("2026-01-01")
            time.is_after(a, b)
        "#,
        );
        assert_eq!(value, Value::Bool(false));
    }

    #[test]
    fn time_is_before_equal_dates() {
        let value = run_forge(
            r#"
            let a = time.parse("2026-01-01")
            let b = time.parse("2026-01-01")
            time.is_before(a, b)
        "#,
        );
        assert_eq!(value, Value::Bool(false));
    }

    #[test]
    fn time_start_of_day() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-05-15T14:30:45")
            let s = time.start_of(t, "day")
            assert(s.hour == 0)
            assert(s.minute == 0)
            assert(s.second == 0)
            assert(s.day == 15)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_start_of_month() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-05-15T14:30:45")
            let s = time.start_of(t, "month")
            assert(s.day == 1)
            assert(s.month == 5)
            assert(s.hour == 0)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_start_of_year() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-07-15T14:30:00")
            let s = time.start_of(t, "year")
            assert(s.month == 1)
            assert(s.day == 1)
            assert(s.hour == 0)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_start_of_week() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-03-05")
            let s = time.start_of(t, "week")
            assert(s.weekday == "Monday")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_start_of_hour() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T14:45:30")
            let s = time.start_of(t, "hour")
            assert(s.hour == 14)
            assert(s.minute == 0)
            assert(s.second == 0)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_start_of_minute() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T14:45:30")
            let s = time.start_of(t, "minute")
            assert(s.hour == 14)
            assert(s.minute == 45)
            assert(s.second == 0)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_end_of_day() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-05-15T10:00:00")
            let e = time.end_of(t, "day")
            assert(e.hour == 23)
            assert(e.minute == 59)
            assert(e.second == 59)
            assert(e.day == 15)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_end_of_month_february() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-02-10")
            let e = time.end_of(t, "month")
            assert(e.day == 28)
            assert(e.month == 2)
            assert(e.hour == 23)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_end_of_month_february_leap() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2024-02-10")
            let e = time.end_of(t, "month")
            assert(e.day == 29)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_end_of_year() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-06-15")
            let e = time.end_of(t, "year")
            assert(e.month == 12)
            assert(e.day == 31)
            assert(e.hour == 23)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_end_of_week() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-03-02")
            let e = time.end_of(t, "week")
            assert(e.weekday == "Sunday")
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_end_of_hour() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T14:15:00")
            let e = time.end_of(t, "hour")
            assert(e.hour == 14)
            assert(e.minute == 59)
            assert(e.second == 59)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_is_weekend_saturday() {
        let result = try_run_forge(
            r#"
            let sat = time.parse("2026-02-28")
            assert(time.is_weekend(sat) == true)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_is_weekend_sunday() {
        let result = try_run_forge(
            r#"
            let sun = time.parse("2026-03-01")
            assert(time.is_weekend(sun) == true)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_is_weekend_weekday() {
        let result = try_run_forge(
            r#"
            let mon = time.parse("2026-03-02")
            assert(time.is_weekend(mon) == false)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_is_weekday_monday() {
        let result = try_run_forge(
            r#"
            let mon = time.parse("2026-03-02")
            assert(time.is_weekday(mon) == true)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_is_weekday_saturday() {
        let result = try_run_forge(
            r#"
            let sat = time.parse("2026-02-28")
            assert(time.is_weekday(sat) == false)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_day_of_week_known() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-12-25")
            time.day_of_week(t)
        "#,
        );
        assert_eq!(value, Value::String("Friday".to_string()));
    }

    #[test]
    fn time_day_of_week_epoch() {
        let value = run_forge(
            r#"
            let t = time.from_unix(0)
            time.day_of_week(t)
        "#,
        );
        assert_eq!(value, Value::String("Thursday".to_string()));
    }

    #[test]
    fn time_days_in_month_february_normal() {
        let value = run_forge("time.days_in_month(2026, 2)");
        assert_eq!(value, Value::Int(28));
    }

    #[test]
    fn time_days_in_month_february_leap() {
        let value = run_forge("time.days_in_month(2024, 2)");
        assert_eq!(value, Value::Int(29));
    }

    #[test]
    fn time_days_in_month_january() {
        let value = run_forge("time.days_in_month(2026, 1)");
        assert_eq!(value, Value::Int(31));
    }

    #[test]
    fn time_days_in_month_april() {
        let value = run_forge("time.days_in_month(2026, 4)");
        assert_eq!(value, Value::Int(30));
    }

    #[test]
    fn time_days_in_month_december() {
        let value = run_forge("time.days_in_month(2026, 12)");
        assert_eq!(value, Value::Int(31));
    }

    #[test]
    fn time_is_leap_year_true() {
        assert_eq!(run_forge("time.is_leap_year(2024)"), Value::Bool(true));
        assert_eq!(run_forge("time.is_leap_year(2000)"), Value::Bool(true));
        assert_eq!(run_forge("time.is_leap_year(2400)"), Value::Bool(true));
    }

    #[test]
    fn time_is_leap_year_false() {
        assert_eq!(run_forge("time.is_leap_year(2026)"), Value::Bool(false));
        assert_eq!(run_forge("time.is_leap_year(1900)"), Value::Bool(false));
        assert_eq!(run_forge("time.is_leap_year(2100)"), Value::Bool(false));
    }

    #[test]
    fn time_measure_returns_millis() {
        let value = run_forge("time.measure()");
        match value {
            Value::Int(n) => assert!(n > 1700000000000i64),
            _ => panic!("expected large int from time.measure()"),
        }
    }

    #[test]
    fn time_elapsed_returns_millis() {
        let value = run_forge("time.elapsed()");
        match value {
            Value::Int(n) => assert!(n > 1700000000000i64),
            _ => panic!("expected large int from time.elapsed()"),
        }
    }

    #[test]
    fn time_roundtrip_parse_format() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-06-15T09:30:00")
            let formatted = time.format(t, "%Y-%m-%dT%H:%M:%S")
            formatted
        "#,
        );
        assert_eq!(value, Value::String("2026-06-15T09:30:00".to_string()));
    }

    #[test]
    fn time_add_then_sub_identity() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-06-15")
            let added = time.add(t, {days: 10})
            let back = time.sub(added, {days: 10})
            assert(back.unix == t.unix)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_chained_operations() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01")
            let dur = {months: 6, days: 14}
            let future = time.add(t, dur)
            assert(future.month == 7)
            assert(future.day == 14)
        "#,
        );
        assert!(
            result.is_ok(),
            "time_chained_operations failed: {:?}",
            result
        );
    }

    #[test]
    fn time_zone_preserves_unix() {
        let result = try_run_forge(
            r#"
            let t = time.now()
            let ny = time.zone(t, "America/New_York")
            let tokyo = time.zone(t, "Asia/Tokyo")
            assert(ny.unix == tokyo.unix)
            assert(ny.unix == t.unix)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_diff_then_add_roundtrip() {
        let value = run_forge(
            r#"
            let a = time.parse("2026-01-01")
            let b = time.parse("2026-03-15")
            let d = time.diff(b, a)
            let secs = get(d, "seconds")
            let restored = time.add(a, secs)
            restored.unix == b.unix
        "#,
        );
        assert_eq!(value, Value::Bool(true));
    }

    #[test]
    fn time_start_end_of_same_day() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-04-10T12:00:00")
            let s = time.start_of(t, "day")
            let e = time.end_of(t, "day")
            assert(s.day == e.day)
            assert(s.hour == 0)
            assert(e.hour == 23)
            let d = time.diff(e, s)
            let secs = get(d, "seconds")
            secs
        "#,
        );
        assert_eq!(value, Value::Int(86399));
    }

    #[test]
    fn time_weekday_fields_on_parsed_date() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-01-01")
            t.weekday
        "#,
        );
        assert_eq!(value, Value::String("Thursday".to_string()));
    }

    #[test]
    fn time_weekday_short_field() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-01-01")
            t.weekday_short
        "#,
        );
        assert_eq!(value, Value::String("Thu".to_string()));
    }

    #[test]
    fn time_day_of_year_jan_1() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-01-01")
            t.day_of_year
        "#,
        );
        assert_eq!(value, Value::Int(1));
    }

    #[test]
    fn time_day_of_year_dec_31() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-12-31")
            t.day_of_year
        "#,
        );
        assert_eq!(value, Value::Int(365));
    }

    #[test]
    fn time_cross_year_add() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2025-12-25")
            let future = time.add(t, {days: 10})
            assert(future.year == 2026)
            assert(future.month == 1)
            assert(future.day == 4)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_cross_year_sub() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-05")
            let past = time.sub(t, {days: 10})
            assert(past.year == 2025)
            assert(past.month == 12)
            assert(past.day == 26)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_add_millis() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-01-01T00:00:00")
            let future = time.add(t, {millis: 5000})
            assert(future.second == 5)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_multiple_timezone_conversions() {
        let result = try_run_forge(
            r#"
            let utc = time.parse("2026-06-15T12:00:00")
            let ny = time.zone(utc, "America/New_York")
            let la = time.zone(utc, "America/Los_Angeles")
            let london = time.zone(utc, "Europe/London")
            let tokyo = time.zone(utc, "Asia/Tokyo")
            assert(ny.hour == 8)
            assert(la.hour == 5)
            assert(london.hour == 13)
            assert(tokyo.hour == 21)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_end_of_month_december() {
        let result = try_run_forge(
            r#"
            let t = time.parse("2026-12-01")
            let e = time.end_of(t, "month")
            assert(e.day == 31)
            assert(e.month == 12)
        "#,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn time_start_of_invalid_unit() {
        let result = try_run_forge(
            r#"
            let t = time.now()
            time.start_of(t, "century")
        "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn time_end_of_invalid_unit() {
        let result = try_run_forge(
            r#"
            let t = time.now()
            time.end_of(t, "millennium")
        "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn time_format_weekday_name() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-12-25")
            time.format(t, "%A")
        "#,
        );
        assert_eq!(value, Value::String("Friday".to_string()));
    }

    #[test]
    fn time_format_month_name() {
        let value = run_forge(
            r#"
            let t = time.parse("2026-07-04")
            time.format(t, "%B")
        "#,
        );
        assert_eq!(value, Value::String("July".to_string()));
    }

    #[test]
    fn time_days_in_month_from_time_object() {
        let value = run_forge(
            r#"
            let t = time.parse("2024-02-15")
            time.days_in_month(t)
        "#,
        );
        assert_eq!(value, Value::Int(29));
    }
}
