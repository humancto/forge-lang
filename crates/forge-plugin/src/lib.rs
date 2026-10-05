//! # forge-plugin — write native Forge plugins in Rust
//!
//! A Forge plugin is a shared library (`cdylib`) that exports functions
//! through Forge's stable, versioned C ABI. Forge loads it with
//! `import native "path/to/libname"` and calls its functions with typed
//! values — no shell, no text parsing.
//!
//! ```rust,ignore
//! use forge_plugin::{export, forge_fn};
//!
//! #[forge_fn]
//! fn add(a: i64, b: i64) -> i64 {
//!     a + b
//! }
//!
//! #[forge_fn]
//! fn divide(a: f64, b: f64) -> Result<f64, String> {
//!     if b == 0.0 { Err("division by zero".into()) } else { Ok(a / b) }
//! }
//!
//! export!(name = "hello", functions = [add, divide]);
//! ```
//!
//! ```toml
//! [lib]
//! crate-type = ["cdylib"]
//! ```
//!
//! ```forge
//! import native "target/release/libhello" as hello
//! say hello.add(1, 2)          // 3
//! try { hello.divide(1, 0) } catch e { say e.message }   // division by zero
//! ```
//!
//! ## Types
//!
//! | Rust | Forge |
//! | --- | --- |
//! | `()` | `null` (return only) |
//! | `bool` | `bool` |
//! | `i64`, `i32`, `u32`, `u64`, `usize` | `int` (checked on the way in) |
//! | `f64`, `f32` | `float` (an `int` argument is accepted) |
//! | `String` (`&str` return) | `string` |
//! | [`Bytes`] | array of ints 0–255 |
//! | `Vec<T>` | array |
//! | `Option<T>` | `null` or `T` |
//! | `HashMap<String, T>`, `BTreeMap<String, T>` | object |
//! | [`Value`] | anything |
//! | `Result<T, E: Display>` | `T`, or a Forge runtime error with `E`'s message (return only) |
//!
//! ## Rules
//!
//! * Functions may be called from several threads at once: keep them
//!   thread-safe (no unsynchronised global state).
//! * Panics are caught and reported as Forge errors (`add() panicked: ...`).
//!   Do not build plugins with `panic = "abort"`, or a panic aborts Forge.
//! * The library is never unloaded once Forge has loaded it.
//! * Loading native code is full trust: Forge needs `--allow-ffi` to load it.

pub mod abi;

// The macros expand to `::forge_plugin::...`; make that path work inside
// this crate too (its own tests use `#[forge_fn]`).
extern crate self as forge_plugin;

use std::collections::{BTreeMap, HashMap};
use std::fmt;

pub use forge_plugin_macros::forge_fn;

/// A dynamically typed Forge value, for functions that accept or return
/// "anything" (and the representation every argument goes through).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    /// Entries in order. Keys are unique when the value comes from Forge.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The Forge-facing type name used in conversion errors.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Bytes(_) => "bytes",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    /// Look up an object field.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// A byte buffer. Forge sees it as an array of ints 0–255; as an argument
/// it accepts such an array.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bytes(pub Vec<u8>);

/// Convert an argument from Forge. The error describes what was expected,
/// e.g. `expected int, got string`.
pub trait FromForge: Sized {
    fn from_forge(value: Value) -> Result<Self, String>;
}

/// Convert a return value to Forge.
pub trait IntoForge {
    fn into_forge(self) -> Value;
}

/// What a `#[forge_fn]` may return: any [`IntoForge`] type, or a `Result`
/// whose error becomes a Forge runtime error.
pub trait IntoForgeResult {
    fn into_forge_result(self) -> Result<Value, String>;
}

impl<T: IntoForge> IntoForgeResult for T {
    fn into_forge_result(self) -> Result<Value, String> {
        Ok(self.into_forge())
    }
}

impl<T: IntoForge, E: fmt::Display> IntoForgeResult for Result<T, E> {
    fn into_forge_result(self) -> Result<Value, String> {
        self.map(IntoForge::into_forge).map_err(|e| e.to_string())
    }
}

fn expected(what: &str, got: &Value) -> String {
    format!("expected {}, got {}", what, got.type_name())
}

impl FromForge for Value {
    fn from_forge(value: Value) -> Result<Self, String> {
        Ok(value)
    }
}

impl IntoForge for Value {
    fn into_forge(self) -> Value {
        self
    }
}

