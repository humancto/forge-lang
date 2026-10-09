//! Host-facing helpers for running untrusted code on the VM (`crate::sandbox`,
//! `forge mcp`): compile a program, call a top-level function with host
//! values, and turn the result back into a host (interpreter) value.
//!
//! Unlike the engine-internal conversions (`VM::convert_interp_value`,
//! `VM::convert_to_interp_val`), which serve builtins and map anything they
//! do not know to `null`, these map `Option` both ways (`Value::Some` /
//! `Value::None` ⇄ the VM's `Option`-tagged objects) and fail loudly on a
//! value that has no host form, because a tool's arguments and results are
//! a contract with the caller.

use indexmap::IndexMap;

use super::bytecode::Chunk;
use super::compiler::{self, CompileError, CompileOptions};
use super::machine::{VMError, VM};
use super::value::{GcRef, ObjKind, Value, ValueKind};
use crate::interpreter::Value as HostValue;
use crate::parser::ast::Program;

/// Why a program cannot run on the VM.
pub(crate) enum CompileFailure {
    /// The VM cannot run it faithfully (an unknown decorator, a construct
    /// the compiler reports as `Unsupported`): run it on the interpreter.
    Unsupported,
    /// A real error (an import that cannot be resolved or read, a branch
    /// too long to encode): report it as the run's error.
    Error(String),
}

/// Compile `program` for the VM, or say why it should not run there.
/// Imports resolve against `base_dir` and are read (to learn their exported
/// names) under the active permission policy, so call this on the thread
/// that will run the program.
pub(crate) fn compile_program(
    program: &Program,
    base_dir: Option<std::path::PathBuf>,
) -> Result<Chunk, CompileFailure> {
    let issues = crate::runtime::metadata::vm_incompatibilities(program);
    if !issues.is_empty() {
        return Err(CompileFailure::Unsupported);
    }
    compiler::compile_with(program, &CompileOptions { base_dir }).map_err(failure)
}

fn failure(e: CompileError) -> CompileFailure {
    if e.is_unsupported() {
        CompileFailure::Unsupported
    } else {
        CompileFailure::Error(e.message)
    }
}

/// Source line of a VM error: the innermost frame that has one (the line
/// the interpreter would report for the same failure), 0 when unknown.
pub(crate) fn error_line(e: &VMError) -> usize {
    e.stack_trace
        .iter()
        .map(|frame| frame.line)
        .find(|line| *line > 0)
        .unwrap_or(0)
}

/// Call the global function `name` with host `args`; the result as a host
/// value. Fails if `name` is not bound, the call fails, or the result holds
/// something that is not data on the host side (a stream, a value nested
/// too deeply).
pub(crate) fn call_global(
    vm: &mut VM,
    name: &str,
    args: &[HostValue],
) -> Result<HostValue, VMError> {
    let func = vm
        .globals
        .get(name)
        .copied()
        .ok_or_else(|| VMError::new(&format!("BUG: function `{}` is missing", name)))?;
    // No collection can run while converting (the GC runs only at safe
    // points of `run_until`); `call_value` roots the arguments in the
    // callee's registers.
    let vm_args = args
        .iter()
        .map(|arg| to_vm(vm, arg))
        .collect::<Result<Vec<_>, _>>()?;
    let result = vm.call_value(func, vm_args)?;
    to_host(vm, result).map_err(|e| VMError::new(&e))
}

/// The host value `v` as a VM value.
pub(crate) fn to_vm(vm: &mut VM, v: &HostValue) -> Result<Value, VMError> {
    let Some(_level) = crate::runtime::recursion::enter_value_level() else {
        return Err(VMError::new(
            &crate::runtime::recursion::value_too_deep_message(),
        ));
    };
    let alloc = |vm: &mut VM, kind: ObjKind| Value::obj(vm.gc.alloc(kind));
    Ok(match v {
        HostValue::Int(n) => Value::int(*n, &mut vm.gc),
        HostValue::Float(f) => Value::float(*f),
        HostValue::Bool(b) => Value::bool_val(*b),
        HostValue::Null => Value::null(),
        HostValue::String(s) => Value::obj(vm.gc.alloc_str(s)),
        HostValue::Array(items) => {
            let items = to_vm_all(vm, items)?;
            alloc(vm, ObjKind::Array(items))
        }
        HostValue::Tuple(items) => {
            let items = to_vm_all(vm, items)?;
            alloc(vm, ObjKind::Tuple(items))
        }
        HostValue::Set(items) => {
            let items = to_vm_all(vm, items)?;
            alloc(vm, ObjKind::Set(items))
        }
        HostValue::Object(map) => {
            let mut out = IndexMap::with_capacity(map.len());
            for (k, x) in map {
                out.insert(k.clone(), to_vm(vm, x)?);
            }
            alloc(vm, ObjKind::Object(out))
        }
        HostValue::Map(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for (k, x) in pairs {
                out.push((to_vm(vm, k)?, to_vm(vm, x)?));
            }
            alloc(vm, ObjKind::Map(out))
        }
        HostValue::Some(inner) => {
            let inner = to_vm(vm, inner)?;
            option(vm, "Some", Some(inner))
        }
        HostValue::None => option(vm, "None", None),
        HostValue::ResultOk(inner) => {
            let inner = to_vm(vm, inner)?;
            alloc(vm, ObjKind::ResultOk(inner))
        }
        HostValue::ResultErr(inner) => {
            let inner = to_vm(vm, inner)?;
            alloc(vm, ObjKind::ResultErr(inner))
        }
        other => {
            return Err(VMError::new(&format!(
                "a {} cannot be passed to the VM",
                other.type_name()
            )))
        }
    })
}

