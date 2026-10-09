//! Native plugins: loading shared libraries that implement the Forge plugin
//! ABI and calling their functions from either engine.
//!
//! Design: `rfcs/0006-native-plugins.md`. ABI: [`abi`] and
//! `crates/forge-plugin/include/forge_plugin.h`.
//!
//! # How the engines use this module
//!
//! * `import native "path" [as name]` / `import { f } from native "path"`
//!   calls [`import`], which resolves the path ([`resolve_library`]), checks
//!   the `ffi` capability against the canonical path, loads the library once
//!   per process ([`load`]) and returns a namespace object whose members are
//!   `Value::BuiltIn("native:<lib>:<fn>")`.
//! * Both engines route builtin names with the [`FN_PREFIX`] prefix to
//!   [`call`] (the VM converts its values at the boundary, like stdlib
//!   module calls), so a plugin function behaves identically everywhere.
//!
//! # Invariants
//!
//! * The `ffi` check happens **before** the library is opened: opening runs
//!   the library's initialisers, which is already arbitrary code.
//! * Libraries are never unloaded. Function values can live anywhere
//!   (closures, globals, other threads), so the code they point to must
//!   stay mapped for the life of the process. The registry is append-only
//!   and its entries are leaked on purpose.
//! * Arguments are borrowed by the plugin for the duration of one call
//!   (they point into the caller's `Vec<Value>` and a per-call arena);
//!   results are copied out and then released with the plugin's own
//!   `free_value`. The host never frees plugin memory itself.

pub mod abi;

use crate::interpreter::Value;
use abi::{ForgeArray, ForgeBuf, ForgeEntry, ForgeObject, ForgePayload, ForgeValue};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};

/// Prefix of the builtin names that identify plugin functions:
/// `native:<library index>:<function name>`.
pub const FN_PREFIX: &str = "native:";

/// Maximum nesting of arrays/objects converted in either direction.
const MAX_DEPTH: usize = 128;

/// Upper bound on the functions one library may export (sanity check on a
/// corrupt descriptor).
const MAX_FUNCTIONS: usize = 65_536;

struct Function {
    name: String,
    /// `None` = variadic.
    arity: Option<usize>,
    call: abi::ForgeFnPtr,
}

struct Library {
    functions: Vec<Function>,
    by_name: HashMap<String, usize>,
    free_value: unsafe extern "C" fn(*mut ForgeValue),
    /// Keeps the code mapped. Never dropped: see the module invariants.
    #[cfg(feature = "host")]
    _handle: libloading::Library,
}

#[derive(Default)]
struct Registry {
    libraries: Vec<&'static Library>,
    by_path: HashMap<PathBuf, usize>,
}

fn registry() -> &'static RwLock<Registry> {
    static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(Registry::default()))
}

/// Serialises loading so two threads importing the same library at once
/// open it only once.
fn load_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Whether `name` is a plugin function (`native:...`).
pub fn is_plugin_fn(name: &str) -> bool {
    name.starts_with(FN_PREFIX)
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn has_library_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("so" | "dylib" | "dll")
    )
}

/// The file names `spec` may refer to on this platform, in lookup order.
///
/// `"dir/hello"` → `dir/hello.so`, `dir/libhello.so` (Linux);
/// `"dir/libhello"` → `dir/libhello.dll`, `dir/hello.dll` (Windows);
/// a path that already ends in `.so`/`.dylib`/`.dll` is used as is.
fn candidate_paths(spec: &str) -> Vec<PathBuf> {
    let path = Path::new(spec);
    if has_library_extension(path) {
        return vec![path.to_path_buf()];
    }
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let suffix = std::env::consts::DLL_SUFFIX;
    let prefix = std::env::consts::DLL_PREFIX;
    let mut out = vec![parent.join(format!("{}{}", file, suffix))];
    if !prefix.is_empty() && !file.starts_with(prefix) {
        out.push(parent.join(format!("{}{}{}", prefix, file, suffix)));
    } else if prefix.is_empty() {
        if let Some(stripped) = file.strip_prefix("lib") {
            out.push(parent.join(format!("{}{}", stripped, suffix)));
        }
    }
    out
}