impl IntoForge for () {
    fn into_forge(self) -> Value {
        Value::Null
    }
}

impl FromForge for bool {
    fn from_forge(value: Value) -> Result<Self, String> {
        match value {
            Value::Bool(b) => Ok(b),
            other => Err(expected("bool", &other)),
        }
    }
}

impl IntoForge for bool {
    fn into_forge(self) -> Value {
        Value::Bool(self)
    }
}

impl FromForge for i64 {
    fn from_forge(value: Value) -> Result<Self, String> {
        match value {
            Value::Int(n) => Ok(n),
            other => Err(expected("int", &other)),
        }
    }
}

impl IntoForge for i64 {
    fn into_forge(self) -> Value {
        Value::Int(self)
    }
}

macro_rules! int_conversions {
    ($($t:ty),*) => {$(
        impl FromForge for $t {
            fn from_forge(value: Value) -> Result<Self, String> {
                let n = i64::from_forge(value)?;
                <$t>::try_from(n).map_err(|_| {
                    format!("{} does not fit in {}", n, stringify!($t))
                })
            }
        }

        impl IntoForge for $t {
            fn into_forge(self) -> Value {
                // Like Forge arithmetic: an int that does not fit in i64
                // becomes a float.
                match i64::try_from(self) {
                    Ok(n) => Value::Int(n),
                    Err(_) => Value::Float(self as f64),
                }
            }
        }
    )*};
}

int_conversions!(i8, i16, i32, u8, u16, u32, u64, usize, isize);

impl FromForge for f64 {
    fn from_forge(value: Value) -> Result<Self, String> {
        match value {
            Value::Float(x) => Ok(x),
            Value::Int(n) => Ok(n as f64),
            other => Err(expected("float", &other)),
        }
    }
}

impl IntoForge for f64 {
    fn into_forge(self) -> Value {
        Value::Float(self)
    }
}

impl FromForge for f32 {
    fn from_forge(value: Value) -> Result<Self, String> {
        f64::from_forge(value).map(|x| x as f32)
    }
}

impl IntoForge for f32 {
    fn into_forge(self) -> Value {
        Value::Float(self as f64)
    }
}

impl FromForge for String {
    fn from_forge(value: Value) -> Result<Self, String> {
        match value {
            Value::String(s) => Ok(s),
            other => Err(expected("string", &other)),
        }
    }
}

impl IntoForge for String {
    fn into_forge(self) -> Value {
        Value::String(self)
    }
}

