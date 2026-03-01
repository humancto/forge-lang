use indexmap::IndexMap;
use std::collections::HashMap;

use super::bytecode::*;
use super::frame::*;
use super::gc::Gc;
use super::jit::profiler::Profiler;
use super::value::*;

pub struct VM {
    pub registers: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub globals: HashMap<String, Value>,
    pub gc: Gc,
    pub output: Vec<String>,
    pub jit_cache: HashMap<String, *const u8>,
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

    fn alloc_string(&mut self, s: &str) -> Value {
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

    fn get_string(&self, val: &Value) -> Option<String> {
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
                            direct_result.unwrap()
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
                            if let Ok(mut jit) = super::jit::jit_module::JitCompiler::new() {
                                if let Ok(ptr) = jit.compile_function(&chunk, &func_name) {
                                    self.jit_cache.insert(func_name.clone(), ptr);
                                    // Leak the JIT module to keep the compiled code alive
                                    std::mem::forget(jit);
                                }
                            }
                        }

                        // JIT dispatch: call native code with raw i64 values
                        if !func_name.is_empty() {
                            if let Some(&native_ptr) = self.jit_cache.get(&func_name) {
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
                                                std::mem::transmute(native_ptr);
                                            f()
                                        }
                                        1 => {
                                            let f: extern "C" fn(i64) -> i64 =
                                                std::mem::transmute(native_ptr);
                                            f(raw_args[0])
                                        }
                                        2 => {
                                            let f: extern "C" fn(i64, i64) -> i64 =
                                                std::mem::transmute(native_ptr);
                                            f(raw_args[0], raw_args[1])
                                        }
                                        _ => {
                                            let f: extern "C" fn(i64, i64, i64) -> i64 =
                                                std::mem::transmute(native_ptr);
                                            f(
                                                raw_args[0],
                                                raw_args[1],
                                                raw_args.get(2).copied().unwrap_or(0),
                                            )
                                        }
                                    }
                                };
                                self.profiler.exit_function();
                                return Ok(Value::Int(result));
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