/// The namespace name `import native "<spec>"` binds when no `as` is given:
/// the file stem without the platform prefix/suffix (`libhello_rust.so` →
/// `hello_rust`). `None` when that is not an identifier.
pub fn default_namespace(spec: &str) -> Option<String> {
    let path = Path::new(spec);
    let stem = if has_library_extension(path) {
        path.file_stem()?
    } else {
        path.file_name()?
    };
    let stem = stem.to_str()?;
    let name = stem
        .strip_prefix("lib")
        .filter(|s| !s.is_empty())
        .unwrap_or(stem);
    is_identifier(name).then(|| name.to_string())
}

/// Resolve a library path the way `import` resolves modules: relative to
/// the importing file's directory first, then the working directory. There
/// is deliberately no search path (`LD_LIBRARY_PATH`, system directories):
/// a plugin is always a file the program names.
pub fn resolve_library(spec: &str, base_dir: Option<&Path>) -> Result<PathBuf, String> {
    if spec.trim().is_empty() {
        return Err("import native: the library path is empty".to_string());
    }
    let candidates = candidate_paths(spec);
    let mut bases: Vec<Option<&Path>> = Vec::new();
    if !Path::new(spec).is_absolute() {
        if let Some(base) = base_dir {
            bases.push(Some(base));
        }
    }
    bases.push(None);
    for base in &bases {
        for candidate in &candidates {
            let full = match base {
                Some(dir) => dir.join(candidate),
                None => candidate.clone(),
            };
            if full.is_file() {
                return Ok(std::fs::canonicalize(&full).unwrap_or(full));
            }
        }
    }
    let tried: Vec<String> = candidates
        .iter()
        .filter_map(|c| c.file_name().map(|f| f.to_string_lossy().into_owned()))
        .collect();
    Err(format!(
        "native library '{}' not found (looked for {} next to the importing file and in the working directory)",
        spec,
        tried.join(", ")
    ))
}

/// Read a NUL-terminated UTF-8 string from a plugin descriptor.
///
/// # Safety
/// `ptr` must be null or point to a NUL-terminated string that stays valid.
unsafe fn descriptor_str(ptr: *const std::os::raw::c_char, what: &str) -> Result<String, String> {
    if ptr.is_null() {
        return Ok(String::new());
    }
    CStr::from_ptr(ptr)
        .to_str()
        .map(str::to_string)
        .map_err(|_| format!("{} is not valid UTF-8", what))
}

/// Load (once per process) the library at the canonical path `path` and
/// validate its descriptor. The caller has already checked `ffi`.
fn load(path: &Path, display: &str) -> Result<usize, String> {
    let _guard = load_lock().lock().unwrap_or_else(|e| e.into_inner());
    {
        let reg = registry().read().unwrap_or_else(|e| e.into_inner());
        if let Some(&id) = reg.by_path.get(path) {
            return Ok(id);
        }
    }
    let library = open_library(path, display)?;
    let mut reg = registry().write().unwrap_or_else(|e| e.into_inner());
    let id = reg.libraries.len();
    reg.libraries.push(Box::leak(Box::new(library)));
    reg.by_path.insert(path.to_path_buf(), id);
    Ok(id)
}

/// Without the host runtime (browser playground) there is no dynamic
/// loader; `import native` fails after the usual resolution and `ffi` check.
#[cfg(not(feature = "host"))]
fn open_library(_path: &Path, _display: &str) -> Result<Library, String> {
    Err(crate::runtime::unavailable_message("`import native`"))
}

