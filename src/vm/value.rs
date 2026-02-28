use super::bytecode::Chunk;
use indexmap::IndexMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GcRef(pub usize);

/// Runtime value. Primitives inline; heap objects via GcRef.
#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Obj(GcRef),
}

impl Value {
    pub fn is_truthy(&self, gc: &super::gc::Gc) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(n) => *n != 0.0,
            Value::Null => false,
            Value::Obj(r) => gc.get(*r).is_some_and(|obj| match &obj.kind {
                ObjKind::String(s) => !s.is_empty(),
                ObjKind::Array(a) => !a.is_empty(),
                ObjKind::Object(o) => !o.is_empty(),
                ObjKind::ResultOk(_) => true,
                ObjKind::ResultErr(_) => false,
                _ => true,
            }),
        }
    }

    pub fn type_name(&self, gc: &super::gc::Gc) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::Null => "Null",
            Value::Obj(r) => gc.get(*r).map_or("Null", |o| o.type_name()),
        }
    }

    pub fn display(&self, gc: &super::gc::Gc) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Float(n) => format!("{}", n),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Obj(r) => gc.get(*r).map_or("<freed>".to_string(), |o| o.display(gc)),
        }
    }

    pub fn to_json_string(&self, gc: &super::gc::Gc) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Float(n) => format!("{}", n),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Obj(r) => gc
                .get(*r)
                .map_or("null".to_string(), |o| o.to_json_string(gc)),
        }
    }

    pub fn equals(&self, other: &Value, gc: &super::gc::Gc) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Obj(a), Value::Obj(b)) => match (gc.get(*a), gc.get(*b)) {
                (Some(oa), Some(ob)) => oa.equals(ob, gc),
                _ => false,
            },
            _ => false,
        }
    }

    /// Check structural identity for constant dedup (no GC needed).
    #[allow(dead_code)]
    pub fn identical(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            _ => false,
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Obj(r) => write!(f, "Obj({})", r.0),
        }
    }
}

pub struct GcObject {
    pub kind: ObjKind,
    pub marked: bool,
}

impl GcObject {
    pub fn new(kind: ObjKind) -> Self {
        Self {
            kind,
            marked: false,
        }
    }

    pub fn display(&self, gc: &super::gc::Gc) -> String {
        match &self.kind {
            ObjKind::String(s) => s.clone(),
            ObjKind::Array(items) => {
                let strs: Vec<String> = items.iter().map(|v| v.display(gc)).collect();
                format!("[{}]", strs.join(", "))
            }
            ObjKind::Object(map) => {
                let entries: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("\"{}\": {}", k, v.to_json_string(gc)))
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
            ObjKind::Function(f) => format!("<fn {}>", f.name),
            ObjKind::Closure(c) => format!("<fn {}>", c.function.name),
            ObjKind::NativeFunction(n) => format!("<builtin {}>", n.name),
            ObjKind::Upvalue(uv) => uv.value.display(gc),
            ObjKind::ResultOk(v) => format!("Ok({})", v.display(gc)),
            ObjKind::ResultErr(v) => format!("Err({})", v.display(gc)),
        }
    }

    pub fn to_json_string(&self, gc: &super::gc::Gc) -> String {
        match &self.kind {
            ObjKind::String(s) => format!("\"{}\"", s),
            ObjKind::Array(items) => {
                let entries: Vec<String> = items.iter().map(|v| v.to_json_string(gc)).collect();
                format!("[{}]", entries.join(", "))
            }
            ObjKind::Object(map) => {
                let entries: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("\"{}\": {}", k, v.to_json_string(gc)))
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
            ObjKind::ResultOk(v) => format!("{{ \"Ok\": {} }}", v.to_json_string(gc)),
            ObjKind::ResultErr(v) => format!("{{ \"Err\": {} }}", v.to_json_string(gc)),
            _ => format!("\"<{}>\"", self.type_name()),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match &self.kind {
            ObjKind::String(_) => "String",
            ObjKind::Array(_) => "Array",
            ObjKind::Object(_) => "Object",
            ObjKind::Function(_) => "Function",
            ObjKind::Closure(_) => "Function",
            ObjKind::NativeFunction(_) => "BuiltIn",
            ObjKind::Upvalue(_) => "Upvalue",
            ObjKind::ResultOk(_) | ObjKind::ResultErr(_) => "Result",
        }
    }

    pub fn equals(&self, other: &GcObject, gc: &super::gc::Gc) -> bool {
        match (&self.kind, &other.kind) {
            (ObjKind::String(a), ObjKind::String(b)) => a == b,
            (ObjKind::Array(a), ObjKind::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y, gc))
            }
            _ => false,
        }
    }

    pub fn trace(&self, worklist: &mut Vec<GcRef>) {
        match &self.kind {
            ObjKind::Array(items) => {
                for item in items {
                    if let Value::Obj(r) = item {
                        worklist.push(*r);
                    }
                }
            }
            ObjKind::Object(map) => {
                for v in map.values() {
                    if let Value::Obj(r) = v {
                        worklist.push(*r);
                    }
                }
            }
            ObjKind::Closure(c) => {
                for uv in &c.upvalues {
                    worklist.push(*uv);
                }
            }
            ObjKind::Upvalue(uv) => {
                if let Value::Obj(r) = &uv.value {
                    worklist.push(*r);
                }
            }
            ObjKind::ResultOk(v) | ObjKind::ResultErr(v) => {
                if let Value::Obj(r) = v {
                    worklist.push(*r);
                }
            }
            _ => {}
        }
    }
}

#[allow(dead_code)]
pub enum ObjKind {
    String(String),
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),
    Function(ObjFunction),
    Closure(ObjClosure),
    NativeFunction(NativeFn),
    Upvalue(ObjUpvalue),
    ResultOk(Value),
    ResultErr(Value),
}

#[derive(Clone)]
pub struct ObjFunction {
    pub name: String,
    pub chunk: Chunk,
}

pub struct ObjClosure {
    pub function: ObjFunction,
    pub upvalues: Vec<GcRef>,
}

pub struct ObjUpvalue {
    pub value: Value,
}

#[allow(dead_code)]
pub struct NativeFn {
    pub name: String,
    pub func: fn(&mut super::machine::VM, Vec<Value>) -> Result<Value, String>,
}