    fn call_native(&mut self, name: &str, args: Vec<Value>) -> Result<Value, VMError> {
        match name {
            "println" | "say" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ");
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "print" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                print!("{}", text.join(" "));
                Ok(Value::Null)
            }
            "yell" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ").to_uppercase();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "whisper" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ").to_lowercase();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "len" => match args.first() {
                Some(v) => {
                    let len = match v {
                        Value::Obj(r) => self.gc.get(*r).map_or(0, |o| match &o.kind {
                            ObjKind::String(s) => s.len() as i64,
                            ObjKind::Array(a) => a.len() as i64,
                            ObjKind::Object(o) => o.len() as i64,
                            _ => 0,
                        }),
                        _ => 0,
                    };
                    Ok(Value::Int(len))
                }
                None => Err(VMError::new("len() requires an argument")),
            },
            "type" => match args.first() {
                Some(v) => {
                    let name = v.type_name(&self.gc);
                    Ok(self.alloc_string(name))
                }
                None => Err(VMError::new("type() requires an argument")),
            },
            "str" => {
                let s = args
                    .first()
                    .map(|v| v.display(&self.gc))
                    .unwrap_or_default();
                Ok(self.alloc_string(&s))
            }
            "int" => match args.first() {
                Some(Value::Int(n)) => Ok(Value::Int(*n)),
                Some(Value::Float(n)) => Ok(Value::Int(*n as i64)),
                Some(Value::Obj(r)) => {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::String(s) = &obj.kind {
                            return s.parse::<i64>().map(Value::Int).map_err(|_| {
                                VMError::new(&format!("cannot convert '{}' to Int", s))
                            });
                        }
                    }
                    Err(VMError::new("int() requires number or string"))
                }
                _ => Err(VMError::new("int() requires number or string")),
            },
            "float" => match args.first() {
                Some(Value::Int(n)) => Ok(Value::Float(*n as f64)),
                Some(Value::Float(n)) => Ok(Value::Float(*n)),
                _ => Err(VMError::new("float() requires a number")),
            },
            "range" => match (args.first(), args.get(1)) {
                (Some(Value::Int(start)), Some(Value::Int(end))) => {
                    let items: Vec<Value> = (*start..*end).map(Value::Int).collect();
                    let r = self.gc.alloc(ObjKind::Array(items));
                    Ok(Value::Obj(r))
                }
                (Some(Value::Int(end_val)), None) => {
                    let items: Vec<Value> = (0..*end_val).map(Value::Int).collect();
                    let r = self.gc.alloc(ObjKind::Array(items));
                    Ok(Value::Obj(r))
                }
                _ => Err(VMError::new("range() requires integer arguments")),
            },
            "push" => {
                if args.len() != 2 {
                    return Err(VMError::new("push() requires array and value"));
                }
                if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut new_items = items.clone();
                            new_items.push(args[1].clone());
                            let nr = self.gc.alloc(ObjKind::Array(new_items));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("push() requires an array"))
            }
            "pop" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut new_items = items.clone();
                            new_items.pop();
                            let nr = self.gc.alloc(ObjKind::Array(new_items));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("pop() requires an array"))
            }
            "Ok" | "Some" => {
                let val = args.first().cloned().unwrap_or(Value::Null);
                if name == "Some" {
                    let mut obj = IndexMap::new();
                    obj.insert("__type__".to_string(), self.alloc_string("Option"));
                    obj.insert("__variant__".to_string(), self.alloc_string("Some"));
                    obj.insert("_0".to_string(), val);
                    let r = self.gc.alloc(ObjKind::Object(obj));
                    Ok(Value::Obj(r))
                } else {
                    let r = self.gc.alloc(ObjKind::ResultOk(val));
                    Ok(Value::Obj(r))
                }
            }
            "Err" => {
                let val = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.alloc_string("error"));
                let r = self.gc.alloc(ObjKind::ResultErr(val));
                Ok(Value::Obj(r))
            }
            "is_ok" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        return Ok(Value::Bool(matches!(obj.kind, ObjKind::ResultOk(_))));
                    }
                }
                Ok(Value::Bool(false))
            }
            "is_err" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        return Ok(Value::Bool(matches!(obj.kind, ObjKind::ResultErr(_))));
                    }
                }
                Ok(Value::Bool(false))
            }
            "unwrap" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::ResultOk(v) = &obj.kind {
                            return Ok(v.clone());
                        }
                        if let ObjKind::ResultErr(v) = &obj.kind {
                            return Err(VMError::new(&format!(
                                "unwrap() on Err: {}",
                                v.display(&self.gc)
                            )));
                        }
                    }
                }
                Err(VMError::new("unwrap() requires a Result"))
            }
            "unwrap_or" => {
                if args.len() < 2 {
                    return Err(VMError::new("unwrap_or() requires 2 args"));
                }
                if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::ResultOk(v) = &obj.kind {
                            return Ok(v.clone());
                        }
                        if matches!(obj.kind, ObjKind::ResultErr(_)) {
                            return Ok(args[1].clone());
                        }
                    }
                }
                Ok(args[1].clone())
            }
            "assert" => {
                let cond = args.first().cloned().unwrap_or(Value::Bool(false));
                if !cond.is_truthy(&self.gc) {
                    let msg = args
                        .get(1)
                        .map(|v| v.display(&self.gc))
                        .unwrap_or_else(|| "assertion failed".to_string());
                    return Err(VMError::new(&format!("assertion failed: {}", msg)));
                }
                Ok(Value::Null)
            }
            "assert_eq" => {
                if args.len() < 2 {
                    return Err(VMError::new("assert_eq() requires 2 arguments"));
                }
                if !args[0].equals(&args[1], &self.gc) {
                    let left = args[0].display(&self.gc);
                    let right = args[1].display(&self.gc);
                    return Err(VMError::new(&format!(
                        "assertion failed: expected `{}`, got `{}`",
                        right, left
                    )));
                }
                Ok(Value::Null)
            }
            "map" => {
                if args.len() != 2 {
                    return Err(VMError::new("map() requires (array, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("map() first arg must be array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    }
                } else {
                    return Err(VMError::new("map() first arg must be array"));
                };
                let func = args[1].clone();
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.call_value(func.clone(), vec![item])?);
                }
                let r = self.gc.alloc(ObjKind::Array(out));
                Ok(Value::Obj(r))
            }
            "filter" => {
                if args.len() != 2 {
                    return Err(VMError::new("filter() requires (array, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("filter() first arg must be array"));
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    return Err(VMError::new("filter() first arg must be array"));
                };
                let func = args[1].clone();
                let mut out = Vec::new();
                for item in items {
                    let keep = self.call_value(func.clone(), vec![item.clone()])?;
                    if keep.is_truthy(&self.gc) {
                        out.push(item);
                    }
                }
                let r = self.gc.alloc(ObjKind::Array(out));
                Ok(Value::Obj(r))
            }
            "reduce" => {
                if args.len() != 3 {
                    return Err(VMError::new("reduce() requires (array, initial, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("reduce() first arg must be array"));
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    return Err(VMError::new("reduce() first arg must be array"));
                };
                let mut acc = args[1].clone();
                let func = args[2].clone();
                for item in items {
                    acc = self.call_value(func.clone(), vec![acc, item])?;
                }
                Ok(acc)
            }
            "sort" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut sorted = items.clone();
                            sorted.sort_by(|a, b| match (a, b) {
                                (Value::Int(x), Value::Int(y)) => x.cmp(y),
                                (Value::Float(x), Value::Float(y)) => {
                                    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                                }
                                _ => std::cmp::Ordering::Equal,
                            });
                            let nr = self.gc.alloc(ObjKind::Array(sorted));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("sort() requires an array"))
            }
            "reverse" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut rev = items.clone();
                            rev.reverse();
                            let nr = self.gc.alloc(ObjKind::Array(rev));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("reverse() requires an array"))
            }
            "contains" => match (args.first(), args.get(1)) {
                (Some(Value::Obj(r)), Some(val)) => {
                    if let Some(obj) = self.gc.get(*r) {
                        match &obj.kind {
                            ObjKind::String(s) => {
                                let sub = val.display(&self.gc);
                                return Ok(Value::Bool(s.contains(&sub)));
                            }
                            ObjKind::Array(items) => {
                                let found = items
                                    .iter()
                                    .any(|v| v.display(&self.gc) == val.display(&self.gc));
                                return Ok(Value::Bool(found));
                            }
                            _ => {}
                        }
                    }
                    Ok(Value::Bool(false))
                }
                _ => Err(VMError::new("contains() requires (collection, value)")),
            },
            "keys" => {
                if let Some(Value::Obj(r)) = args.first() {
                    let key_strings: Vec<String> = if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            map.keys().cloned().collect()
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };
                    if !key_strings.is_empty() {
                        let keys: Vec<Value> =
                            key_strings.iter().map(|k| self.alloc_string(k)).collect();
                        let nr = self.gc.alloc(ObjKind::Array(keys));
                        return Ok(Value::Obj(nr));
                    }
                }
                Err(VMError::new("keys() requires an object"))
            }
            "values" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            let vals: Vec<Value> = map.values().cloned().collect();
                            let nr = self.gc.alloc(ObjKind::Array(vals));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("values() requires an object"))
            }
            "enumerate" => {
                if let Some(Value::Obj(r)) = args.first() {
                    let items_clone: Option<Vec<Value>> = if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            Some(items.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(items) = items_clone {
                        let mut pairs = Vec::new();
                        for (idx, item) in items.iter().enumerate() {
                            let mut row = IndexMap::new();
                            row.insert("index".to_string(), Value::Int(idx as i64));
                            row.insert("value".to_string(), item.clone());
                            let rr = self.gc.alloc(ObjKind::Object(row));
                            pairs.push(Value::Obj(rr));
                        }
                        let nr = self.gc.alloc(ObjKind::Array(pairs));
                        return Ok(Value::Obj(nr));
                    }
                }
                Err(VMError::new("enumerate() requires an array"))
            }
            "split" => {
                if let (Some(Value::Obj(r1)), Some(Value::Obj(r2))) = (args.first(), args.get(1)) {
                    let s = self.get_string(&Value::Obj(*r1)).unwrap_or_default();
                    let delim = self.get_string(&Value::Obj(*r2)).unwrap_or_default();
                    let parts: Vec<Value> = s.split(&delim).map(|p| self.alloc_string(p)).collect();
                    let nr = self.gc.alloc(ObjKind::Array(parts));
                    return Ok(Value::Obj(nr));
                }
                Err(VMError::new("split() requires (string, delimiter)"))
            }
            "join" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let sep = args.get(1).map(|v| v.display(&self.gc)).unwrap_or_default();
                            let parts: Vec<String> =
                                items.iter().map(|v| v.display(&self.gc)).collect();
                            return Ok(self.alloc_string(&parts.join(&sep)));
                        }
                    }
                }
                Err(VMError::new("join() requires an array"))
            }
            "replace" => {
                if args.len() == 3 {
                    let s = args[0].display(&self.gc);
                    let from = args[1].display(&self.gc);
                    let to = args[2].display(&self.gc);
                    return Ok(self.alloc_string(&s.replace(&from, &to)));
                }
                Err(VMError::new("replace() requires (string, from, to)"))
            }
            "starts_with" => {
                if args.len() == 2 {
                    let s = args[0].display(&self.gc);
                    let prefix = args[1].display(&self.gc);
                    return Ok(Value::Bool(s.starts_with(&prefix)));
                }
                Err(VMError::new("starts_with() requires (string, prefix)"))
            }
            "ends_with" => {
                if args.len() == 2 {
                    let s = args[0].display(&self.gc);
                    let suffix = args[1].display(&self.gc);
                    return Ok(Value::Bool(s.ends_with(&suffix)));
                }
                Err(VMError::new("ends_with() requires (string, suffix)"))
            }
            "wait" => {
                if let Some(Value::Int(secs)) = args.first() {
                    std::thread::sleep(std::time::Duration::from_secs(*secs as u64));
                }
                Ok(Value::Null)
            }
            "uuid" => {
                let id = uuid::Uuid::new_v4().to_string();
                Ok(self.alloc_string(&id))
            }
            "json" => {
                if let Some(v) = args.first() {
                    let s = v.to_json_string(&self.gc);
                    Ok(self.alloc_string(&s))
                } else {
                    Err(VMError::new("json() requires an argument"))
                }
            }
            "is_some" | "is_none" | "satisfies" => {
                // Simplified versions
                Ok(Value::Bool(false))
            }
            n if n.starts_with("math.") => {
                crate::stdlib::math::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))
            }
            n if n.starts_with("fs.") => {
                let result =
                    crate::stdlib::fs::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::stdlib::fs::FsResult::StringVal(s) => Ok(self.alloc_string(&s)),
                    crate::stdlib::fs::FsResult::BoolVal(b) => Ok(Value::Bool(b)),
                    crate::stdlib::fs::FsResult::ArrayVal(items) => {
                        let vals: Vec<Value> = items.iter().map(|s| self.alloc_string(s)).collect();
                        let r = self.gc.alloc(ObjKind::Array(vals));
                        Ok(Value::Obj(r))
                    }
                    crate::stdlib::fs::FsResult::NullVal => Ok(Value::Null),
                }
            }
            n if n.starts_with("io.") => {
                crate::stdlib::io::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))
            }
            n if n.starts_with("crypto.") => {
                let str_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(obj) = self.gc.get(*r) {
                                if let ObjKind::String(s) = &obj.kind {
                                    return crate::interpreter::Value::String(s.clone());
                                }
                            }
                            crate::interpreter::Value::Null
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::crypto::call(n, str_args).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::interpreter::Value::String(s) => Ok(self.alloc_string(&s)),
                    _ => Ok(Value::Null),
                }
            }
            n if n.starts_with("db.") => {
                let str_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(obj) = self.gc.get(*r) {
                                if let ObjKind::String(s) = &obj.kind {
                                    return crate::interpreter::Value::String(s.clone());
                                }
                            }
                            crate::interpreter::Value::Null
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result = crate::stdlib::db::call(n, str_args).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::interpreter::Value::Bool(b) => Ok(Value::Bool(b)),
                    crate::interpreter::Value::Int(n) => Ok(Value::Int(n)),
                    crate::interpreter::Value::String(s) => Ok(self.alloc_string(&s)),
                    crate::interpreter::Value::Array(items) => {
                        let vm_items: Vec<Value> = items
                            .iter()
                            .map(|v| match v {
                                crate::interpreter::Value::Object(map) => {
                                    let mut vm_map = IndexMap::new();
                                    for (k, v) in map {
                                        let vm_v = match v {
                                            crate::interpreter::Value::Int(n) => Value::Int(*n),
                                            crate::interpreter::Value::Float(n) => Value::Float(*n),
                                            crate::interpreter::Value::String(s) => {
                                                self.alloc_string(s)
                                            }
                                            crate::interpreter::Value::Bool(b) => Value::Bool(*b),
                                            _ => Value::Null,
                                        };
                                        vm_map.insert(k.clone(), vm_v);
                                    }
                                    let r = self.gc.alloc(ObjKind::Object(vm_map));
                                    Value::Obj(r)
                                }
                                _ => Value::Null,
                            })
                            .collect();
                        let r = self.gc.alloc(ObjKind::Array(vm_items));
                        Ok(Value::Obj(r))
                    }
                    _ => Ok(Value::Null),
                }
            }
            n if n.starts_with("adt:") => {
                let parts: Vec<&str> = n.splitn(4, ':').collect();
                if parts.len() == 4 {
                    let type_name = parts[1];
                    let variant_name = parts[2];
                    let field_count: usize = parts[3].parse().unwrap_or(0);
                    if args.len() != field_count {
                        return Err(VMError::new(&format!(
                            "{}() expects {} args, got {}",
                            variant_name,
                            field_count,
                            args.len()
                        )));
                    }
                    let mut obj = IndexMap::new();
                    obj.insert("__type__".to_string(), self.alloc_string(type_name));
                    obj.insert("__variant__".to_string(), self.alloc_string(variant_name));
                    for (i, arg) in args.into_iter().enumerate() {
                        obj.insert(format!("_{}", i), arg);
                    }
                    let r = self.gc.alloc(ObjKind::Object(obj));
                    Ok(Value::Obj(r))
                } else {
                    Err(VMError::new(&format!("invalid ADT constructor: {}", n)))
                }
            }
            "fetch" => match args.first() {
                Some(Value::Obj(r)) => {
                    let url = self.get_string(&Value::Obj(*r)).unwrap_or_default();
                    let method = "GET".to_string();
                    match crate::runtime::client::fetch_blocking(&url, &method, None, None) {
                        Ok(interp_val) => Ok(self.convert_interp_value(&interp_val)),
                        Err(e) => Err(VMError::new(&format!("fetch error: {}", e))),
                    }
                }
                _ => Err(VMError::new("fetch() requires a URL string")),
            },
            "exit" => {
                let code = match args.first() {
                    Some(Value::Int(n)) => *n as i32,
                    _ => 0,
                };
                std::process::exit(code);
            }
            "run_command" => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::exec_module::call(interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("env.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::env::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("json.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        Value::Float(n) => crate::interpreter::Value::Float(*n),
                        Value::Bool(b) => crate::interpreter::Value::Bool(*b),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result = crate::stdlib::json_module::call(n, interp_args)
                    .map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("regex.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result = crate::stdlib::regex_module::call(n, interp_args)
                    .map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("log.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                crate::stdlib::log::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(Value::Null)
            }
            n if n.starts_with("http.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else if let Some(obj) = self.gc.get(*r) {
                                if let ObjKind::Object(map) = &obj.kind {
                                    let mut im = indexmap::IndexMap::new();
                                    for (k, val) in map {
                                        im.insert(k.clone(), self.convert_to_interp_val(val));
                                    }
                                    crate::interpreter::Value::Object(im)
                                } else {
                                    crate::interpreter::Value::Null
                                }
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        Value::Float(n) => crate::interpreter::Value::Float(*n),
                        Value::Bool(b) => crate::interpreter::Value::Bool(*b),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::http::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("term.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::term::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("csv.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::csv::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("time.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::time::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("pg.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::pg::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            "shell" => {
                let cmd = self.get_string_arg(&args, 0)?;
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| VMError::new(&format!("shell error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_string();
                let mut map = IndexMap::new();
                map.insert("stdout".to_string(), self.alloc_string(&stdout));
                map.insert("stderr".to_string(), self.alloc_string(&stderr));
                map.insert(
                    "status".to_string(),
                    Value::Int(output.status.code().unwrap_or(-1) as i64),
                );
                map.insert("ok".to_string(), Value::Bool(output.status.success()));
                let r = self.gc.alloc(ObjKind::Object(map));
                Ok(Value::Obj(r))
            }
            "sh" => {
                let cmd = self.get_string_arg(&args, 0)?;
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| VMError::new(&format!("sh error: {}", e)))?;
                Ok(self.alloc_string(
                    &String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_string(),
                ))
            }
            "sh_lines" => {
                let cmd = self.get_string_arg(&args, 0)?;
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| VMError::new(&format!("sh_lines error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let lines: Vec<Value> = stdout
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| self.alloc_string(l))
                    .collect();
                let r = self.gc.alloc(ObjKind::Array(lines));
                Ok(Value::Obj(r))
            }
            "sh_json" => {
                let cmd = self.get_string_arg(&args, 0)?;
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| VMError::new(&format!("sh_json error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let json: serde_json::Value = serde_json::from_str(stdout.trim())
                    .map_err(|e| VMError::new(&format!("sh_json parse error: {}", e)))?;
                let interp_val = crate::runtime::server::json_to_forge(json);
                Ok(self.convert_interp_value(&interp_val))
            }
            "sh_ok" => {
                let cmd = self.get_string_arg(&args, 0)?;
                let status = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map_err(|e| VMError::new(&format!("sh_ok error: {}", e)))?;
                Ok(Value::Bool(status.success()))
            }
            "which" => {
                let cmd = self.get_string_arg(&args, 0)?;
                let result = std::process::Command::new("/usr/bin/which")
                    .arg(&cmd)
                    .output();
                match result {
                    Ok(output) if output.status.success() => Ok(self
                        .alloc_string(&String::from_utf8_lossy(&output.stdout).trim().to_string())),
                    _ => Ok(Value::Null),
                }
            }
            "cwd" => {
                let path = std::env::current_dir()
                    .map_err(|e| VMError::new(&format!("cwd error: {}", e)))?;
                Ok(self.alloc_string(&path.display().to_string()))
            }
            "cd" => {
                let path = self.get_string_arg(&args, 0)?;
                std::env::set_current_dir(&path)
                    .map_err(|e| VMError::new(&format!("cd error: {}", e)))?;
                Ok(self.alloc_string(&path))
            }
            "lines" => {
                let text = self.get_string_arg(&args, 0)?;
                let result: Vec<Value> = text.lines().map(|l| self.alloc_string(l)).collect();
                let r = self.gc.alloc(ObjKind::Array(result));
                Ok(Value::Obj(r))
            }
            "pipe_to" => {
                let input = self.get_string_arg(&args, 0)?;
                let cmd = self.get_string_arg(&args, 1)?;
                use std::io::Write;
                let mut child = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| VMError::new(&format!("pipe_to error: {}", e)))?;
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(input.as_bytes());
                }
                let output = child
                    .wait_with_output()
                    .map_err(|e| VMError::new(&format!("pipe_to error: {}", e)))?;
                let mut map = IndexMap::new();
                map.insert(
                    "stdout".to_string(),
                    self.alloc_string(
                        &String::from_utf8_lossy(&output.stdout)
                            .trim_end()
                            .to_string(),
                    ),
                );
                map.insert(
                    "stderr".to_string(),
                    self.alloc_string(
                        &String::from_utf8_lossy(&output.stderr)
                            .trim_end()
                            .to_string(),
                    ),
                );
                map.insert(
                    "status".to_string(),
                    Value::Int(output.status.code().unwrap_or(-1) as i64),
                );
                map.insert("ok".to_string(), Value::Bool(output.status.success()));
                let r = self.gc.alloc(ObjKind::Object(map));
                Ok(Value::Obj(r))
            }
            "has_key" => {
                if let (Some(Value::Obj(r)), Some(key_val)) = (args.first(), args.get(1)) {
                    let key = self.get_string(key_val).unwrap_or_default();
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            return Ok(Value::Bool(map.contains_key(&key)));
                        }
                    }
                }
                Ok(Value::Bool(false))
            }
            "get" => {
                if let (Some(Value::Obj(r)), Some(key_val)) = (args.first(), args.get(1)) {
                    let key = self.get_string(key_val).unwrap_or_default();
                    let default = args.get(2).cloned().unwrap_or(Value::Null);
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            if key.contains('.') {
                                let parts: Vec<&str> = key.split('.').collect();
                                let mut current_map = map.clone();
                                for (i, part) in parts.iter().enumerate() {
                                    if let Some(val) = current_map.get(*part) {
                                        if i == parts.len() - 1 {
                                            return Ok(val.clone());
                                        }
                                        if let Value::Obj(inner_r) = val {
                                            if let Some(inner_obj) = self.gc.get(*inner_r) {
                                                if let ObjKind::Object(inner_map) = &inner_obj.kind
                                                {
                                                    current_map = inner_map.clone();
                                                    continue;
                                                }
                                            }
                                        }
                                        return Ok(default);
                                    } else {
                                        return Ok(default);
                                    }
                                }
                            }
                            return Ok(map.get(&key).cloned().unwrap_or(default));
                        }
                    }
                    Ok(default)
                } else {
                    Ok(Value::Null)
                }
            }
            "pick" => {
                if let (Some(Value::Obj(r)), Some(Value::Obj(keys_r))) = (args.first(), args.get(1))
                {
                    let mut result = IndexMap::new();
                    let field_names: Vec<String> = if let Some(obj) = self.gc.get(*keys_r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            items.iter().filter_map(|v| self.get_string(v)).collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            for name in &field_names {
                                if let Some(val) = map.get(name) {
                                    result.insert(name.clone(), val.clone());
                                }
                            }
                        }
                    }
                    let r = self.gc.alloc(ObjKind::Object(result));
                    Ok(Value::Obj(r))
                } else {
                    Ok(Value::Null)
                }
            }
            "omit" => {
                if let (Some(Value::Obj(r)), Some(Value::Obj(keys_r))) = (args.first(), args.get(1))
                {
                    let omit_names: Vec<String> = if let Some(obj) = self.gc.get(*keys_r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            items.iter().filter_map(|v| self.get_string(v)).collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    let mut result = IndexMap::new();
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            for (k, v) in map {
                                if !omit_names.contains(k) {
                                    result.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                    let r = self.gc.alloc(ObjKind::Object(result));
                    Ok(Value::Obj(r))
                } else {
                    Ok(Value::Null)
                }
            }
            "merge" => {
                let mut result = IndexMap::new();
                for arg in &args {
                    if let Value::Obj(r) = arg {
                        if let Some(obj) = self.gc.get(*r) {
                            if let ObjKind::Object(map) = &obj.kind {
                                for (k, v) in map {
                                    result.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
                let r = self.gc.alloc(ObjKind::Object(result));
                Ok(Value::Obj(r))
            }
            "entries" => {
                if let Some(Value::Obj(r)) = args.first() {
                    let kv_pairs: Vec<(String, Value)> = if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    if !kv_pairs.is_empty() {
                        let mut pairs = Vec::new();
                        for (k, v) in kv_pairs {
                            let key = self.alloc_string(&k);
                            let pair_r = self.gc.alloc(ObjKind::Array(vec![key, v]));
                            pairs.push(Value::Obj(pair_r));
                        }
                        let r = self.gc.alloc(ObjKind::Array(pairs));
                        return Ok(Value::Obj(r));
                    }
                }
                Ok(Value::Null)
            }
            "from_entries" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(pairs) = &obj.kind {
                            let mut result = IndexMap::new();
                            let pairs_clone = pairs.clone();
                            for pair in &pairs_clone {
                                if let Value::Obj(pr) = pair {
                                    if let Some(pobj) = self.gc.get(*pr) {
                                        if let ObjKind::Array(kv) = &pobj.kind {
                                            if let (Some(k), Some(v)) = (kv.first(), kv.get(1)) {
                                                if let Some(key) = self.get_string(k) {
                                                    result.insert(key, v.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            let r = self.gc.alloc(ObjKind::Object(result));
                            return Ok(Value::Obj(r));
                        }
                    }
                }
                Ok(Value::Null)
            }
            "find" | "flat_map" => {
                let interp_args = self.args_to_interp(&args);
                let mut interp = crate::interpreter::Interpreter::new();
                let result = interp
                    .call_builtin(name, interp_args)
                    .map_err(|e| VMError::new(&e.message))?;
                Ok(self.convert_interp_value(&result))
            }
            "ok" | "Ok" => {
                let value = args.first().cloned().unwrap_or(Value::Null);
                let r = self.gc.alloc(ObjKind::ResultOk(value));
                Ok(Value::Obj(r))
            }
            "err" | "Err" => {
                let value = args.first().cloned().unwrap_or(self.alloc_string("error"));
                let r = self.gc.alloc(ObjKind::ResultErr(value));
                Ok(Value::Obj(r))
            }
            _ => Err(VMError::new(&format!("unknown builtin: {}", name))),
        }
    }

    fn get_string_arg(&self, args: &[Value], idx: usize) -> Result<String, VMError> {
        match args.get(idx) {
            Some(v) => self
                .get_string(v)
                .ok_or_else(|| VMError::new("expected string argument")),
            None => Err(VMError::new("missing argument")),
        }
    }

    fn args_to_interp(&self, args: &[Value]) -> Vec<crate::interpreter::Value> {
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

    fn convert_to_interp_val(&self, v: &Value) -> crate::interpreter::Value {
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

    fn convert_interp_value(&mut self, v: &crate::interpreter::Value) -> Value {
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