fn to_vm_all(vm: &mut VM, items: &[HostValue]) -> Result<Vec<Value>, VMError> {
    items.iter().map(|x| to_vm(vm, x)).collect()
}

/// `Some(inner)` / `None` in the VM's representation (an `Option`-tagged
/// object, as the `Some` builtin and the `None` prelude build it).
fn option(vm: &mut VM, variant: &str, inner: Option<Value>) -> Value {
    if inner.is_none() {
        if let Some(none) = vm.globals.get("None").copied() {
            return none;
        }
    }
    let mut obj = IndexMap::new();
    obj.insert(
        "__type__".to_string(),
        Value::obj(vm.gc.alloc_str("Option")),
    );
    obj.insert(
        "__variant__".to_string(),
        Value::obj(vm.gc.alloc_str(variant)),
    );
    if let Some(inner) = inner {
        obj.insert("_0".to_string(), inner);
    }
    Value::obj(vm.gc.alloc(ObjKind::Object(obj)))
}

/// The VM value `v` (a function's result) as a host value. Values that are
/// not data (functions, channels, task handles, streams) are errors.
pub(crate) fn to_host(vm: &VM, v: Value) -> Result<HostValue, String> {
    let Some(_level) = crate::runtime::recursion::enter_value_level() else {
        return Err(crate::runtime::recursion::value_too_deep_message());
    };
    let r = match v.classify(&vm.gc) {
        ValueKind::Int(n) => return Ok(HostValue::Int(n)),
        ValueKind::Float(f) => return Ok(HostValue::Float(f)),
        ValueKind::Bool(b) => return Ok(HostValue::Bool(b)),
        ValueKind::Null => return Ok(HostValue::Null),
        ValueKind::Obj(r) => r,
    };
    let Some(obj) = vm.gc.get(r) else {
        return Ok(HostValue::Null);
    };
    let all = |items: &[Value]| -> Result<Vec<HostValue>, String> {
        items.iter().map(|x| to_host(vm, *x)).collect()
    };
    Ok(match &obj.kind {
        ObjKind::String(s) => HostValue::String(s.clone()),
        ObjKind::BoxedInt(n) => HostValue::Int(*n),
        ObjKind::Array(items) => HostValue::Array(all(items)?),
        ObjKind::Tuple(items) => HostValue::Tuple(all(items)?),
        ObjKind::Set(items) => HostValue::Set(all(items)?),
        ObjKind::Object(map) => match option_variant(vm, map) {
            Some(true) => HostValue::Some(Box::new(match map.get("_0") {
                Some(inner) => to_host(vm, *inner)?,
                None => HostValue::Null,
            })),
            Some(false) => HostValue::None,
            None => {
                let mut out = IndexMap::with_capacity(map.len());
                for (k, x) in map {
                    out.insert(k.clone(), to_host(vm, *x)?);
                }
                HostValue::Object(out)
            }
        },
        ObjKind::Map(pairs) => {
            let mut out = Vec::with_capacity(pairs.len());
            for (k, x) in pairs {
                out.push((to_host(vm, *k)?, to_host(vm, *x)?));
            }
            HostValue::Map(out)
        }
        ObjKind::ResultOk(inner) => HostValue::ResultOk(Box::new(to_host(vm, *inner)?)),
        ObjKind::ResultErr(inner) => HostValue::ResultErr(Box::new(to_host(vm, *inner)?)),
        ObjKind::Frozen(inner) => to_host(vm, *inner)?,
        ObjKind::Stream(_) => return Err(
            "Stream cannot cross the VM/interpreter boundary; call .collect() first to materialize"
                .to_string(),
        ),
        // The interpreter's type names, so the message matches a tool
        // result checked on either engine.
        other => {
            let name = match other {
                ObjKind::Function(f) if f.name == "<lambda>" => "Lambda",
                ObjKind::Closure(c) if c.function.name == "<lambda>" => "Lambda",
                ObjKind::Function(_) | ObjKind::Closure(_) => "Function",
                ObjKind::NativeFunction(_) => "BuiltIn",
                ObjKind::TaskHandle(_) => "TaskHandle",
                ObjKind::Channel(_) => "Channel",
                _ => "Null",
            };
            return Err(format!("the result contains a {}, which is not data", name));
        }
    })
}

/// `Some(true)` for `Some(..)`, `Some(false)` for `None`, `None` for any
/// other object.
fn option_variant(vm: &VM, map: &IndexMap<String, Value>) -> Option<bool> {
    let text = |key: &str| -> Option<&str> {
        let r: GcRef = map.get(key)?.as_obj()?;
        match &vm.gc.get(r)?.kind {
            ObjKind::String(s) => Some(s.as_str()),
            _ => None,
        }
    };
    if text("__type__")? != "Option" {
        return None;
    }
    match text("__variant__")? {
        "Some" => Some(true),
        "None" => Some(false),
        _ => None,
    }
}