impl IntoForge for &str {
    fn into_forge(self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromForge for Bytes {
    fn from_forge(value: Value) -> Result<Self, String> {
        match value {
            Value::Bytes(b) => Ok(Bytes(b)),
            Value::Array(items) => items
                .into_iter()
                .map(|item| match item {
                    Value::Int(n) => u8::try_from(n)
                        .map_err(|_| format!("byte value {} is out of range 0-255", n)),
                    other => Err(expected("an array of bytes", &other)),
                })
                .collect::<Result<Vec<u8>, String>>()
                .map(Bytes),
            other => Err(expected("bytes", &other)),
        }
    }
}

impl IntoForge for Bytes {
    fn into_forge(self) -> Value {
        Value::Bytes(self.0)
    }
}

impl<T: FromForge> FromForge for Vec<T> {
    fn from_forge(value: Value) -> Result<Self, String> {
        match value {
            Value::Array(items) => items
                .into_iter()
                .enumerate()
                .map(|(i, item)| T::from_forge(item).map_err(|e| format!("item {}: {}", i, e)))
                .collect(),
            other => Err(expected("array", &other)),
        }
    }
}

impl<T: IntoForge> IntoForge for Vec<T> {
    fn into_forge(self) -> Value {
        Value::Array(self.into_iter().map(IntoForge::into_forge).collect())
    }
}

impl<T: FromForge> FromForge for Option<T> {
    fn from_forge(value: Value) -> Result<Self, String> {
        match value {
            Value::Null => Ok(None),
            other => T::from_forge(other).map(Some),
        }
    }
}

impl<T: IntoForge> IntoForge for Option<T> {
    fn into_forge(self) -> Value {
        match self {
            Some(v) => v.into_forge(),
            None => Value::Null,
        }
    }
}

fn object_entries<T: FromForge>(value: Value) -> Result<Vec<(String, T)>, String> {
    match value {
        Value::Object(entries) => entries
            .into_iter()
            .map(|(k, v)| match T::from_forge(v) {
                Ok(v) => Ok((k, v)),
                Err(e) => Err(format!("field '{}': {}", k, e)),
            })
            .collect(),
        other => Err(expected("object", &other)),
    }
}

impl<T: FromForge> FromForge for HashMap<String, T> {
    fn from_forge(value: Value) -> Result<Self, String> {
        object_entries(value).map(|e| e.into_iter().collect())
    }
}

impl<T: IntoForge> IntoForge for HashMap<String, T> {
    fn into_forge(self) -> Value {
        // Sorted, so the object Forge sees does not depend on hash order.
        let mut entries: Vec<(String, Value)> =
            self.into_iter().map(|(k, v)| (k, v.into_forge())).collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Value::Object(entries)
    }
}

impl<T: FromForge> FromForge for BTreeMap<String, T> {
    fn from_forge(value: Value) -> Result<Self, String> {
        object_entries(value).map(|e| e.into_iter().collect())
    }
}

impl<T: IntoForge> IntoForge for BTreeMap<String, T> {
    fn into_forge(self) -> Value {
        Value::Object(self.into_iter().map(|(k, v)| (k, v.into_forge())).collect())
    }
}

/// Implemented by `#[forge_fn]` for a hidden marker type named after the
/// function; [`export!`] collects the `DEF`s into the plugin's table.
pub trait ForgeFunctionDef {
    const DEF: abi::ForgeFunction;
}

/// Export the plugin: generates `forge_plugin_abi_version`,
/// `forge_plugin_register` and the static descriptor. Use it exactly once,
/// in the crate root of the `cdylib`.
///
/// ```rust,ignore
/// forge_plugin::export!(name = "hello", functions = [add, greet]);
/// // optional: version = "1.2.3" (defaults to the crate's version)
/// ```
#[macro_export]
macro_rules! export {
    (name = $name:literal, functions = [$($f:ident),* $(,)?] $(,)?) => {
        $crate::export!(
            name = $name,
            version = ::core::env!("CARGO_PKG_VERSION"),
            functions = [$($f),*]
        );
    };
    (name = $name:literal, version = $version:expr, functions = [$($f:ident),* $(,)?] $(,)?) => {
        const _: () = {
            const __FORGE_FUNCTIONS: &[$crate::abi::ForgeFunction] =
                &[$(<$f as $crate::ForgeFunctionDef>::DEF),*];

            static __FORGE_PLUGIN: $crate::__private::Static<$crate::abi::ForgePlugin> =
                $crate::__private::Static($crate::abi::ForgePlugin {
                    abi_version: $crate::abi::ABI_VERSION,
                    name: ::core::concat!($name, "\0").as_ptr() as *const ::std::os::raw::c_char,
                    version: ::core::concat!($version, "\0").as_ptr()
                        as *const ::std::os::raw::c_char,
                    functions: __FORGE_FUNCTIONS.as_ptr(),
                    function_count: __FORGE_FUNCTIONS.len(),
                    free_value: ::core::option::Option::Some($crate::__private::free_value),
                });

            #[unsafe(no_mangle)]
            pub extern "C" fn forge_plugin_abi_version() -> u32 {
                $crate::abi::ABI_VERSION
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn forge_plugin_register() -> *const $crate::abi::ForgePlugin {
                &__FORGE_PLUGIN.0
            }
        };
    };
}

/// Implementation details used by the macros. Not a stable API.
#[doc(hidden)]
pub mod __private {
    use super::abi::{
        self, ForgeArray, ForgeBuf, ForgeEntry, ForgeObject, ForgePayload, ForgeValue,
    };
    use super::{FromForge, Value};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    const MAX_DEPTH: usize = 128;

    /// Lets the static descriptor (which holds raw pointers to static data)
    /// live in a `static`.
    #[repr(transparent)]
    pub struct Static<T>(pub T);
    // SAFETY: only ever wraps descriptors that point to immutable 'static
    // data (string literals, const tables, fn pointers).
    unsafe impl<T> Sync for Static<T> {}

    /// Take the next argument and convert it; errors name the function and
    /// parameter (`add(): argument 2 (b) expected int, got string`).
    pub fn arg<T: FromForge>(
        args: &mut std::vec::IntoIter<Value>,
        function: &str,
        position: usize,
        param: &str,
    ) -> Result<T, String> {
        let value = args
            .next()
            .ok_or_else(|| format!("{}(): missing argument {} ({})", function, position, param))?;
        T::from_forge(value)
            .map_err(|e| format!("{}(): argument {} ({}) {}", function, position, param, e))
    }

    fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
        if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic payload".to_string()
        }
    }

    /// The body of every generated `extern "C"` wrapper: read the arguments,
    /// run `f` with panics caught, write the result or the error to `out`.
    ///
    /// # Safety
    /// Called by the host per the ABI: `args` points to `argc` valid values
    /// and `out` to a writable slot.
    pub unsafe fn invoke<F>(
        name: &str,
        arity: usize,
        args: *const ForgeValue,
        argc: usize,
        out: *mut ForgeValue,
        f: F,
    ) -> i32
    where
        F: FnOnce(Vec<Value>) -> Result<Value, String>,
    {
        if out.is_null() {
            return abi::STATUS_ERR;
        }
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<ForgeValue, String> {
            if argc != arity {
                return Err(format!(
                    "{}() expects {} argument{}, got {}",
                    name,
                    arity,
                    if arity == 1 { "" } else { "s" },
                    argc
                ));
            }
            let raw = slice(args, argc)?;
            let mut values = Vec::with_capacity(argc);
            for v in raw {
                values.push(from_abi(v, 0)?);
            }
            f(values).map(into_abi)
        }));
        let (status, value) = match result {
            Ok(Ok(v)) => (abi::STATUS_OK, v),
            Ok(Err(msg)) => (abi::STATUS_ERR, into_abi(Value::String(msg))),
            Err(payload) => (
                abi::STATUS_ERR,
                into_abi(Value::String(format!(
                    "{}() panicked: {}",
                    name,
                    panic_message(payload.as_ref())
                ))),
            ),
        };
        out.write(value);
        status
    }

    unsafe fn slice<'a, T>(ptr: *const T, len: usize) -> Result<&'a [T], String> {
        if len == 0 {
            Ok(&[])
        } else if ptr.is_null() {
            Err(format!("NULL pointer with length {}", len))
        } else {
            Ok(std::slice::from_raw_parts(ptr, len))
        }
    }

    unsafe fn read_string(buf: &ForgeBuf) -> Result<String, String> {
        let bytes = slice(buf.ptr as *const u8, buf.len)?;
        std::str::from_utf8(bytes)
            .map(str::to_string)
            .map_err(|_| "a string argument is not valid UTF-8".to_string())
    }

    /// Copy a borrowed host value.
    ///
    /// # Safety
    /// `v` must be a valid ABI value.
    pub unsafe fn from_abi(v: &ForgeValue, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(format!("argument nested deeper than {} levels", MAX_DEPTH));
        }
        Ok(match v.tag {
            abi::TAG_NULL => Value::Null,
            abi::TAG_BOOL => Value::Bool(v.as_.i != 0),
            abi::TAG_INT => Value::Int(v.as_.i),
            abi::TAG_FLOAT => Value::Float(v.as_.f),
            abi::TAG_STRING => Value::String(read_string(&v.as_.buf)?),
            abi::TAG_BYTES => {
                Value::Bytes(slice(v.as_.buf.ptr as *const u8, v.as_.buf.len)?.to_vec())
            }
            abi::TAG_ARRAY => {
                let items = slice(v.as_.array.ptr as *const ForgeValue, v.as_.array.len)?;
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(from_abi(item, depth + 1)?);
                }
                Value::Array(out)
            }
            abi::TAG_OBJECT => {
                let entries = slice(v.as_.object.ptr as *const ForgeEntry, v.as_.object.len)?;
                let mut out = Vec::with_capacity(entries.len());
                for entry in entries {
                    out.push((read_string(&entry.key)?, from_abi(&entry.value, depth + 1)?));
                }
                Value::Object(out)
            }
            other => return Err(format!("unknown value tag {}", other)),
        })
    }

    fn owned_buf(bytes: Vec<u8>) -> ForgeBuf {
        let boxed = bytes.into_boxed_slice();
        let len = boxed.len();
        ForgeBuf {
            ptr: Box::into_raw(boxed) as *mut u8,
            len,
        }
    }

    /// Move a value into plugin-owned ABI memory; released by [`free_value`].
    pub fn into_abi(v: Value) -> ForgeValue {
        let (tag, as_) = match v {
            Value::Null => return ForgeValue::NULL,
            Value::Bool(b) => (abi::TAG_BOOL, ForgePayload { i: b as i64 }),
            Value::Int(n) => (abi::TAG_INT, ForgePayload { i: n }),
            Value::Float(x) => (abi::TAG_FLOAT, ForgePayload { f: x }),
            Value::String(s) => (
                abi::TAG_STRING,
                ForgePayload {
                    buf: owned_buf(s.into_bytes()),
                },
            ),
            Value::Bytes(b) => (abi::TAG_BYTES, ForgePayload { buf: owned_buf(b) }),
            Value::Array(items) => {
                let boxed: Box<[ForgeValue]> = items.into_iter().map(into_abi).collect();
                let len = boxed.len();
                (
                    abi::TAG_ARRAY,
                    ForgePayload {
                        array: ForgeArray {
                            ptr: Box::into_raw(boxed) as *mut ForgeValue,
                            len,
                        },
                    },
                )
            }
            Value::Object(entries) => {
                let boxed: Box<[ForgeEntry]> = entries
                    .into_iter()
                    .map(|(k, v)| ForgeEntry {
                        key: owned_buf(k.into_bytes()),
                        value: into_abi(v),
                    })
                    .collect();
                let len = boxed.len();
                (
                    abi::TAG_OBJECT,
                    ForgePayload {
                        object: ForgeObject {
                            ptr: Box::into_raw(boxed) as *mut ForgeEntry,
                            len,
                        },
                    },
                )
            }
        };
        ForgeValue { tag, as_ }
    }

    unsafe fn free_buf(buf: ForgeBuf) {
        if !buf.ptr.is_null() {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                buf.ptr, buf.len,
            )));
        }
    }

    unsafe fn free_in_place(v: &mut ForgeValue) {
        match v.tag {
            abi::TAG_STRING | abi::TAG_BYTES => free_buf(v.as_.buf),
            abi::TAG_ARRAY => {
                let array = v.as_.array;
                if !array.ptr.is_null() {
                    let mut items =
                        Box::from_raw(std::ptr::slice_from_raw_parts_mut(array.ptr, array.len));
                    for item in items.iter_mut() {
                        free_in_place(item);
                    }
                }
            }
            abi::TAG_OBJECT => {
                let object = v.as_.object;
                if !object.ptr.is_null() {
                    let mut entries =
                        Box::from_raw(std::ptr::slice_from_raw_parts_mut(object.ptr, object.len));
                    for entry in entries.iter_mut() {
                        free_buf(entry.key);
                        free_in_place(&mut entry.value);
                    }
                }
            }
            _ => {}
        }
        *v = ForgeValue::NULL;
    }

    /// The descriptor's `free_value`: releases a value this plugin returned.
    ///
    /// # Safety
    /// `v` must be null or a value produced by [`into_abi`] and not yet freed.
    pub unsafe extern "C" fn free_value(v: *mut ForgeValue) {
        if v.is_null() {
            return;
        }
        // Dropping plain buffers cannot panic, but never unwind into the host.
        let _ = catch_unwind(AssertUnwindSafe(|| free_in_place(&mut *v)));
    }
}