#[cfg(feature = "host")]
fn open_library(path: &Path, display: &str) -> Result<Library, String> {
    let fail = |msg: String| format!("cannot load native library '{}': {}", display, msg);
    // SAFETY: loading a library runs its initialisers. That is exactly the
    // trust the `ffi` capability grants; the caller checked it.
    let handle = unsafe { libloading::Library::new(path) }.map_err(|e| fail(e.to_string()))?;

    // SAFETY: the symbol types are the ABI's; a library that exports these
    // names with other signatures violates the plugin contract.
    let version = unsafe {
        let f = handle
            .get::<abi::AbiVersionFn>(abi::SYM_ABI_VERSION)
            .map_err(|_| {
                fail("it is not a Forge plugin (no forge_plugin_abi_version symbol)".to_string())
            })?;
        f()
    };
    if version != abi::ABI_VERSION {
        return Err(fail(format!(
            "it was built for plugin ABI version {}, but this Forge supports version {} (rebuild it against a matching forge-plugin)",
            version,
            abi::ABI_VERSION
        )));
    }
    let descriptor = unsafe {
        let f = handle
            .get::<abi::RegisterFn>(abi::SYM_REGISTER)
            .map_err(|_| fail("it does not export forge_plugin_register".to_string()))?;
        f()
    };
    if descriptor.is_null() {
        return Err(fail("forge_plugin_register returned NULL".to_string()));
    }
    // SAFETY: non-null, and the ABI requires the descriptor to be static.
    let desc = unsafe { &*descriptor };
    if desc.abi_version != abi::ABI_VERSION {
        return Err(fail(format!(
            "its descriptor declares ABI version {}, expected {}",
            desc.abi_version,
            abi::ABI_VERSION
        )));
    }
    let free_value = desc
        .free_value
        .ok_or_else(|| fail("its descriptor has no free_value function".to_string()))?;
    if desc.function_count > MAX_FUNCTIONS {
        return Err(fail(format!(
            "its descriptor claims {} functions",
            desc.function_count
        )));
    }
    if desc.function_count > 0 && desc.functions.is_null() {
        return Err(fail("its function table is NULL".to_string()));
    }
    let table: &[abi::ForgeFunction] = if desc.function_count == 0 {
        &[]
    } else {
        // SAFETY: non-null and `function_count` entries per the ABI.
        unsafe { std::slice::from_raw_parts(desc.functions, desc.function_count) }
    };
    let mut functions = Vec::with_capacity(table.len());
    let mut by_name = HashMap::with_capacity(table.len());
    for (i, entry) in table.iter().enumerate() {
        // SAFETY: the ABI requires NUL-terminated static strings.
        let name = unsafe { descriptor_str(entry.name, &format!("function #{} name", i)) }
            .map_err(fail)?;
        if !is_identifier(&name) {
            return Err(fail(format!(
                "function #{} has an invalid name '{}' (must be an identifier)",
                i, name
            )));
        }
        let call = entry
            .call
            .ok_or_else(|| fail(format!("function '{}' has a NULL call pointer", name)))?;
        let arity = match entry.arity {
            -1 => None,
            n if n >= 0 => Some(n as usize),
            n => {
                return Err(fail(format!(
                    "function '{}' has an invalid arity {}",
                    name, n
                )))
            }
        };
        if by_name.insert(name.clone(), functions.len()).is_some() {
            return Err(fail(format!("function '{}' is exported twice", name)));
        }
        functions.push(Function { name, arity, call });
    }
    Ok(Library {
        functions,
        by_name,
        free_value,
        _handle: handle,
    })
}

fn library(id: usize) -> Option<&'static Library> {
    let reg = registry().read().unwrap_or_else(|e| e.into_inner());
    reg.libraries.get(id).copied()
}

/// `import native "<spec>"`: resolve, check `ffi`, load, and return the
/// namespace object (function name → callable). With `names`, every name
/// must be exported; the error lists what the library does export.
pub fn import(
    spec: &str,
    base_dir: Option<&Path>,
    names: Option<&[String]>,
) -> Result<Value, String> {
    let path = resolve_library(spec, base_dir)?;
    crate::permissions::require_ffi(&path).map_err(|e| e.to_string())?;
    let id = load(&path, spec)?;
    let lib =
        library(id).ok_or_else(|| "BUG: native library vanished from the registry".to_string())?;
    if let Some(names) = names {
        for name in names {
            if !lib.by_name.contains_key(name) {
                let exported: Vec<&str> = lib.functions.iter().map(|f| f.name.as_str()).collect();
                return Err(format!(
                    "native library '{}' has no function '{}' (it exports: {})",
                    spec,
                    name,
                    if exported.is_empty() {
                        "nothing".to_string()
                    } else {
                        exported.join(", ")
                    }
                ));
            }
        }
    }
    let mut namespace = IndexMap::with_capacity(lib.functions.len());
    for f in &lib.functions {
        namespace.insert(
            f.name.clone(),
            Value::BuiltIn(format!("{}{}:{}", FN_PREFIX, id, f.name)),
        );
    }
    Ok(Value::Object(namespace))
}

