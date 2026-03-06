use indexmap::IndexMap;
use std::collections::HashMap;

use super::bytecode::*;
use super::frame::*;
use super::gc::Gc;
use super::jit::profiler::Profiler;
use super::value::*;

#[derive(Clone, Copy)]
pub struct JitEntry {
    pub ptr: *const u8,
    pub uses_float: bool,
}

pub struct VM {
    pub registers: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub globals: HashMap<String, Value>,
    pub gc: Gc,
    pub output: Vec<String>,
    pub jit_cache: HashMap<String, JitEntry>,
    pub profiler: Profiler,
}

#[derive(Debug)]
pub struct VMError {
    pub message: String,
    pub stack_trace: Vec<StackFrame>,
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
        }
    }

    #[allow(dead_code)]
    pub fn with_trace(msg: &str, trace: Vec<StackFrame>) -> Self {
        Self {
            message: msg.to_string(),
            stack_trace: trace,
        }
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
            registers: vec![Value::Null; MAX_REGISTERS],
            frames: Vec::with_capacity(MAX_FRAMES),
            globals: HashMap::new(),
            gc: Gc::new(),
            output: Vec::new(),
            jit_cache: HashMap::new(),
            profiler: Profiler::new(false),
        };
        vm.register_builtins();
        vm
    }

    pub fn with_profiling() -> Self {
        let mut vm = Self {
            registers: vec![Value::Null; MAX_REGISTERS],
            frames: Vec::with_capacity(MAX_FRAMES),
            globals: HashMap::new(),
            gc: Gc::new(),
            output: Vec::new(),
            jit_cache: HashMap::new(),
            profiler: Profiler::new(true),
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
        ];
        for name in &builtins {
            let name_ref = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: name.to_string(),
                func: native_dispatch,
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
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
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
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            fs_map.insert(name.to_string(), Value::Obj(nr));
        }
        let fs_ref = self.gc.alloc(ObjKind::Object(fs_map));
        self.globals.insert("fs".to_string(), Value::Obj(fs_ref));

        // io module
        let mut io_map = IndexMap::new();
        for name in &["prompt", "print", "args"] {
            let full = format!("io.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
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
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            crypto_map.insert(name.to_string(), Value::Obj(nr));
        }
        let crypto_ref = self.gc.alloc(ObjKind::Object(crypto_map));
        self.globals
            .insert("crypto".to_string(), Value::Obj(crypto_ref));

        // db module
        let mut db_map = IndexMap::new();
        for name in &["open", "query", "execute", "close"] {
            let full = format!("db.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            db_map.insert(name.to_string(), Value::Obj(nr));
        }
        let db_ref = self.gc.alloc(ObjKind::Object(db_map));
        self.globals.insert("db".to_string(), Value::Obj(db_ref));

        // env module
        let mut env_map = IndexMap::new();
        for name in &["get", "set", "keys", "has"] {
            let full = format!("env.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            env_map.insert(name.to_string(), Value::Obj(nr));
        }
        let env_ref = self.gc.alloc(ObjKind::Object(env_map));
        self.globals.insert("env".to_string(), Value::Obj(env_ref));

        // json module
        let mut json_map = IndexMap::new();
        for name in &["parse", "stringify"] {
            let full = format!("json.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            json_map.insert(name.to_string(), Value::Obj(nr));
        }
        let json_ref = self.gc.alloc(ObjKind::Object(json_map));
        self.globals
            .insert("json".to_string(), Value::Obj(json_ref));

        // regex module
        let mut regex_map = IndexMap::new();
        for name in &["test", "find", "find_all", "replace", "split"] {
            let full = format!("regex.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            regex_map.insert(name.to_string(), Value::Obj(nr));
        }
        let regex_ref = self.gc.alloc(ObjKind::Object(regex_map));
        self.globals
            .insert("regex".to_string(), Value::Obj(regex_ref));

        // log module
        let mut log_map = IndexMap::new();
        for name in &["info", "warn", "error", "debug"] {
            let full = format!("log.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
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
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
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
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            term_map.insert(name.to_string(), Value::Obj(nr));
        }
        let term_ref = self.gc.alloc(ObjKind::Object(term_map));
        self.globals
            .insert("term".to_string(), Value::Obj(term_ref));

        // csv module
        let mut csv_map = IndexMap::new();
        for name in &["parse", "stringify", "read", "write"] {
            let full = format!("csv.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
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
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            time_map.insert(name.to_string(), Value::Obj(nr));
        }
        let time_ref = self.gc.alloc(ObjKind::Object(time_map));
        self.globals
            .insert("time".to_string(), Value::Obj(time_ref));

        // pg module
        let mut pg_map = IndexMap::new();
        for name in &["connect", "query", "execute", "close"] {
            let full = format!("pg.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            pg_map.insert(name.to_string(), Value::Obj(nr));
        }
        let pg_ref = self.gc.alloc(ObjKind::Object(pg_map));
        self.globals.insert("pg".to_string(), Value::Obj(pg_ref));

        // jwt module
        let mut jwt_map = IndexMap::new();
        for name in &["sign", "verify", "decode", "valid"] {
            let full = format!("jwt.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            jwt_map.insert(name.to_string(), Value::Obj(nr));
        }
        let jwt_ref = self.gc.alloc(ObjKind::Object(jwt_map));
        self.globals.insert("jwt".to_string(), Value::Obj(jwt_ref));

        // mysql module
        let mut mysql_map = IndexMap::new();
        for name in &["connect", "query", "execute", "close"] {
            let full = format!("mysql.{}", name);
            let nr = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
                name: full,
                func: native_dispatch,
            }));
            mysql_map.insert(name.to_string(), Value::Obj(nr));
        }
        let mysql_ref = self.gc.alloc(ObjKind::Object(mysql_map));
        self.globals
            .insert("mysql".to_string(), Value::Obj(mysql_ref));

        // Option prelude
        let mut none_obj = IndexMap::new();
        none_obj.insert("__type__".to_string(), self.alloc_string("Option"));
        none_obj.insert("__variant__".to_string(), self.alloc_string("None"));
        let none_ref = self.gc.alloc(ObjKind::Object(none_obj));
        self.globals
            .insert("None".to_string(), Value::Obj(none_ref));

        let some_native = self.gc.alloc(ObjKind::NativeFunction(NativeFn {
            name: "Some".to_string(),
            func: native_dispatch,
        }));
        self.globals
            .insert("Some".to_string(), Value::Obj(some_native));
    }

    pub(super) fn alloc_string(&mut self, s: &str) -> Value {
        let r = self.gc.alloc_string(s.to_string());
        Value::Obj(r)
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
        self.run()
    }

    fn run(&mut self) -> Result<Value, VMError> {
        loop {
            if self.frames.is_empty() {
                return Ok(Value::Null);
            }

            let frame_idx = self.frames.len() - 1;
            let chunk = {
                let frame = &self.frames[frame_idx];
                let closure_obj = self
                    .gc
                    .get(frame.closure)
                    .ok_or_else(|| VMError::new("invalid closure"))?;
                if let ObjKind::Closure(c) = &closure_obj.kind {
                    c.function.chunk.clone()
                } else {
                    return Err(VMError::new("expected closure"));
                }
            };

            let frame = &mut self.frames[frame_idx];
            if frame.ip >= chunk.code.len() {
                self.frames.pop();
                continue;
            }

            let inst = chunk.code[frame.ip];
            frame.ip += 1;
            let base = frame.base;

            let op = decode_op(inst);
            let a = decode_a(inst);
            let b = decode_b(inst);
            let c = decode_c(inst);
            let bx = decode_bx(inst);
            let sbx = decode_sbx(inst);
            let opcode: OpCode = unsafe { std::mem::transmute(op) };

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
                    self.registers[base + a as usize] = self.registers[base + b as usize].clone();
                }
                OpCode::Add => {
                    let left = self.registers[base + b as usize].clone();
                    let right = self.registers[base + c as usize].clone();
                    self.registers[base + a as usize] =
                        self.arith_op(&left, &right, OpCode::Add)?;
                }
                OpCode::Sub => {
                    let left = self.registers[base + b as usize].clone();
                    let right = self.registers[base + c as usize].clone();
                    self.registers[base + a as usize] =
                        self.arith_op(&left, &right, OpCode::Sub)?;
                }
                OpCode::Mul => {
                    let left = self.registers[base + b as usize].clone();
                    let right = self.registers[base + c as usize].clone();
                    self.registers[base + a as usize] =
                        self.arith_op(&left, &right, OpCode::Mul)?;
                }
                OpCode::Div => {
                    let left = self.registers[base + b as usize].clone();
                    let right = self.registers[base + c as usize].clone();
                    self.registers[base + a as usize] =
                        self.arith_op(&left, &right, OpCode::Div)?;
                }
                OpCode::Mod => {
                    let left = self.registers[base + b as usize].clone();
                    let right = self.registers[base + c as usize].clone();
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
                    self.registers[base + a as usize] = Value::Bool(left.equals(right, &self.gc));
                }
                OpCode::NotEq => {
                    let left = &self.registers[base + b as usize];
                    let right = &self.registers[base + c as usize];
                    self.registers[base + a as usize] = Value::Bool(!left.equals(right, &self.gc));
                }
                OpCode::Lt => {
                    let left = &self.registers[base + b as usize];
                    let right = &self.registers[base + c as usize];
                    self.registers[base + a as usize] = self.compare_op(left, right, OpCode::Lt)?;
                }
                OpCode::Gt => {
                    let left = &self.registers[base + b as usize];
                    let right = &self.registers[base + c as usize];
                    self.registers[base + a as usize] = self.compare_op(left, right, OpCode::Gt)?;
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
                        let val = self.registers[base + a as usize].clone();
                        self.globals.insert(name.clone(), val);
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
                    let func_val = self.registers[base + a as usize].clone();
                    let arg_count = b as usize;
                    let dst_reg = base + c as usize;

                    let mut args = Vec::with_capacity(arg_count);
                    for i in 0..arg_count {
                        args.push(self.registers[base + a as usize + 1 + i].clone());
                    }

                    let result = self.call_value(func_val, args)?;
                    self.registers[dst_reg] = result;
                }
                OpCode::Return => {
                    let val = self.registers[base + a as usize].clone();
                    self.profiler.exit_function();
                    self.frames.pop();
                    return Ok(val);
                }
                OpCode::ReturnNull => {
                    self.profiler.exit_function();
                    self.frames.pop();
                    return Ok(Value::Null);
                }
                OpCode::Closure => {
                    let proto = chunk.prototypes[bx as usize].clone();

                    let mut upvalue_refs = Vec::new();
                    for &src_reg in &proto.upvalue_sources {
                        let val = self.registers[base + src_reg as usize].clone();
                        let uv_ref = self.gc.alloc(ObjKind::Upvalue(ObjUpvalue { value: val }));
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
                                            self.registers[base + a as usize] = uv.value.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                OpCode::SetUpvalue => {
                    let uv_idx = a as usize;
                    let val = self.registers[base + b as usize].clone();
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
                        items.push(self.registers[start + i].clone());
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
                        let val = self.registers[start + i * 2 + 1].clone();
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
                        // Two-phase: extract data, then allocate if needed
                        let needs_alloc: Option<String>;
                        let direct_result: Option<Value>;
                        if let Some(obj) = self.gc.get(r) {
                            match &obj.kind {
                                ObjKind::Object(map) => {
                                    direct_result =
                                        Some(map.get(&field).cloned().ok_or_else(|| {
                                            VMError::new(&format!("no field '{}' on object", field))
                                        })?);
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
                            // Safety: needs_alloc and direct_result are mutually exclusive branches above
                            direct_result.expect("BUG: direct_result must be Some when needs_alloc is None")
                        };
                        self.registers[base + a as usize] = result;
                    }
                }
                OpCode::SetField => {
                    let field_const = &chunk.constants[b as usize];
                    let val = self.registers[base + c as usize].clone();
                    if let Constant::Str(field) = field_const {
                        let obj_ref = if let Value::Obj(r) = &self.registers[base + a as usize] {
                            *r
                        } else {
                            return Err(VMError::new("cannot set field on non-object"));
                        };
                        if let Some(obj) = self.gc.get_mut(obj_ref) {
                            if let ObjKind::Object(map) = &mut obj.kind {
                                map.insert(field.clone(), val);
                            }
                        }
                    }
                }
                OpCode::GetIndex => {
                    let obj = self.registers[base + b as usize].clone();
                    let idx = self.registers[base + c as usize].clone();
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
                            let key = self
                                .get_string(&idx)
                                .ok_or_else(|| VMError::new("index must be string for objects"))?;
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
                    let idx = self.registers[base + b as usize].clone();
                    let val = self.registers[base + c as usize].clone();
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
                                    ObjKind::String(s) => s.len() as i64,
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
                                    self.registers[base + a as usize] = v.clone();
                                }
                                ObjKind::ResultErr(_) => {
                                    let val = self.registers[base + b as usize].clone();
                                    self.frames.pop();
                                    return Ok(val);
                                }
                                _ => return Err(VMError::new("? operator requires Result value")),
                            }
                        }
                    } else {
                        return Err(VMError::new("? operator requires Result value"));
                    }
                }
                OpCode::Spawn => {
                    // For now, just call the closure synchronously (same as Phase 1)
                    let closure_val = self.registers[base + a as usize].clone();
                    self.call_value(closure_val, vec![])?;
                }
                _ => {
                    return Err(VMError::new(&format!("unknown opcode: {}", op)));
                }
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
                        if !func_name.is_empty()
                            && !self.jit_cache.contains_key(&func_name)
                            && self.profiler.is_hot(&func_name)
                        {
                            let type_info = super::jit::type_analysis::analyze(&chunk);
                            if !type_info.has_unsupported_ops {
                                if let Ok(mut jit) = super::jit::jit_module::JitCompiler::new() {
                                    if let Ok(ptr) = jit.compile_function(&chunk, &func_name) {
                                        self.jit_cache.insert(
                                            func_name.clone(),
                                            JitEntry {
                                                ptr,
                                                uses_float: type_info.has_float,
                                            },
                                        );
                                        std::mem::forget(jit);
                                    }
                                }
                            }
                        }

                        // JIT dispatch
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
                                    let result: f64 = unsafe {
                                        match raw_args.len() {
                                            0 => {
                                                let f: extern "C" fn() -> f64 =
                                                    std::mem::transmute(entry.ptr);
                                                f()
                                            }
                                            1 => {
                                                let f: extern "C" fn(f64) -> f64 =
                                                    std::mem::transmute(entry.ptr);
                                                f(raw_args[0])
                                            }
                                            2 => {
                                                let f: extern "C" fn(f64, f64) -> f64 =
                                                    std::mem::transmute(entry.ptr);
                                                f(raw_args[0], raw_args[1])
                                            }
                                            _ => {
                                                let f: extern "C" fn(f64, f64, f64) -> f64 =
                                                    std::mem::transmute(entry.ptr);
                                                f(
                                                    raw_args[0],
                                                    raw_args[1],
                                                    raw_args.get(2).copied().unwrap_or(0.0),
                                                )
                                            }
                                        }
                                    };
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
                                    let result: i64 = unsafe {
                                        match raw_args.len() {
                                            0 => {
                                                let f: extern "C" fn() -> i64 =
                                                    std::mem::transmute(entry.ptr);
                                                f()
                                            }
                                            1 => {
                                                let f: extern "C" fn(i64) -> i64 =
                                                    std::mem::transmute(entry.ptr);
                                                f(raw_args[0])
                                            }
                                            2 => {
                                                let f: extern "C" fn(i64, i64) -> i64 =
                                                    std::mem::transmute(entry.ptr);
                                                f(raw_args[0], raw_args[1])
                                            }
                                            _ => {
                                                let f: extern "C" fn(i64, i64, i64) -> i64 =
                                                    std::mem::transmute(entry.ptr);
                                                f(
                                                    raw_args[0],
                                                    raw_args[1],
                                                    raw_args.get(2).copied().unwrap_or(0),
                                                )
                                            }
                                        }
                                    };
                                    Value::Int(result)
                                };
                                self.profiler.exit_function();
                                return Ok(result_val);
                            }
                        }

                        let arity = chunk.arity as usize;
                        let new_base = self.frames.last().map(|f| f.base + 256).unwrap_or(0);
                        if new_base + 256 > MAX_REGISTERS {
                            return Err(VMError::new("stack overflow"));
                        }

                        for (i, arg) in args.iter().enumerate() {
                            if i < arity {
                                self.registers[new_base + i] = arg.clone();
                            }
                        }
                        for i in args.len()..arity {
                            self.registers[new_base + i] = Value::Null;
                        }

                        self.frames.push(CallFrame::new(*r, new_base));
                        self.run()
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

fn native_dispatch(_vm: &mut VM, _args: Vec<Value>) -> Result<Value, String> {
    Ok(Value::Null)
}