#[cfg(test)]
mod tests {
    use super::__private::{free_value, from_abi, into_abi, invoke};
    use super::*;

    fn round_trip(v: Value) -> Value {
        let mut abi = into_abi(v);
        let back = unsafe { from_abi(&abi, 0) }.expect("from_abi");
        unsafe { free_value(&mut abi) };
        assert_eq!(abi.tag, abi::TAG_NULL);
        back
    }

    #[test]
    fn values_round_trip() {
        let v = Value::Object(vec![
            ("name".into(), Value::String("forge".into())),
            ("n".into(), Value::Int(-7)),
            ("x".into(), Value::Float(2.5)),
            ("ok".into(), Value::Bool(true)),
            ("bytes".into(), Value::Bytes(vec![0, 255])),
            (
                "list".into(),
                Value::Array(vec![Value::Null, Value::Array(vec![])]),
            ),
            ("empty".into(), Value::String(String::new())),
        ]);
        assert_eq!(round_trip(v.clone()), v);
    }

    #[test]
    fn conversions() {
        assert_eq!(i64::from_forge(Value::Int(3)), Ok(3));
        assert_eq!(
            i64::from_forge(Value::String("3".into())),
            Err("expected int, got string".to_string())
        );
        assert_eq!(f64::from_forge(Value::Int(3)), Ok(3.0));
        assert_eq!(
            u8::from_forge(Value::Int(300)),
            Err("300 does not fit in u8".to_string())
        );
        assert_eq!(Option::<i64>::from_forge(Value::Null), Ok(None));
        assert_eq!(
            Vec::<i64>::from_forge(Value::Array(vec![Value::Int(1), Value::Bool(true)])),
            Err("item 1: expected int, got bool".to_string())
        );
        assert_eq!(u64::MAX.into_forge(), Value::Float(u64::MAX as f64));
        let mut m = HashMap::new();
        m.insert("b".to_string(), 2i64);
        m.insert("a".to_string(), 1i64);
        assert_eq!(
            m.into_forge(),
            Value::Object(vec![
                ("a".into(), Value::Int(1)),
                ("b".into(), Value::Int(2))
            ])
        );
        assert_eq!(
            Bytes::from_forge(Value::Array(vec![Value::Int(1), Value::Int(2)])),
            Ok(Bytes(vec![1, 2]))
        );
        let r: Result<i64, String> = Err("nope".into());
        assert_eq!(r.into_forge_result(), Err("nope".to_string()));
    }