fn lookup(qualified: &str) -> Result<(&'static Library, &'static Function), String> {
    let rest = qualified
        .strip_prefix(FN_PREFIX)
        .ok_or_else(|| format!("unknown native function: {}", qualified))?;
    let (id, name) = rest
        .split_once(':')
        .ok_or_else(|| format!("unknown native function: {}", qualified))?;
    let lib = id
        .parse::<usize>()
        .ok()
        .and_then(library)
        .ok_or_else(|| format!("unknown native function: {}", qualified))?;
    let f = lib
        .by_name
        .get(name)
        .map(|&i| &lib.functions[i])
        .ok_or_else(|| format!("unknown native function: {}", qualified))?;
    Ok((lib, f))
}

/// Call a plugin function. Arity errors, unconvertible arguments, plugin
/// errors and panics (reported by the SDK) all become `Err(message)`.
pub fn call(qualified: &str, args: Vec<Value>) -> Result<Value, String> {
    let (lib, f) = lookup(qualified)?;
    if let Some(n) = f.arity {
        if args.len() != n {
            return Err(format!(
                "{}() expects {}, got {}",
                f.name,
                crate::builtins_registry::Arity::Range(n, n).describe(),
                args.len()
            ));
        }
    }
    let mut arena = Arena::default();
    let mut abi_args = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        let v = to_abi(arg, &mut arena, 0).map_err(|what| {
            format!(
                "{}(): cannot pass argument {} to a native function: {}",
                f.name,
                i + 1,
                what
            )
        })?;
        abi_args.push(v);
    }
    let mut out = ForgeValue::NULL;
    // SAFETY: the plugin contract — `args` (and everything reachable from
    // it, which lives in `args`/`arena`) is valid for the call; `out` is a
    // valid, NULL-initialised slot.
    let status = unsafe { (f.call)(abi_args.as_ptr(), abi_args.len(), &mut out) };
    drop(abi_args);
    drop(arena);
    // SAFETY: `out` was filled by the plugin per the ABI.
    let converted = unsafe { from_abi(&out, 0) };
    // SAFETY: the value came from this plugin and is released exactly once.
    unsafe { (lib.free_value)(&mut out) };
    match status {
        abi::STATUS_OK => {
            converted.map_err(|e| format!("{}() returned a value Forge cannot read: {}", f.name, e))
        }
        abi::STATUS_ERR => Err(match converted {
            Ok(Value::String(msg)) if !msg.is_empty() => msg,
            _ => format!("{}() failed", f.name),
        }),
        other => Err(format!(
            "{}() returned an invalid status code {}",
            f.name, other
        )),
    }
}

/// Backing storage for one call's arguments. Boxed slices never move their
/// contents, so pointers into them stay valid while the arena lives.
#[derive(Default)]
struct Arena {
    arrays: Vec<Box<[ForgeValue]>>,
    objects: Vec<Box<[ForgeEntry]>>,
}

fn borrowed_buf(bytes: &[u8]) -> ForgeBuf {
    ForgeBuf {
        // The plugin only reads through it (ABI rule 1).
        ptr: bytes.as_ptr() as *mut u8,
        len: bytes.len(),
    }
}

fn to_abi_array(items: &[Value], arena: &mut Arena, depth: usize) -> Result<ForgeValue, String> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(to_abi(item, arena, depth + 1)?);
    }
    let mut boxed = out.into_boxed_slice();
    let array = ForgeArray {
        ptr: boxed.as_mut_ptr(),
        len: boxed.len(),
    };
    arena.arrays.push(boxed);
    Ok(ForgeValue {
        tag: abi::TAG_ARRAY,
        as_: ForgePayload { array },
    })
}

fn to_abi_object<'a>(
    entries: impl Iterator<Item = (&'a str, &'a Value)>,
    arena: &mut Arena,
    depth: usize,
) -> Result<ForgeValue, String> {
    let mut out = Vec::new();
    for (key, value) in entries {
        out.push(ForgeEntry {
            key: borrowed_buf(key.as_bytes()),
            value: to_abi(value, arena, depth + 1)?,
        });
    }
    let mut boxed = out.into_boxed_slice();
    let object = ForgeObject {
        ptr: boxed.as_mut_ptr(),
        len: boxed.len(),
    };
    arena.objects.push(boxed);
    Ok(ForgeValue {
        tag: abi::TAG_OBJECT,
        as_: ForgePayload { object },
    })
}

