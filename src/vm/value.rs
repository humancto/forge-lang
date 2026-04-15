use super::bytecode::Chunk;
use super::gc::Gc;
use super::nanbox::NanBoxedValue;
use indexmap::IndexMap;
use std::fmt;
use std::sync::mpsc::{Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};

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

/// Sender half of a VM channel. Bounded uses SyncSender for backpressure.
pub enum VmChannelSender {
    Bounded(SyncSender<SharedValue>),
    Unbounded(Sender<SharedValue>),
}

/// Thread-safe channel internals. Both sender and receiver are wrapped in
/// Mutex<Option<...>> so `close()` can set sender to None.
pub struct VmChannelInner {
    pub sender: Mutex<Option<VmChannelSender>>,
    pub receiver: Mutex<Option<Receiver<SharedValue>>>,
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
    Channel(Arc<VmChannelInner>),
    Tuple(Vec<SharedValue>),
    Set(Vec<SharedValue>),
    Map(Vec<(SharedValue, SharedValue)>),
}

/// Convert a VM Value to a SharedValue (owns all data, no GcRefs).
/// Functions/closures/natives/upvalues/task handles map to Null.
pub fn value_to_shared(gc: &Gc, val: &Value) -> SharedValue {
    match val.classify(gc) {
        ValueKind::Int(n) => SharedValue::Int(n),
        ValueKind::Float(n) => SharedValue::Float(n),
        ValueKind::Bool(b) => SharedValue::Bool(b),
        ValueKind::Null => SharedValue::Null,
        ValueKind::Obj(r) => match gc.get(r) {
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
                ObjKind::Channel(ch) => SharedValue::Channel(ch.clone()),
                ObjKind::Frozen(v) => value_to_shared(gc, v),
                ObjKind::Tuple(items) => {
                    SharedValue::Tuple(items.iter().map(|v| value_to_shared(gc, v)).collect())
                }
                ObjKind::Set(items) => {
                    SharedValue::Set(items.iter().map(|v| value_to_shared(gc, v)).collect())
                }
                ObjKind::Map(pairs) => SharedValue::Map(
                    pairs
                        .iter()
                        .map(|(k, v)| (value_to_shared(gc, k), value_to_shared(gc, v)))
                        .collect(),
                ),
                ObjKind::BoxedInt(n) => SharedValue::Int(*n),
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
        SharedValue::Int(n) => Value::int(*n, gc),
        SharedValue::Float(n) => Value::float(*n),
        SharedValue::Bool(b) => Value::bool_val(*b),
        SharedValue::Null => Value::null(),
        SharedValue::String(s) => {
            let r = gc.alloc(ObjKind::String(s.clone()));
            Value::obj(r)
        }
        SharedValue::Array(items) => {
            let vals: Vec<Value> = items.iter().map(|sv| shared_to_value(gc, sv)).collect();
            let r = gc.alloc(ObjKind::Array(vals));
            Value::obj(r)
        }
        SharedValue::Object(map) => {
            let entries: IndexMap<String, Value> = map
                .iter()
                .map(|(k, sv)| (k.clone(), shared_to_value(gc, sv)))
                .collect();
            let r = gc.alloc(ObjKind::Object(entries));
            Value::obj(r)
        }
        SharedValue::ResultOk(v) => {
            let inner = shared_to_value(gc, v);
            let r = gc.alloc(ObjKind::ResultOk(inner));
            Value::obj(r)
        }
        SharedValue::ResultErr(v) => {
            let inner = shared_to_value(gc, v);
            let r = gc.alloc(ObjKind::ResultErr(inner));
            Value::obj(r)
        }
        SharedValue::Channel(ch) => {
            let r = gc.alloc(ObjKind::Channel(ch.clone()));
            Value::obj(r)
        }
        SharedValue::Tuple(items) => {
            let vals: Vec<Value> = items.iter().map(|sv| shared_to_value(gc, sv)).collect();
            let r = gc.alloc(ObjKind::Tuple(vals));
            Value::obj(r)
        }
        SharedValue::Set(items) => {
            let vals: Vec<Value> = items.iter().map(|sv| shared_to_value(gc, sv)).collect();
            let r = gc.alloc(ObjKind::Set(vals));
            Value::obj(r)
        }
        SharedValue::Map(pairs) => {
            let vals: Vec<(Value, Value)> = pairs
                .iter()
                .map(|(k, v)| (shared_to_value(gc, k), shared_to_value(gc, v)))
                .collect();
            let r = gc.alloc(ObjKind::Map(vals));
            Value::obj(r)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GcRef(pub usize);

/// Deconstructed value for exhaustive pattern matching.
/// Use `val.classify(&gc)` to get this from a `Value`.
pub enum ValueKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Obj(GcRef),
}

/// Runtime value. NaN-boxed into 8 bytes.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Value(pub(crate) NanBoxedValue);

impl Value {
    // ---- Constructors ----

    /// Create an integer value. For values known to be small, prefer `small_int`.
    /// This allocates a BoxedInt on the GC heap if the value exceeds 48-bit range.
    #[inline]
    pub fn int(n: i64, gc: &mut Gc) -> Value {
        match NanBoxedValue::try_from_int(n) {
            Some(nb) => Value(nb),
            None => {
                let r = gc.alloc(ObjKind::BoxedInt(n));
                Value(NanBoxedValue::from_obj(r))
            }
        }
    }

    /// Create an integer value that is known to fit in 48 bits.
    /// Panics in debug mode if the value is out of range.
    #[inline]
    pub fn small_int(n: i64) -> Value {
        Value(NanBoxedValue::from_small_int(n))
    }

    #[inline]
    pub fn float(f: f64) -> Value {
        Value(NanBoxedValue::from_float(f))
    }

    #[inline]
    pub fn bool_val(b: bool) -> Value {
        Value(NanBoxedValue::from_bool(b))
    }

    #[inline]
    pub fn null() -> Value {
        Value(NanBoxedValue::null())
    }

    #[inline]
    pub fn obj(r: GcRef) -> Value {
        Value(NanBoxedValue::from_obj(r))
    }

    // ---- Extractors (BoxedInt-aware) ----

    /// Extract an integer, checking both inline Int and heap BoxedInt.
    #[inline]
    pub fn as_int(&self, gc: &Gc) -> Option<i64> {
        if let Some(n) = self.0.as_int() {
            return Some(n);
        }
        if let Some(r) = self.0.as_obj() {
            if let Some(obj) = gc.get(r) {
                if let ObjKind::BoxedInt(n) = &obj.kind {
                    return Some(*n);
                }
            }
        }
        None
    }

    /// Extract an inline integer only (no GC lookup). Use in hot paths
    /// where BoxedInt is impossible (e.g., loop counters, small constants).
    #[inline]
    pub fn as_inline_int(&self) -> Option<i64> {
        self.0.as_int()
    }

    #[inline]
    pub fn as_float(&self) -> Option<f64> {
        self.0.as_float()
    }

    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        self.0.as_bool()
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    #[inline]
    pub fn is_int(&self, gc: &Gc) -> bool {
        self.as_int(gc).is_some()
    }

    #[inline]
    pub fn as_obj(&self) -> Option<GcRef> {
        self.0.as_obj()
    }

    /// Deconstruct into a matchable enum for exhaustive pattern matching.
    /// BoxedInt is transparently unwrapped to `ValueKind::Int`.
    pub fn classify(&self, gc: &Gc) -> ValueKind {
        if let Some(n) = self.0.as_int() {
            return ValueKind::Int(n);
        }
        if let Some(f) = self.0.as_float() {
            return ValueKind::Float(f);
        }
        if let Some(b) = self.0.as_bool() {
            return ValueKind::Bool(b);
        }
        if self.0.is_null() {
            return ValueKind::Null;
        }
        if let Some(r) = self.0.as_obj() {
            if let Some(obj) = gc.get(r) {
                if let ObjKind::BoxedInt(n) = &obj.kind {
                    return ValueKind::Int(*n);
                }
            }
            return ValueKind::Obj(r);
        }
        ValueKind::Null
    }

    // ---- Existing methods ----

    pub fn is_truthy(&self, gc: &Gc) -> bool {
        self.0.is_truthy(gc)
    }

    pub fn type_name(&self, gc: &Gc) -> &'static str {
        self.0.type_name(gc)
    }

    pub fn display(&self, gc: &Gc) -> String {
        self.0.display(gc)
    }

    pub fn to_json_string(&self, gc: &Gc) -> String {
        self.0.to_json_string(gc)
    }

    pub fn equals(&self, other: &Value, gc: &Gc) -> bool {
        // Scalar equality — follows IEEE-754 (NaN != NaN). Set-specific
        // equality (dedup, .has(), set operations) uses `Value::set_eq`
        // below which NaN-equates for container-membership purposes.
        match (self.classify(gc), other.classify(gc)) {
            (ValueKind::Int(a), ValueKind::Int(b)) => a == b,
            (ValueKind::Float(a), ValueKind::Float(b)) => a == b,
            (ValueKind::Int(a), ValueKind::Float(b)) => (a as f64) == b,
            (ValueKind::Float(a), ValueKind::Int(b)) => a == (b as f64),
            (ValueKind::Bool(a), ValueKind::Bool(b)) => a == b,
            (ValueKind::Null, ValueKind::Null) => true,
            (ValueKind::Obj(a), ValueKind::Obj(b)) => {
                if a == b {
                    return true;
                }
                match (gc.get(a), gc.get(b)) {
                    (Some(oa), Some(ob)) => oa.equals(ob, gc),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Equality for set membership / dedup. Like `equals`, but NaN is
    /// considered equal to itself so that `set([nan]).has(nan)` works
    /// and NaN isn't duplicated in a set. Language-level `==` still
    /// uses `equals` (IEEE-754 compliant).
    pub fn set_eq(&self, other: &Value, gc: &Gc) -> bool {
        match (self.classify(gc), other.classify(gc)) {
            (ValueKind::Float(a), ValueKind::Float(b)) => a == b || (a.is_nan() && b.is_nan()),
            _ => self.equals(other, gc),
        }
    }

    /// Check structural identity for constant dedup (no GC needed).
    #[allow(dead_code)]
    pub fn identical(&self, other: &Value) -> bool {
        self.0.identical(&other.0)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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

    pub fn display(&self, gc: &Gc) -> String {
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
            ObjKind::Channel(_) => "<channel>".to_string(),
            ObjKind::Frozen(v) => v.display(gc),
            ObjKind::Tuple(items) => {
                let strs: Vec<String> = items.iter().map(|v| v.display(gc)).collect();
                format!("({})", strs.join(", "))
            }
            ObjKind::Set(items) => {
                let strs: Vec<String> = items.iter().map(|v| v.display(gc)).collect();
                format!("set({})", strs.join(", "))
            }
            ObjKind::Map(pairs) => {
                let strs: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{} => {}", k.display(gc), v.display(gc)))
                    .collect();
                format!("Map({})", strs.join(", "))
            }
            ObjKind::BoxedInt(n) => n.to_string(),
        }
    }

    pub fn to_json_string(&self, gc: &Gc) -> String {
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
            ObjKind::Tuple(items) | ObjKind::Set(items) => {
                let entries: Vec<String> = items.iter().map(|v| v.to_json_string(gc)).collect();
                format!("[{}]", entries.join(", "))
            }
            ObjKind::Map(pairs) => {
                // Display-only lossy form; `json.stringify(m)` errors on
                // non-string keys in stdlib/json_module.rs.
                let entries: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("[{}, {}]", k.to_json_string(gc), v.to_json_string(gc)))
                    .collect();
                format!("[{}]", entries.join(", "))
            }
            ObjKind::BoxedInt(n) => n.to_string(),
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
            ObjKind::Channel(_) => "channel",
            ObjKind::Frozen(_) => "Frozen",
            ObjKind::Tuple(_) => "Tuple",
            ObjKind::Set(_) => "Set",
            ObjKind::Map(_) => "Map",
            ObjKind::BoxedInt(_) => "Int",
        }
    }

    pub fn equals(&self, other: &GcObject, gc: &Gc) -> bool {
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
            (ObjKind::Tuple(a), ObjKind::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y, gc))
            }
            (ObjKind::Set(a), ObjKind::Set(b)) => {
                // Order-independent: same length + every element in A is in B.
                // Uses set_eq so NaN elements match themselves.
                a.len() == b.len() && a.iter().all(|x| b.iter().any(|y| x.set_eq(y, gc)))
            }
            (ObjKind::Map(a), ObjKind::Map(b)) => {
                // Order-independent on entries. Keys compared with set_eq
                // (NaN self-match, Int/Float promote). Values with equals.
                a.len() == b.len()
                    && a.iter().all(|(ak, av)| {
                        b.iter()
                            .any(|(bk, bv)| ak.set_eq(bk, gc) && av.equals(bv, gc))
                    })
            }
            (ObjKind::BoxedInt(a), ObjKind::BoxedInt(b)) => a == b,
            _ => false,
        }
    }

    pub fn trace(&self, worklist: &mut Vec<GcRef>) {
        match &self.kind {
            ObjKind::Array(items) | ObjKind::Tuple(items) | ObjKind::Set(items) => {
                for item in items {
                    if let Some(r) = item.as_obj() {
                        worklist.push(r);
                    }
                }
            }
            ObjKind::Object(map) => {
                for v in map.values() {
                    if let Some(r) = v.as_obj() {
                        worklist.push(r);
                    }
                }
            }
            ObjKind::Map(pairs) => {
                for (k, v) in pairs {
                    if let Some(r) = k.as_obj() {
                        worklist.push(r);
                    }
                    if let Some(r) = v.as_obj() {
                        worklist.push(r);
                    }
                }
            }
            ObjKind::Closure(c) => {
                for uv in &c.upvalues {
                    worklist.push(*uv);
                }
            }
            ObjKind::Upvalue(uv) => {
                if let Some(r) = uv.value.as_obj() {
                    worklist.push(r);
                }
            }
            ObjKind::ResultOk(v) | ObjKind::ResultErr(v) | ObjKind::Frozen(v) => {
                if let Some(r) = v.as_obj() {
                    worklist.push(r);
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
    Channel(Arc<VmChannelInner>),
    Frozen(Value),
    /// Immutable, fixed-length, heterogeneous collection.
    Tuple(Vec<Value>),
    /// Unordered collection of unique elements.
    Set(Vec<Value>),
    /// First-class Map with any-value keys. Ordered vec preserves insertion
    /// order; keys compared via `Value::set_eq` so NaN self-matches and
    /// Int/Float promote (e.g. `1` and `1.0` collide).
    Map(Vec<(Value, Value)>),
    /// Heap-boxed i64 for values exceeding 48-bit NaN-box inline range.
    /// Used when NaN-boxed Value is active; transparent to user code.
    BoxedInt(i64),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_is_8_bytes() {
        assert_eq!(std::mem::size_of::<Value>(), 8);
    }
}