    #[test]
    fn invoke_reports_errors_and_panics_without_unwinding() {
        let mut out = ForgeValue::NULL;
        let status = unsafe {
            invoke(
                "boom",
                0,
                std::ptr::null(),
                0,
                &mut out,
                |_| -> Result<Value, String> { panic!("kaboom") },
            )
        };
        assert_eq!(status, abi::STATUS_ERR);
        let msg = unsafe { from_abi(&out, 0) }.expect("message");
        unsafe { free_value(&mut out) };
        assert_eq!(msg, Value::String("boom() panicked: kaboom".into()));

        let arg = into_abi(Value::Int(1));
        let status = unsafe { invoke("two", 2, &arg, 1, &mut out, |_| Ok(Value::Null)) };
        assert_eq!(status, abi::STATUS_ERR);
        let msg = unsafe { from_abi(&out, 0) }.expect("message");
        unsafe { free_value(&mut out) };
        assert_eq!(
            msg,
            Value::String("two() expects 2 arguments, got 1".into())
        );
    }

    use super::abi::ForgeValue;

    #[forge_fn]
    fn add(a: i64, b: i64) -> i64 {
        a + b
    }

    #[forge_fn(name = "safe_div")]
    fn divide(a: f64, b: f64) -> Result<f64, String> {
        if b == 0.0 {
            Err("division by zero".into())
        } else {
            Ok(a / b)
        }
    }