/// Forge value → borrowed ABI value. Strings point into `v`; arrays and
/// objects into `arena`.
fn to_abi(v: &Value, arena: &mut Arena, depth: usize) -> Result<ForgeValue, String> {
    if depth > MAX_DEPTH {
        return Err(format!("nested deeper than {} levels", MAX_DEPTH));
    }
    let scalar = |tag, as_| Ok(ForgeValue { tag, as_ });
    match v {
        Value::Null | Value::None => Ok(ForgeValue::NULL),
        Value::Bool(b) => scalar(abi::TAG_BOOL, ForgePayload { i: *b as i64 }),
        Value::Int(n) => scalar(abi::TAG_INT, ForgePayload { i: *n }),
        Value::Float(x) => scalar(abi::TAG_FLOAT, ForgePayload { f: *x }),
        Value::String(s) => scalar(
            abi::TAG_STRING,
            ForgePayload {
                buf: borrowed_buf(s.as_bytes()),
            },
        ),
        Value::Array(items) | Value::Tuple(items) | Value::Set(items) => {
            to_abi_array(items, arena, depth)
        }
        Value::Object(map) => to_abi_object(map.iter().map(|(k, v)| (k.as_str(), v)), arena, depth),
        Value::Map(pairs) => {
            let mut entries = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                match k {
                    Value::String(k) => entries.push((k.as_str(), v)),
                    other => {
                        return Err(format!(
                            "a Map key must be a string to cross the native boundary, got {}",
                            other.type_name()
                        ))
                    }
                }
            }
            to_abi_object(entries.into_iter(), arena, depth)
        }
        Value::Some(inner) | Value::Frozen(inner) => to_abi(inner, arena, depth),
        other => Err(format!(
            "a {} cannot cross the native boundary",
            other.type_name()
        )),
    }
}

/// View an ABI (pointer, len) pair as a slice.
///
/// # Safety
/// When `len > 0`, `ptr` must point to `len` valid elements.
unsafe fn abi_slice<'a, T>(ptr: *const T, len: usize) -> Result<&'a [T], String> {
    if len == 0 {
        Ok(&[])
    } else if ptr.is_null() {
        Err(format!("NULL pointer with length {}", len))
    } else {
        Ok(std::slice::from_raw_parts(ptr, len))
    }
}

unsafe fn abi_string(buf: &ForgeBuf) -> Result<String, String> {
    let bytes = abi_slice(buf.ptr as *const u8, buf.len)?;
    std::str::from_utf8(bytes)
        .map(str::to_string)
        .map_err(|_| "a string is not valid UTF-8".to_string())
}

