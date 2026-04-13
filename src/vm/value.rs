use super::bytecode::Chunk;
use super::gc::Gc;
use indexmap::IndexMap;
use std::fmt;
use std::sync::Arc;

/// Escape a string for safe JSON embedding.
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

/// GC-free value representation for crossing thread boundaries.
/// Used for spawn result slots and globals transfer during fork_for_spawn.
#[derive(Clone)]
pub enum SharedValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    String(String),
    Array(Vec<SharedValue>),
    Object(IndexMap<String, SharedValue>),
    ResultOk(Box<SharedValue>),
    ResultErr(Box<SharedValue>),
}

/// Convert a VM Value to a SharedValue (owns all data, no GcRefs).
/// Functions/closures/natives/upvalues/task handles map to Null.
pub fn value_to_shared(gc: &Gc, val: &Value) -> SharedValue {
    match val {
        Value::Int(n) => SharedValue::Int(*n),
        Value::Float(n) => SharedValue::Float(*n),
        Value::Bool(b) => SharedValue::Bool(*b),
        Value::Null => SharedValue::Null,
        Value::Obj(r) => match gc.get(*r) {
            Some(obj) => match &obj.kind {
                ObjKind::String(s) => SharedValue::String(s.clone()),
                ObjKind::Array(items) => {
                    SharedValue::Array(items.iter().map(|v| value_to_shared(gc, v)).collect())
                }
                ObjKind::Object(map) => {
                    let entries = map
                        .iter()
                        .map(|(k, v)| (k.clone(), value_to_shared(gc, v)))
                        .collect();
                    SharedValue::Object(entries)
                }
                ObjKind::ResultOk(v) => SharedValue::ResultOk(Box::new(value_to_shared(gc, v))),
                ObjKind::ResultErr(v) => SharedValue::ResultErr(Box::new(value_to_shared(gc, v))),
                ObjKind::Frozen(v) => value_to_shared(gc, v),
                // Functions, closures, natives, upvalues, task handles are not transferable
                _ => SharedValue::Null,
            },
            None => SharedValue::Null,
        },
    }
}

/// Convert a SharedValue back to a VM Value (allocates in target GC).
pub fn shared_to_value(gc: &mut Gc, sv: &SharedValue) -> Value {
    match sv {
        SharedValue::Int(n) => Value::Int(*n),
        SharedValue::Float(n) => Value::Float(*n),
        SharedValue::Bool(b) => Value::Bool(*b),
        SharedValue::Null => Value::Null,
        SharedValue::String(s) => {
            let r = gc.alloc(ObjKind::String(s.clone()));
            Value::Obj(r)
        }
        SharedValue::Array(items) => {
            let vals: Vec<Value> = items.iter().map(|sv| shared_to_value(gc, sv)).collect();
            let r = gc.alloc(ObjKind::Array(vals));
            Value::Obj(r)
        }
        SharedValue::Object(map) => {
            let entries: IndexMap<String, Value> = map
                .iter()
                .map(|(k, sv)| (k.clone(), shared_to_value(gc, sv)))
                .collect();
            let r = gc.alloc(ObjKind::Object(entries));
            Value::Obj(r)
        }
        SharedValue::ResultOk(v) => {
            let inner = shared_to_value(gc, v);
            let r = gc.alloc(ObjKind::ResultOk(inner));
            Value::Obj(r)
        }
        SharedValue::ResultErr(v) => {
            let inner = shared_to_value(gc, v);
            let r = gc.alloc(ObjKind::ResultErr(inner));
            Value::Obj(r)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GcRef(pub usize);

/// Runtime value. Primitives inline; heap objects via GcRef.
#[derive(Clone, Copy)]
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
            (Value::Obj(a), Value::Obj(b)) => {
                // Fast path: interned strings (and any shared object) have the same GcRef
                if a == b {
                    return true;
                }
                match (gc.get(*a), gc.get(*b)) {
                    (Some(oa), Some(ob)) => oa.equals(ob, gc),
                    _ => false,
                }
            }
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
                    .map(|(k, v)| format!("{}: {}", escape_json_string(k), v.to_json_string(gc)))
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
            ObjKind::Function(f) => format!("<fn {}>", f.name),
            ObjKind::Closure(c) => format!("<fn {}>", c.function.name),
            ObjKind::NativeFunction(n) => format!("<builtin {}>", n.name),
            ObjKind::Upvalue(uv) => uv.value.display(gc),
            ObjKind::ResultOk(v) => format!("Ok({})", v.display(gc)),
            ObjKind::ResultErr(v) => format!("Err({})", v.display(gc)),
            ObjKind::TaskHandle(_) => "<task>".to_string(),
            ObjKind::Frozen(v) => v.display(gc),
        }
    }

    pub fn to_json_string(&self, gc: &super::gc::Gc) -> String {
        match &self.kind {
            ObjKind::String(s) => escape_json_string(s),
            ObjKind::Array(items) => {
                let entries: Vec<String> = items.iter().map(|v| v.to_json_string(gc)).collect();
                format!("[{}]", entries.join(", "))
            }
            ObjKind::Object(map) => {
                let entries: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", escape_json_string(k), v.to_json_string(gc)))
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
            ObjKind::TaskHandle(_) => "TaskHandle",
            ObjKind::Frozen(_) => "Frozen",
        }
    }

    pub fn equals(&self, other: &GcObject, gc: &super::gc::Gc) -> bool {
        match (&self.kind, &other.kind) {
            (ObjKind::String(a), ObjKind::String(b)) => a == b,
            (ObjKind::Array(a), ObjKind::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y, gc))
            }
            (ObjKind::Object(a), ObjKind::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.get(k).map_or(false, |bv| v.equals(bv, gc)))
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
            ObjKind::ResultOk(v) | ObjKind::ResultErr(v) | ObjKind::Frozen(v) => {
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
    TaskHandle(Arc<(std::sync::Mutex<Option<SharedValue>>, std::sync::Condvar)>),
    Frozen(Value),
}

#[derive(Clone)]
pub struct ObjFunction {
    pub name: String,
    pub chunk: Arc<Chunk>,
}

pub struct ObjClosure {
    pub function: ObjFunction,
    pub upvalues: Vec<GcRef>,
}

pub struct ObjUpvalue {
    pub value: Value,
}

pub struct NativeFn {
    pub name: String,
}