    fn call(def: &abi::ForgeFunction, args: Vec<Value>) -> (i32, Value) {
        let raw: Vec<ForgeValue> = args.into_iter().map(into_abi).collect();
        let mut out = ForgeValue::NULL;
        let f = def.call.expect("call");
        let status = unsafe { f(raw.as_ptr(), raw.len(), &mut out) };
        let value = unsafe { from_abi(&out, 0) }.expect("result");
        unsafe {
            free_value(&mut out);
            for mut v in raw {
                free_value(&mut v);
            }
        }
        (status, value)
    }

    #[test]
    fn forge_fn_generates_a_working_definition() {
        let def = <add as ForgeFunctionDef>::DEF;
        assert_eq!(def.arity, 2);
        let name = unsafe { std::ffi::CStr::from_ptr(def.name) };
        assert_eq!(name.to_str(), Ok("add"));
        assert_eq!(
            call(&def, vec![Value::Int(2), Value::Int(3)]),
            (0, Value::Int(5))
        );
        assert_eq!(
            call(&def, vec![Value::Int(2), Value::String("x".into())]),
            (
                1,
                Value::String("add(): argument 2 (b) expected int, got string".into())
            )
        );

        let def = <divide as ForgeFunctionDef>::DEF;
        let name = unsafe { std::ffi::CStr::from_ptr(def.name) };
        assert_eq!(name.to_str(), Ok("safe_div"));
        assert_eq!(
            call(&def, vec![Value::Int(1), Value::Int(0)]),
            (1, Value::String("division by zero".into()))
        );
        // The Rust function itself is untouched.
        assert_eq!(add(1, 1), 2);
    }
}
