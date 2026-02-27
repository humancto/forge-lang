use crate::parser::ast::*;
/// Forge Tree-Walk Interpreter
/// Walks the AST and executes it directly.
/// Phase 1 only — replaced by bytecode VM in Phase 3.
use std::collections::HashMap;
use std::fmt;

/// Runtime values
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
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

/// Variable environment (scope chain)
#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<HashMap<String, Value>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
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

    pub fn set(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(RuntimeError::new(&format!("undefined variable: {}", name)))
    }
}

/// Control flow signals
enum Signal {
    None,
    Return(Value),
    Break,
    Continue,
}

/// The interpreter
pub struct Interpreter {
    pub env: Environment,
    pub output: Vec<String>, // captured output for testing
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
            env: Environment::new(),
            output: Vec::new(),
        };
        interp.register_builtins();
        interp
    }

    fn register_builtins(&mut self) {
        // Register built-in functions
        for name in &[
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
            "Ok",
            "Err",
            "is_ok",
            "is_err",
            "unwrap",
            "unwrap_or",
            "json",
            "fetch",
            "time",
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
                Signal::None => {}
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
                Signal::None => {}
            }
            // In REPL mode, capture the last expression's value
            if let Stmt::Expression(ref expr) = stmt {
                // Re-evaluate to get the value (cheap for most exprs)
                // But skip calls to print/println to avoid double execution
                match expr {
                    Expr::Call { function, .. } => {
                        if let Expr::Ident(name) = function.as_ref() {
                            if name == "print" || name == "println" {
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
            Stmt::Let { name, value, .. } => {
                let val = self.eval_expr(value)?;
                self.env.define(name.clone(), val);
                Ok(Signal::None)
            }

            Stmt::Assign { target, value } => {
                let val = self.eval_expr(value)?;
                match target {
                    Expr::Ident(name) => self.env.set(name, val)?,
                    Expr::FieldAccess { object, field } => {
                        // obj.field = value (we need mutable access)
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
                // Re-capture closure with self-reference for recursion
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
                // Store struct definition as a constructor function
                self.env
                    .define(name.clone(), Value::BuiltIn(format!("struct:{}", name)));
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
                                Signal::None => {
                                    self.env.pop_scope();
                                }
                            }
                        }
                    }
                    _ => return Err(RuntimeError::new("can only iterate over arrays")),
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
                        Signal::None => {}
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
                        Signal::None => {}
                    }
                }
                Ok(Signal::None)
            }

            Stmt::Break => Ok(Signal::Break),
            Stmt::Continue => Ok(Signal::Continue),

            Stmt::Spawn { body } => {
                // Phase 1: just run synchronously (spawn is a no-op)
                self.exec_block(body)?;
                Ok(Signal::None)
            }

            Stmt::DecoratorStmt(dec) => {
                // Store decorator config (e.g., @server(port: 8080))
                let name = format!("@{}", dec.name);
                let mut config = HashMap::new();
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

            Stmt::Expression(expr) => {
                self.eval_expr(expr)?;
                Ok(Signal::None)
            }
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<Signal, RuntimeError> {
        self.env.push_scope();
        let mut result = Signal::None;
        for stmt in stmts {
            result = self.exec_stmt(stmt)?;
            match &result {
                Signal::Return(_) | Signal::Break | Signal::Continue => break,
                Signal::None => {}
            }
        }
        self.env.pop_scope();
        Ok(result)
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
                let vals: Result<Vec<Value>, _> = items.iter().map(|e| self.eval_expr(e)).collect();
                Ok(Value::Array(vals?))
            }

            Expr::Object(fields) => {
                let mut map = HashMap::new();
                for (key, expr) in fields {
                    map.insert(key.clone(), self.eval_expr(expr)?);
                }
                Ok(Value::Object(map))
            }

            Expr::Ident(name) => self
                .env
                .get(name)
                .cloned()
                .ok_or_else(|| RuntimeError::new(&format!("undefined variable: {}", name))),

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
                    Value::String(s) => {
                        // String methods
                        match field.as_str() {
                            "len" => Ok(Value::Int(s.len() as i64)),
                            "upper" => Ok(Value::String(s.to_uppercase())),
                            "lower" => Ok(Value::String(s.to_lowercase())),
                            "trim" => Ok(Value::String(s.trim().to_string())),
                            _ => Err(RuntimeError::new(&format!(
                                "no method '{}' on String",
                                field
                            ))),
                        }
                    }
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
                let mut map = HashMap::new();
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
                    match self.exec_stmt(stmt)? {
                        Signal::Return(v) => {
                            self.env.pop_scope();
                            return Ok(v);
                        }
                        _ => {}
                    }
                    if let Stmt::Expression(expr) = stmt {
                        last = self.eval_expr(expr)?;
                    }
                }
                self.env.pop_scope();
                Ok(last)
            }
        }
    }

    fn eval_binop(&self, left: &Value, op: &BinOp, right: &Value) -> Result<Value, RuntimeError> {
        match (left, right) {
            // Int arithmetic
            (Value::Int(a), Value::Int(b)) => match op {
                BinOp::Add => Ok(Value::Int(a + b)),
                BinOp::Sub => Ok(Value::Int(a - b)),
                BinOp::Mul => Ok(Value::Int(a * b)),
                BinOp::Div => {
                    if *b == 0 {
                        return Err(RuntimeError::new("division by zero"));
                    }
                    Ok(Value::Int(a / b))
                }
                BinOp::Mod => Ok(Value::Int(a % b)),
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                BinOp::Lt => Ok(Value::Bool(a < b)),
                BinOp::Gt => Ok(Value::Bool(a > b)),
                BinOp::LtEq => Ok(Value::Bool(a <= b)),
                BinOp::GtEq => Ok(Value::Bool(a >= b)),
                _ => Err(RuntimeError::new("invalid operator for Int")),
            },

            // Float arithmetic
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

            // Mixed numeric
            (Value::Int(a), Value::Float(b)) => {
                self.eval_binop(&Value::Float(*a as f64), op, &Value::Float(*b))
            }
            (Value::Float(a), Value::Int(b)) => {
                self.eval_binop(&Value::Float(*a), op, &Value::Float(*b as f64))
            }

            // String concatenation
            (Value::String(a), Value::String(b)) => match op {
                BinOp::Add => Ok(Value::String(format!("{}{}", a, b))),
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                _ => Err(RuntimeError::new("invalid operator for String")),
            },

            // Boolean logic
            (Value::Bool(a), Value::Bool(b)) => match op {
                BinOp::And => Ok(Value::Bool(*a && *b)),
                BinOp::Or => Ok(Value::Bool(*a || *b)),
                BinOp::Eq => Ok(Value::Bool(a == b)),
                BinOp::NotEq => Ok(Value::Bool(a != b)),
                _ => Err(RuntimeError::new("invalid operator for Bool")),
            },

            // String + anything = concatenation
            (Value::String(a), b) => match op {
                BinOp::Add => Ok(Value::String(format!("{}{}", a, b))),
                _ => Err(RuntimeError::new("invalid operator")),
            },
            (a, Value::String(b)) => match op {
                BinOp::Add => Ok(Value::String(format!("{}{}", a, b))),
                _ => Err(RuntimeError::new("invalid operator")),
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
        match func {
            Value::Function {
                name,
                params,
                body,
                closure,
                ..
            } => {
                let saved_env = self.env.clone();
                self.env = closure;
                self.env.push_scope();

                // Enable recursion: inject the function itself into its scope
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

                let result = self.exec_block(&body);
                self.env.pop_scope();
                self.env = saved_env;

                match result {
                    Ok(Signal::Return(v)) => Ok(v),
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

                let result = self.exec_block(&body);
                self.env.pop_scope();
                self.env = saved_env;

                match result {
                    Ok(Signal::Return(v)) => Ok(v),
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

    fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
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
            "type" => match args.first() {
                Some(v) => Ok(Value::String(v.type_name().to_string())),
                None => Err(RuntimeError::new("type() requires an argument")),
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
            "contains" => match (args.get(0), args.get(1)) {
                (Some(Value::String(s)), Some(Value::String(sub))) => {
                    Ok(Value::Bool(s.contains(sub.as_str())))
                }
                (Some(Value::Array(arr)), Some(val)) => Ok(Value::Bool(
                    arr.iter().any(|v| format!("{}", v) == format!("{}", val)),
                )),
                _ => Err(RuntimeError::new(
                    "contains() requires (string, string) or (array, value)",
                )),
            },
            "range" => match (args.get(0), args.get(1)) {
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
                        let mut row = HashMap::new();
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
            "Ok" => {
                let value = args.first().cloned().unwrap_or(Value::Null);
                Ok(Value::ResultOk(Box::new(value)))
            }
            "Err" => {
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
                // Stub — returns mock time
                Ok(Value::Object({
                    let mut m = HashMap::new();
                    m.insert(
                        "now".to_string(),
                        Value::String("2026-02-22T00:00:00Z".to_string()),
                    );
                    m
                }))
            }
            "json" => {
                // json.parse / json.stringify stubs
                match args.first() {
                    Some(Value::String(s)) => {
                        // Try to parse JSON string
                        match serde_json::from_str::<serde_json::Value>(s) {
                            Ok(v) => Ok(json_to_value(v)),
                            Err(e) => Err(RuntimeError::new(&format!("JSON parse error: {}", e))),
                        }
                    }
                    Some(v) => Ok(Value::String(v.to_json_string())),
                    None => Err(RuntimeError::new("json() requires an argument")),
                }
            }
            _ => Err(RuntimeError::new(&format!("unknown builtin: {}", name))),
        }
    }

    // ========== Pattern Matching ==========

    fn match_pattern(&self, pattern: &Pattern, value: &Value) -> bool {
        match pattern {
            Pattern::Wildcard => true,
            Pattern::Binding(_) => true,
            Pattern::Literal(expr) => {
                // Simple literal comparison
                match (expr, value) {
                    (Expr::Int(a), Value::Int(b)) => a == b,
                    (Expr::Float(a), Value::Float(b)) => a == b,
                    (Expr::StringLit(a), Value::String(b)) => a == b,
                    (Expr::Bool(a), Value::Bool(b)) => a == b,
                    _ => false,
                }
            }
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
                    // Also check variant tag
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

    fn eval_repl(source: &str) -> Value {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("parsing should succeed");
        let mut interpreter = Interpreter::new();
        interpreter
            .run_repl(&program)
            .expect("execution should succeed")
    }

    #[test]
    fn evaluates_interpolated_expression() {
        let value = eval_repl(
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
        let value = eval_repl(
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
        let value = eval_repl(
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
        let value = eval_repl(
            r#"
            fn double(x) { return x * 2 }
            fn even(x) { return x % 2 == 0 }

            let mapped = map([1, 2, 3, 4], double)
            let filtered = filter(mapped, even)
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
        let value = eval_repl(
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
}