/// ABI value (owned by the plugin) → Forge value (copied).
///
/// # Safety
/// `v` must be a value produced by the plugin per the ABI.
unsafe fn from_abi(v: &ForgeValue, depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err(format!("nested deeper than {} levels", MAX_DEPTH));
    }
    Ok(match v.tag {
        abi::TAG_NULL => Value::Null,
        abi::TAG_BOOL => Value::Bool(v.as_.i != 0),
        abi::TAG_INT => Value::Int(v.as_.i),
        abi::TAG_FLOAT => Value::Float(v.as_.f),
        abi::TAG_STRING => Value::String(abi_string(&v.as_.buf)?),
        abi::TAG_BYTES => {
            let bytes = abi_slice(v.as_.buf.ptr as *const u8, v.as_.buf.len)?;
            Value::Array(bytes.iter().map(|&b| Value::Int(b as i64)).collect())
        }
        abi::TAG_ARRAY => {
            let items = abi_slice(v.as_.array.ptr as *const ForgeValue, v.as_.array.len)?;
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(from_abi(item, depth + 1)?);
            }
            Value::Array(out)
        }
        abi::TAG_OBJECT => {
            let entries = abi_slice(v.as_.object.ptr as *const ForgeEntry, v.as_.object.len)?;
            let mut out = IndexMap::with_capacity(entries.len());
            for entry in entries {
                out.insert(abi_string(&entry.key)?, from_abi(&entry.value, depth + 1)?);
            }
            Value::Object(out)
        }
        other => return Err(format!("unknown value tag {}", other)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_namespace_strips_platform_decoration() {
        assert_eq!(
            default_namespace("target/debug/libhello_rust").as_deref(),
            Some("hello_rust")
        );
        assert_eq!(default_namespace("libhello.so").as_deref(), Some("hello"));
        assert_eq!(
            default_namespace("plugins/math.dylib").as_deref(),
            Some("math")
        );
        assert_eq!(default_namespace("x/hello.dll").as_deref(), Some("hello"));
        assert_eq!(default_namespace("lib").as_deref(), Some("lib"));
        assert_eq!(default_namespace("dir/my-plugin"), None);
        assert_eq!(default_namespace("dir/2fast"), None);
    }

    #[test]
    fn candidates_add_platform_suffix_and_prefix() {
        let suffix = std::env::consts::DLL_SUFFIX;
        let c = candidate_paths("dir/hello");
        assert_eq!(c[0], Path::new("dir").join(format!("hello{}", suffix)));
        // Unix also tries the `lib` prefix; Windows (empty DLL_PREFIX) does not.
        let expected = if std::env::consts::DLL_PREFIX.is_empty() {
            1
        } else {
            2
        };
        assert_eq!(c.len(), expected);
        let c = candidate_paths("dir/libhello.so");
        assert_eq!(c, vec![PathBuf::from("dir/libhello.so")]);
    }

    #[test]
    fn missing_library_message_is_engine_independent() {
        let err = resolve_library("no/such/libnothing", Some(Path::new("/nonexistent-base")))
            .expect_err("missing");
        assert!(
            err.starts_with("native library 'no/such/libnothing' not found"),
            "{err}"
        );
        assert!(!err.contains("nonexistent-base"), "{err}");
    }

    #[test]
    fn values_round_trip_through_the_abi() {
        let mut map = IndexMap::new();
        map.insert("a".to_string(), Value::Int(1));
        map.insert(
            "b".to_string(),
            Value::Array(vec![Value::Float(1.5), Value::Bool(true), Value::Null]),
        );
        let v = Value::Object(map);
        let mut arena = Arena::default();
        let abi = to_abi(&v, &mut arena, 0).expect("to abi");
        let back = unsafe { from_abi(&abi, 0) }.expect("from abi");
        assert_eq!(back, v);
        let tuple = Value::Tuple(vec![Value::String("x".into())]);
        let abi = to_abi(&tuple, &mut arena, 0).expect("to abi");
        assert_eq!(
            unsafe { from_abi(&abi, 0) }.expect("from abi"),
            Value::Array(vec![Value::String("x".into())])
        );
    }

    #[test]
    fn unsupported_values_are_rejected_with_their_type() {
        let mut arena = Arena::default();
        let err = to_abi(&Value::ResultOk(Box::new(Value::Int(1))), &mut arena, 0)
            .err()
            .expect("rejected");
        assert_eq!(err, "a Result cannot cross the native boundary");
        let mut deep = Value::Null;
        for _ in 0..(MAX_DEPTH + 2) {
            deep = Value::Array(vec![deep]);
        }
        assert!(to_abi(&deep, &mut arena, 0).is_err());
    }

    #[test]
    fn malformed_plugin_values_are_errors_not_ub() {
        let v = ForgeValue {
            tag: abi::TAG_STRING,
            as_: ForgePayload {
                buf: ForgeBuf {
                    ptr: std::ptr::null_mut(),
                    len: 3,
                },
            },
        };
        assert!(unsafe { from_abi(&v, 0) }.is_err());
        let v = ForgeValue {
            tag: 99,
            as_: ForgePayload { i: 0 },
        };
        assert_eq!(
            unsafe { from_abi(&v, 0) }.err().as_deref(),
            Some("unknown value tag 99")
        );
        let empty = ForgeValue {
            tag: abi::TAG_ARRAY,
            as_: ForgePayload {
                array: ForgeArray {
                    ptr: std::ptr::null_mut(),
                    len: 0,
                },
            },
        };
        assert_eq!(
            unsafe { from_abi(&empty, 0) }.expect("empty"),
            Value::Array(vec![])
        );
    }

    #[test]
    fn unknown_plugin_function_names_are_errors() {
        assert!(call("native:999999:nope", vec![]).is_err());
        assert!(call("native:garbage", vec![]).is_err());
    }
}
