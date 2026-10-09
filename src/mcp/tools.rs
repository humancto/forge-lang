//! MCP tools and resources written in Forge (`forge mcp serve tools.fg`).
//!
//! A tool is a top-level function with a `@tool` decorator; its typed
//! parameters become the tool's JSON Schema:
//!
//! ```forge
//! @tool(description: "Convert a temperature between Celsius and Fahrenheit")
//! @param(value: "The temperature to convert", to: "Target unit: \"C\" or \"F\"")
//! fn convert_temperature(value: Float, to: String = "C") -> Float { ... }
//! ```
//!
//! | Forge type | JSON Schema |
//! | --- | --- |
//! | `Int` | `integer` |
//! | `Float` | `number` (integers are accepted and become floats) |
//! | `String`, `Bool` | `string`, `boolean` |
//! | `[T]`, `Array<T>` | `array` of `T` |
//! | `Object`, `Map<String, T>` | `object` (of `T`) |
//! | a `struct` | `object` with the struct's fields |
//! | `?T` | `T` or `null` (passed as `null`) |
//! | `Option<T>` | `T` or `null` (passed as `Some(v)` / `None`) |
//! | `Any` / no annotation | any JSON value |
//!
//! A parameter with a default, `?T` or `Option<T>` is optional. A return
//! type (other than `Any`) becomes the tool's `outputSchema`, and results
//! are checked against it. Returning `Err(message)` reports a tool error.
//!
//! `@resource(uri: "...", description: "...")` on a function without
//! parameters exposes its result as a read-only MCP resource.
//!
//! # Execution model
//!
//! The file's top level runs once at start-up, inside the same sandbox the
//! tools get (policy, time limit, captured output, no host runtime), on the
//! server's engine. The result is a read-only template; every call runs in
//! a fresh fork of it — the HTTP server's per-request fork:
//! [`VmTemplate::fork`] on the VM (the default),
//! [`Interpreter::fork_for_serving`] on the interpreter — so calls never
//! see each other's state. A file the VM cannot run faithfully (an unknown
//! decorator) is served by the interpreter.
//! Arguments are validated against the schema in Rust before any Forge code
//! runs.

use super::{tool_error, truncate_utf8, CAPTURE_LIMIT};
use crate::interpreter::{Interpreter, RuntimeError, Value};
use crate::parser::ast::{
    Decorator, DecoratorArg, Expr, FieldDef, Param, Program, Stmt, TypeAnn, UnaryOp,
};
use crate::permissions::{Capabilities, Capability};
use crate::runtime::limits::Limits;
use crate::sandbox::{
    parse_source, CancelHandle, Engine, JobError, RunScope, Sandbox, SandboxError,
};
use crate::vm::embed;
use crate::vm::serve::VmTemplate;
use indexmap::IndexMap;
use serde_json::{json, Map, Value as Json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Deepest result value a tool may return (JSON nesting levels).
const MAX_RESULT_DEPTH: usize = 128;

/// Tools and resources loaded from a Forge file, ready to serve.
pub struct ToolSet {
    /// The program after its top level ran. Never run directly: every call
    /// forks it (on the call's worker, under the call's budget).
    template: Template,
    tools: Vec<ToolDef>,
    resources: Vec<ResourceDef>,
    /// Policy every tool call runs under.
    caps: Capabilities,
    /// Resource limits for the top level and for each call (a fresh budget
    /// every time).
    limits: Limits,
    /// File the tools came from (error messages, imports).
    path: String,
}

impl std::fmt::Debug for ToolSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolSet")
            .field("path", &self.path)
            .field(
                "tools",
                &self.tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
            )
            .field(
                "resources",
                &self.resources.iter().map(|r| &r.uri).collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// One `@tool` function.
#[derive(Debug, Clone)]
struct ToolDef {
    /// MCP tool name (the function name unless `@tool(name: ...)`).
    name: String,
    /// The Forge function to call.
    function: String,
    title: Option<String>,
    description: String,
    params: Vec<ParamDef>,
    /// Declared return type, if it constrains anything.
    returns: Option<Ty>,
    /// Per-tool time limit (`@tool(timeout: secs)`), capped by the server's.
    timeout: Option<Duration>,
    /// Explicit annotation hints from `@tool(read_only: ..., ...)`.
    hints: Vec<(&'static str, bool)>,
}

/// One `@resource` function.
#[derive(Debug, Clone)]
struct ResourceDef {
    uri: String,
    name: String,
    title: Option<String>,
    description: Option<String>,
    mime_type: Option<String>,
    function: String,
}

#[derive(Debug, Clone)]
struct ParamDef {
    name: String,
    ty: Ty,
    description: Option<String>,
    default: ParamDefault,
}

#[derive(Debug, Clone)]
enum ParamDefault {
    /// No default: required unless the type is optional.
    None,
    /// A constant default, shown in the schema and passed when omitted.
    Const(Json),
    /// A default the interpreter must evaluate; only allowed on the last
    /// parameter, so omitting it is just a shorter argument list.
    Expr,
}

/// A parameter or result type, resolved from its annotation.
#[derive(Debug, Clone)]
enum Ty {
    Any,
    Int,
    Float,
    Str,
    Bool,
    Null,
    Array(Box<Ty>),
    /// A JSON object with values of one type (`Object` = `Map<String, Any>`).
    Map(Box<Ty>),
    Struct(Arc<StructTy>),
    /// `?T`: `null` or `T`, passed through as is.
    Nullable(Box<Ty>),
    /// `Option<T>`: `null` → `None`, value → `Some(value)`.
    Option(Box<Ty>),
}

#[derive(Debug)]
struct StructTy {
    name: String,
    fields: Vec<FieldTy>,
}

#[derive(Debug)]
struct FieldTy {
    name: String,
    ty: Ty,
    default: Option<Json>,
}

impl Ty {
    fn is_optional(&self) -> bool {
        matches!(self, Ty::Nullable(_) | Ty::Option(_) | Ty::Null)
    }

    /// Whether results of this type are JSON objects, so they can be the
    /// structured content themselves instead of `{"result": ...}`.
    fn is_object(&self) -> bool {
        matches!(self, Ty::Map(_) | Ty::Struct(_))
    }

    fn schema(&self) -> Json {
        match self {
            Ty::Any => json!({}),
            Ty::Int => json!({ "type": "integer" }),
            Ty::Float => json!({ "type": "number" }),
            Ty::Str => json!({ "type": "string" }),
            Ty::Bool => json!({ "type": "boolean" }),
            Ty::Null => json!({ "type": "null" }),
            Ty::Array(item) => json!({ "type": "array", "items": item.schema() }),
            Ty::Map(value) => match value.as_ref() {
                Ty::Any => json!({ "type": "object" }),
                value => json!({ "type": "object", "additionalProperties": value.schema() }),
            },
            Ty::Struct(st) => {
                let mut properties = Map::new();
                let mut required = Vec::new();
                for field in &st.fields {
                    let mut schema = field.ty.schema();
                    if let Some(default) = &field.default {
                        schema["default"] = default.clone();
                    } else if !field.ty.is_optional() {
                        required.push(json!(field.name));
                    }
                    properties.insert(field.name.clone(), schema);
                }
                json!({
                    "type": "object",
                    "title": st.name,
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false
                })
            }
            Ty::Nullable(inner) | Ty::Option(inner) => {
                let mut schema = inner.schema();
                match schema.get("type").cloned() {
                    Some(Json::String(t)) => {
                        schema["type"] = json!([t, "null"]);
                        schema
                    }
                    _ if matches!(inner.as_ref(), Ty::Any) => schema,
                    _ => json!({ "anyOf": [schema, { "type": "null" }] }),
                }
            }
        }
    }

    fn describe(&self) -> String {
        match self {
            Ty::Any => "any value".into(),
            Ty::Int => "an integer".into(),
            Ty::Float => "a number".into(),
            Ty::Str => "a string".into(),
            Ty::Bool => "a boolean".into(),
            Ty::Null => "null".into(),
            Ty::Array(_) => "an array".into(),
            Ty::Map(_) => "an object".into(),
            Ty::Struct(st) => format!("an object ({})", st.name),
            Ty::Nullable(inner) | Ty::Option(inner) => format!("{} or null", inner.describe()),
        }
    }
}

fn json_kind(v: &Json) -> &'static str {
    match v {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(n) if n.is_i64() || n.is_u64() => "an integer",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "an array",
        Json::Object(_) => "an object",
    }
}

fn mismatch(path: &str, ty: &Ty, got: &Json) -> String {
    format!(
        "`{}`: expected {}, got {}",
        path,
        ty.describe(),
        json_kind(got)
    )
}

/// An integer, accepting floats with no fractional part (`3.0`) as JSON
/// Schema's `integer` does.
fn json_int(v: &Json) -> Option<i64> {
    if let Some(i) = v.as_i64() {
        return Some(i);
    }
    let f = v.as_f64()?;
    // 2^63 is exact in f64; everything below it in magnitude fits in i64.
    (f.fract() == 0.0 && f.abs() < 9_223_372_036_854_775_808.0).then_some(f as i64)
}

/// Convert an argument to the Forge value the function receives, checking
/// it against `ty`.
fn convert(ty: &Ty, v: &Json, path: &str) -> Result<Value, String> {
    Ok(match ty {
        Ty::Any => crate::runtime::server::json_to_forge(v.clone()),
        Ty::Int => Value::Int(json_int(v).ok_or_else(|| mismatch(path, ty, v))?),
        Ty::Float => Value::Float(v.as_f64().ok_or_else(|| mismatch(path, ty, v))?),
        Ty::Str => Value::String(v.as_str().ok_or_else(|| mismatch(path, ty, v))?.to_string()),
        Ty::Bool => Value::Bool(v.as_bool().ok_or_else(|| mismatch(path, ty, v))?),
        Ty::Null if v.is_null() => Value::Null,
        Ty::Null => return Err(mismatch(path, ty, v)),
        Ty::Array(item) => {
            let items = v.as_array().ok_or_else(|| mismatch(path, ty, v))?;
            Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, x)| convert(item, x, &format!("{}[{}]", path, i)))
                    .collect::<Result<_, _>>()?,
            )
        }
        Ty::Map(value) => {
            let obj = v.as_object().ok_or_else(|| mismatch(path, ty, v))?;
            Value::Object(
                obj.iter()
                    .map(|(k, x)| Ok((k.clone(), convert(value, x, &format!("{}.{}", path, k))?)))
                    .collect::<Result<IndexMap<_, _>, String>>()?,
            )
        }
        Ty::Struct(st) => {
            let obj = v.as_object().ok_or_else(|| mismatch(path, ty, v))?;
            if let Some(extra) = obj
                .keys()
                .find(|k| !st.fields.iter().any(|f| &f.name == *k))
            {
                return Err(format!(
                    "`{}`: unknown field `{}` for {}",
                    path, extra, st.name
                ));
            }
            let mut out = IndexMap::new();
            for field in &st.fields {
                let field_path = format!("{}.{}", path, field.name);
                let value = match (obj.get(&field.name), &field.default) {
                    (Some(x), _) => convert(&field.ty, x, &field_path)?,
                    (None, Some(default)) => convert(&field.ty, default, &field_path)?,
                    (None, None) if field.ty.is_optional() => {
                        convert(&field.ty, &Json::Null, &field_path)?
                    }
                    (None, None) => {
                        return Err(format!("`{}`: missing required field", field_path))
                    }
                };
                out.insert(field.name.clone(), value);
            }
            out.insert("__type__".to_string(), Value::String(st.name.clone()));
            Value::Object(out)
        }
        Ty::Nullable(_) if v.is_null() => Value::Null,
        Ty::Nullable(inner) => convert(inner, v, path)?,
        Ty::Option(_) if v.is_null() => Value::None,
        Ty::Option(inner) => Value::Some(Box::new(convert(inner, v, path)?)),
    })
}

/// Check a result (already JSON) against the declared return type.
fn check(ty: &Ty, v: &Json, path: &str) -> Result<(), String> {
    let ok = match ty {
        Ty::Any => true,
        Ty::Int => v.is_i64() || v.is_u64(),
        Ty::Float => v.is_number(),
        Ty::Str => v.is_string(),
        Ty::Bool => v.is_boolean(),
        Ty::Null => v.is_null(),
        Ty::Array(item) => {
            let items = v.as_array().ok_or_else(|| mismatch(path, ty, v))?;
            for (i, x) in items.iter().enumerate() {
                check(item, x, &format!("{}[{}]", path, i))?;
            }
            true
        }
        Ty::Map(value) => {
            let obj = v.as_object().ok_or_else(|| mismatch(path, ty, v))?;
            for (k, x) in obj {
                check(value, x, &format!("{}.{}", path, k))?;
            }
            true
        }
        Ty::Struct(st) => {
            let obj = v.as_object().ok_or_else(|| mismatch(path, ty, v))?;
            if let Some(extra) = obj
                .keys()
                .find(|k| !st.fields.iter().any(|f| &f.name == *k))
            {
                return Err(format!(
                    "`{}`: unknown field `{}` for {}",
                    path, extra, st.name
                ));
            }
            for field in &st.fields {
                let field_path = format!("{}.{}", path, field.name);
                match obj.get(&field.name) {
                    Some(x) => check(&field.ty, x, &field_path)?,
                    None if field.default.is_some() || field.ty.is_optional() => {}
                    None => return Err(format!("`{}`: missing required field", field_path)),
                }
            }
            true
        }
        Ty::Nullable(inner) | Ty::Option(inner) => {
            v.is_null() || {
                check(inner, v, path)?;
                true
            }
        }
    };
    if ok {
        Ok(())
    } else {
        Err(mismatch(path, ty, v))
    }
}

/// A tool's return value as JSON: `Some(v)` → `v`, `None` → `null`, struct
/// instances lose their `__type__` tag, maps with non-string keys become
/// `[key, value]` pairs, non-data values (functions, channels) are errors.
fn value_to_json(v: &Value, depth: usize) -> Result<Json, String> {
    if depth > MAX_RESULT_DEPTH {
        return Err(format!(
            "the result is nested more than {} levels deep",
            MAX_RESULT_DEPTH
        ));
    }
    let list = |items: &[Value]| -> Result<Json, String> {
        items
            .iter()
            .map(|x| value_to_json(x, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Json::Array)
    };
    Ok(match v {
        Value::Null | Value::None => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(n) => json!(n),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(Json::Number)
            .ok_or_else(|| format!("the result contains {}, which JSON cannot represent", f))?,
        Value::String(s) => Json::String(s.clone()),
        Value::Array(items) | Value::Tuple(items) | Value::Set(items) => list(items)?,
        Value::Object(map) => {
            let is_struct = matches!(map.get("__type__"), Some(Value::String(_)));
            let mut obj = Map::new();
            for (k, x) in map {
                if is_struct && k == "__type__" {
                    continue;
                }
                obj.insert(k.clone(), value_to_json(x, depth + 1)?);
            }
            Json::Object(obj)
        }
        Value::Map(pairs) => {
            if pairs.iter().all(|(k, _)| matches!(k, Value::String(_))) {
                let mut obj = Map::new();
                for (k, x) in pairs {
                    if let Value::String(k) = k {
                        obj.insert(k.clone(), value_to_json(x, depth + 1)?);
                    }
                }
                Json::Object(obj)
            } else {
                let mut out = Vec::with_capacity(pairs.len());
                for (k, x) in pairs {
                    out.push(Json::Array(vec![
                        value_to_json(k, depth + 1)?,
                        value_to_json(x, depth + 1)?,
                    ]));
                }
                Json::Array(out)
            }
        }
        Value::Some(inner) | Value::Frozen(inner) | Value::ResultOk(inner) => {
            value_to_json(inner, depth + 1)?
        }
        Value::ResultErr(inner) => json!({ "error": value_to_json(inner, depth + 1)? }),
        other => {
            return Err(format!(
                "the result contains a {}, which is not data",
                other.type_name()
            ))
        }
    })
}

/// A loaded tool file, ready to fork per call.
enum Template {
    /// The interpreter after the top level ran ([`Interpreter::fork_for_serving`]).
    Interpreter(Arc<Interpreter>),
    /// The VM after the top level ran, frozen ([`VmTemplate::fork`]).
    Vm(Arc<VmTemplate>),
}

/// Prefix of the load error for a top-level stream (reported without the
/// "top level failed" wrapper).
const STREAM_PREFIX: &str = "stream in template: ";

/// Run the tool file's top level on `engine` (the interpreter when the VM
/// cannot run it faithfully) and freeze the result into a [`Template`].
/// Also returns the first of `functions` that is not a function afterwards.
fn build_template(
    scope: &RunScope,
    engine: Engine,
    program: &Program,
    source: String,
    file: std::path::PathBuf,
    functions: &[String],
) -> Result<(Template, Option<String>), JobError> {
    if engine == Engine::Vm {
        match embed::compile_program(program, file.parent().map(Path::to_path_buf)) {
            Ok(chunk) => {
                let mut vm = crate::vm::machine::VM::new();
                scope.contain_vm(&mut vm);
                vm.execute(&chunk)?;
                let template = VmTemplate::new(&vm, HashMap::new()).map_err(|_| {
                    JobError::new(format!(
                        "{}a top-level value is a stream; streams are single-use and cannot \
                         be shared by tool calls (create it inside the tool)",
                        STREAM_PREFIX
                    ))
                })?;
                let missing = functions
                    .iter()
                    .find(|f| !template.has_function(f))
                    .cloned();
                return Ok((Template::Vm(Arc::new(template)), missing));
            }
            Err(embed::CompileFailure::Error(message)) => return Err(JobError::new(message)),
            Err(embed::CompileFailure::Unsupported) => {}
        }
    }
    let mut interp = Interpreter::new();
    interp.source = Some(source);
    interp.source_file = Some(file);
    scope.contain_interpreter(&mut interp);
    interp.run(program)?;
    if let Some(path) = Interpreter::find_stream_in_env(&interp.env) {
        return Err(JobError::new(format!(
            "{}top-level value `{}` is a stream; streams are single-use and cannot be shared \
             by tool calls (create it inside the tool)",
            STREAM_PREFIX, path
        )));
    }
    let missing = functions
        .iter()
        .find(|f| !matches!(interp.env.get(f), Some(Value::Function(_))))
        .cloned();
    Ok((Template::Interpreter(Arc::new(interp)), missing))
}

/// A tool function's return value as JSON: `Err(v)` is a tool error.
fn produce(value: &Value) -> Result<Produced, String> {
    match value {
        Value::ResultErr(inner) => value_to_json(inner, 0).map(Produced::Err),
        other => value_to_json(other, 0).map(Produced::Ok),
    }
}

/// What a tool or resource function produced, converted on the worker.
enum Produced {
    Ok(Json),
    /// The function returned `Err(value)`.
    Err(Json),
}

/// Literal constant value of a default or decorator argument.
fn const_json(expr: &Expr) -> Option<Json> {
    Some(match expr {
        Expr::Int(n) => json!(n),
        Expr::Float(f) => json!(f),
        Expr::StringLit(s) => json!(s),
        Expr::Bool(b) => json!(b),
        Expr::Ident(name) if name == "null" || name == "None" => Json::Null,
        Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand,
        } => match operand.as_ref() {
            Expr::Int(n) => json!(n.checked_neg()?),
            Expr::Float(f) => json!(-f),
            _ => return None,
        },
        Expr::Array(items) => Json::Array(items.iter().map(const_json).collect::<Option<_>>()?),
        Expr::Object(fields) => Json::Object(
            fields
                .iter()
                .map(|(k, x)| Some((k.clone(), const_json(x)?)))
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}

/// A load-time problem, reported as `file:line: message`.
struct LoadError {
    line: usize,
    message: String,
}

fn err<T>(line: usize, message: impl Into<String>) -> Result<T, LoadError> {
    Err(LoadError {
        line,
        message: message.into(),
    })
}

/// Resolve type annotations against the file's structs.
struct TypeResolver<'a> {
    structs: HashMap<&'a str, &'a [FieldDef]>,
    resolved: HashMap<String, Arc<StructTy>>,
    /// Structs being resolved (recursion check).
    stack: Vec<String>,
}

impl<'a> TypeResolver<'a> {
    fn resolve(&mut self, ann: &TypeAnn) -> Result<Ty, String> {
        let unsupported = |what: &str| {
            Err(format!(
                "type `{}` cannot be described as JSON; use Int, Float, String, Bool, Any, \
                 [T], Object, Map<String, T>, ?T, Option<T> or a struct",
                what
            ))
        };
        Ok(match ann {
            TypeAnn::Simple(name) => match name.as_str() {
                "Int" | "int" => Ty::Int,
                "Float" | "float" | "Number" => Ty::Float,
                "String" | "string" | "str" => Ty::Str,
                "Bool" | "bool" => Ty::Bool,
                "Any" | "Json" => Ty::Any,
                "Null" => Ty::Null,
                "Object" | "Map" => Ty::Map(Box::new(Ty::Any)),
                "Array" | "List" => Ty::Array(Box::new(Ty::Any)),
                other => match self.structs.get(other) {
                    Some(fields) => Ty::Struct(self.resolve_struct(other, fields)?),
                    None => return unsupported(other),
                },
            },
            TypeAnn::Array(item) => Ty::Array(Box::new(self.resolve(item)?)),
            TypeAnn::Optional(inner) => Ty::Nullable(Box::new(self.resolve(inner)?)),
            TypeAnn::Generic(name, args) => match (name.as_str(), args.as_slice()) {
                ("Array" | "List", [item]) => Ty::Array(Box::new(self.resolve(item)?)),
                ("Option", [inner]) => Ty::Option(Box::new(self.resolve(inner)?)),
                ("Map" | "Object", [value]) => Ty::Map(Box::new(self.resolve(value)?)),
                ("Map" | "Object", [TypeAnn::Simple(key), value])
                    if matches!(key.as_str(), "String" | "string" | "str") =>
                {
                    Ty::Map(Box::new(self.resolve(value)?))
                }
                _ => return unsupported(&format!("{}<...>", name)),
            },
            TypeAnn::Tuple(_) => return unsupported("tuple"),
            TypeAnn::Function(..) => return unsupported("function"),
        })
    }

    fn resolve_struct(&mut self, name: &str, fields: &[FieldDef]) -> Result<Arc<StructTy>, String> {
        if let Some(done) = self.resolved.get(name) {
            return Ok(done.clone());
        }
        if self.stack.iter().any(|s| s == name) {
            return Err(format!(
                "struct `{}` contains itself; recursive types cannot be tool parameters",
                name
            ));
        }
        self.stack.push(name.to_string());
        let mut out = Vec::with_capacity(fields.len());
        for field in fields {
            let ty = self
                .resolve(&field.type_ann)
                .map_err(|e| format!("field `{}.{}`: {}", name, field.name, e))?;
            let default = match &field.default {
                None => None,
                Some(expr) => Some(const_json(expr).ok_or_else(|| {
                    format!(
                        "field `{}.{}`: a tool parameter's struct defaults must be constants",
                        name, field.name
                    )
                })?),
            };
            out.push(FieldTy {
                name: field.name.clone(),
                ty,
                default,
            });
        }
        self.stack.pop();
        let st = Arc::new(StructTy {
            name: name.to_string(),
            fields: out,
        });
        self.resolved.insert(name.to_string(), st.clone());
        Ok(st)
    }

    /// The return type, if it constrains the result (`Result<T, E>` → `T`).
    fn resolve_return(&mut self, ann: &TypeAnn) -> Result<Option<Ty>, String> {
        let ann = match ann {
            TypeAnn::Generic(name, args) if name == "Result" && !args.is_empty() => &args[0],
            other => other,
        };
        Ok(match self.resolve(ann)? {
            Ty::Any => None,
            ty => Some(ty),
        })
    }
}

/// Decorator arguments as `name → constant`. A single positional argument
/// is accepted as `positional_key` (`@tool("Adds two numbers")`).
fn decorator_args(
    dec: &Decorator,
    positional_key: Option<&str>,
    line: usize,
) -> Result<Vec<(String, Json)>, LoadError> {
    let mut out = Vec::new();
    for (i, arg) in dec.args.iter().enumerate() {
        let (key, expr) = match (arg, positional_key) {
            (DecoratorArg::Named(k, e), _) => (k.clone(), e),
            (DecoratorArg::Positional(e), Some(k)) if i == 0 => (k.to_string(), e),
            (DecoratorArg::Positional(_), _) => {
                return err(
                    line,
                    format!("@{}: arguments must be named (key: value)", dec.name),
                )
            }
        };
        let Some(value) = const_json(expr) else {
            return err(
                line,
                format!(
                    "@{}: `{}` must be a constant (string, number, bool)",
                    dec.name, key
                ),
            );
        };
        if out.iter().any(|(k, _)| k == &key) {
            return err(line, format!("@{}: `{}` given twice", dec.name, key));
        }
        out.push((key, value));
    }
    Ok(out)
}

fn want_str(dec: &str, key: &str, v: &Json, line: usize) -> Result<String, LoadError> {
    match v.as_str() {
        Some(s) => Ok(s.to_string()),
        None => err(line, format!("@{}: `{}` must be a string", dec, key)),
    }
}

/// MCP tool names: 1-128 of `[A-Za-z0-9_.-]`.
fn valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

const TOOL_KEYS: &[&str] = &[
    "description",
    "name",
    "title",
    "timeout",
    "read_only",
    "destructive",
    "idempotent",
    "open_world",
];
const RESOURCE_KEYS: &[&str] = &["uri", "name", "title", "description", "mime_type"];

fn check_keys(
    dec: &str,
    args: &[(String, Json)],
    allowed: &[&str],
    line: usize,
) -> Result<(), LoadError> {
    match args.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
        Some((k, _)) => err(
            line,
            format!(
                "@{}: unknown argument `{}` (expected one of: {})",
                dec,
                k,
                allowed.join(", ")
            ),
        ),
        None => Ok(()),
    }
}

fn build_tool(
    types: &mut TypeResolver,
    fn_name: &str,
    params: &[Param],
    return_type: Option<&TypeAnn>,
    tool_args: Vec<(String, Json)>,
    param_docs: Vec<(String, Json)>,
    line: usize,
) -> Result<ToolDef, LoadError> {
    check_keys("tool", &tool_args, TOOL_KEYS, line)?;
    let get = |key: &str| tool_args.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let name = match get("name") {
        Some(v) => want_str("tool", "name", v, line)?,
        None => fn_name.to_string(),
    };
    if !valid_tool_name(&name) {
        return err(
            line,
            format!(
                "tool name `{}` must be 1-128 letters, digits, `_`, `-` or `.`",
                name
            ),
        );
    }
    let description = match get("description") {
        Some(v) => want_str("tool", "description", v, line)?,
        None => {
            return err(
                line,
                format!(
                    "@tool on `{}` needs a description: @tool(description: \"what it does\")",
                    fn_name
                ),
            )
        }
    };
    let title = get("title")
        .map(|v| want_str("tool", "title", v, line))
        .transpose()?;
    let timeout = match get("timeout") {
        None => None,
        Some(v) => match v
            .as_f64()
            .filter(|s| *s > 0.0)
            .and_then(|s| Duration::try_from_secs_f64(s).ok())
        {
            Some(d) => Some(d),
            None => {
                return err(
                    line,
                    "@tool: `timeout` must be a positive number of seconds",
                )
            }
        },
    };
    let mut hints = Vec::new();
    for (key, hint) in [
        ("read_only", "readOnlyHint"),
        ("destructive", "destructiveHint"),
        ("idempotent", "idempotentHint"),
        ("open_world", "openWorldHint"),
    ] {
        if let Some(v) = get(key) {
            match v.as_bool() {
                Some(b) => hints.push((hint, b)),
                None => return err(line, format!("@tool: `{}` must be true or false", key)),
            }
        }
    }

    for (doc_name, doc) in &param_docs {
        if !params.iter().any(|p| &p.name == doc_name) {
            return err(
                line,
                format!("@param: `{}` is not a parameter of `{}`", doc_name, fn_name),
            );
        }
        want_str("param", doc_name, doc, line)?;
    }
    let mut defs = Vec::with_capacity(params.len());
    for (i, param) in params.iter().enumerate() {
        let ty = match &param.type_ann {
            None => Ty::Any,
            Some(ann) => types.resolve(ann).or_else(|e| {
                err(
                    line,
                    format!("parameter `{}` of `{}`: {}", param.name, fn_name, e),
                )
            })?,
        };
        let default = match &param.default {
            None => ParamDefault::None,
            Some(expr) => match const_json(expr) {
                Some(value) => {
                    convert(&ty, &value, &param.name).or_else(|e| {
                        err(
                            line,
                            format!("default of `{}` does not match its type: {}", param.name, e),
                        )
                    })?;
                    ParamDefault::Const(value)
                }
                None if i + 1 == params.len() => ParamDefault::Expr,
                None => {
                    return err(
                        line,
                        format!(
                            "parameter `{}` of `{}`: a default that is not a constant must be \
                             on the last parameter",
                            param.name, fn_name
                        ),
                    )
                }
            },
        };
        let description = param_docs
            .iter()
            .find(|(n, _)| n == &param.name)
            .and_then(|(_, d)| d.as_str().map(str::to_string));
        defs.push(ParamDef {
            name: param.name.clone(),
            ty,
            description,
            default,
        });
    }
    let returns = match return_type {
        None => None,
        Some(ann) => types
            .resolve_return(ann)
            .or_else(|e| err(line, format!("return type of `{}`: {}", fn_name, e)))?,
    };
    Ok(ToolDef {
        name,
        function: fn_name.to_string(),
        title,
        description,
        params: defs,
        returns,
        timeout,
        hints,
    })
}

fn build_resource(
    fn_name: &str,
    params: &[Param],
    args: Vec<(String, Json)>,
    line: usize,
) -> Result<ResourceDef, LoadError> {
    check_keys("resource", &args, RESOURCE_KEYS, line)?;
    let get = |key: &str| -> Result<Option<String>, LoadError> {
        args.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| want_str("resource", key, v, line))
            .transpose()
    };
    if params.iter().any(|p| p.default.is_none()) {
        return err(
            line,
            format!("@resource function `{}` must not take parameters", fn_name),
        );
    }
    let Some(uri) = get("uri")? else {
        return err(
            line,
            format!(
                "@resource on `{}` needs a uri: @resource(uri: \"forge://...\")",
                fn_name
            ),
        );
    };
    if !uri.contains(':') {
        return err(
            line,
            format!("@resource: `{}` is not a URI (scheme:...)", uri),
        );
    }
    Ok(ResourceDef {
        uri,
        name: get("name")?.unwrap_or_else(|| fn_name.to_string()),
        title: get("title")?,
        description: get("description")?,
        mime_type: get("mime_type")?,
        function: fn_name.to_string(),
    })
}

/// Read the `@tool` / `@resource` / `@param` declarations of a program.
fn declarations(
    program: &crate::parser::ast::Program,
) -> Result<(Vec<ToolDef>, Vec<ResourceDef>), LoadError> {
    let mut types = TypeResolver {
        structs: HashMap::new(),
        resolved: HashMap::new(),
        stack: Vec::new(),
    };
    for s in &program.statements {
        if let Stmt::StructDef { name, fields, .. } = &s.stmt {
            types.structs.insert(name.as_str(), fields.as_slice());
        }
    }
    let mut tools: Vec<ToolDef> = Vec::new();
    let mut resources: Vec<ResourceDef> = Vec::new();
    for s in &program.statements {
        let line = s.line;
        match &s.stmt {
            Stmt::DecoratorStmt(dec)
                if matches!(dec.name.as_str(), "tool" | "resource" | "param") =>
            {
                return err(
                    line,
                    format!("@{} must be followed by a function", dec.name),
                );
            }
            Stmt::FnDef {
                name,
                params,
                return_type,
                decorators,
                type_params,
                ..
            } => {
                let mut tool = None;
                let mut resource = None;
                let mut docs = Vec::new();
                for dec in decorators {
                    match dec.name.as_str() {
                        "tool" if tool.is_some() => return err(line, "@tool given twice"),
                        "tool" => tool = Some(decorator_args(dec, Some("description"), line)?),
                        "resource" if resource.is_some() => {
                            return err(line, "@resource given twice")
                        }
                        "resource" => resource = Some(decorator_args(dec, Some("uri"), line)?),
                        "param" => docs.extend(decorator_args(dec, None, line)?),
                        _ => {}
                    }
                }
                if tool.is_none() && resource.is_none() {
                    if !docs.is_empty() {
                        return err(line, format!("@param on `{}` needs @tool", name));
                    }
                    continue;
                }
                if !type_params.is_empty() {
                    return err(line, format!("tool `{}` cannot be generic", name));
                }
                if let Some(args) = tool {
                    let def = build_tool(
                        &mut types,
                        name,
                        params,
                        return_type.as_ref(),
                        args,
                        docs,
                        line,
                    )?;
                    if tools.iter().any(|t| t.name == def.name) {
                        return err(line, format!("two tools are named `{}`", def.name));
                    }
                    tools.push(def);
                }
                if let Some(args) = resource {
                    let def = build_resource(name, params, args, line)?;
                    if resources.iter().any(|r| r.uri == def.uri) {
                        return err(line, format!("two resources have the URI `{}`", def.uri));
                    }
                    resources.push(def);
                }
            }
            _ => {}
        }
    }
    Ok((tools, resources))
}

/// What loading printed and how long it took, for the operator's log.
pub struct LoadReport {
    pub stdout: String,
}

impl ToolSet {
    /// Load tools from Forge source: read the declarations, then run the
    /// top level once under `caps`, `max_time` and `limits` to build the
    /// template. Every later call runs under the same policy and limits.
    /// Errors read `path:line: message`.
    pub fn load(
        path: &Path,
        source: &str,
        caps: Capabilities,
        max_time: Duration,
        limits: Limits,
        engine: Engine,
    ) -> Result<(ToolSet, LoadReport), String> {
        let label = path.display().to_string();
        let program = parse_source(source).map_err(|e| format!("{}: {}", label, e))?;
        let (tools, resources) =
            declarations(&program).map_err(|e| format!("{}:{}: {}", label, e.line, e.message))?;
        if tools.is_empty() && resources.is_empty() {
            return Err(format!(
                "{}: no @tool or @resource functions found (annotate a top-level function \
                 with @tool(description: \"...\"))",
                label
            ));
        }
        let functions: Vec<String> = tools
            .iter()
            .map(|t| t.function.clone())
            .chain(resources.iter().map(|r| r.function.clone()))
            .collect();

        let sandbox = Sandbox::with_capabilities(caps.clone())
            .max_time(max_time)
            .max_output(CAPTURE_LIMIT)
            .limits(limits.clone())
            .source_label(label.clone());
        let source = source.to_string();
        let file = path.to_path_buf();
        let run = sandbox.run_contained(
            || (),
            &CancelHandle::new(),
            move |(), scope| build_template(scope, engine, &program, source, file, &functions),
        );
        let (template, stdout) = match run.result {
            Ok(((template, missing), stdout)) => match missing {
                Some(function) => {
                    return Err(format!(
                        "{}: `{}` is not a function after the top level ran (was it redefined?)",
                        label, function
                    ))
                }
                None => (template, stdout),
            },
            Err(SandboxError::Runtime { message, .. }) if message.starts_with(STREAM_PREFIX) => {
                return Err(format!("{}: {}", label, &message[STREAM_PREFIX.len()..]))
            }
            Err(e) => return Err(format!("{}: top level failed: {}", label, e)),
        };
        Ok((
            ToolSet {
                template,
                tools,
                resources,
                caps,
                limits,
                path: label,
            },
            LoadReport { stdout },
        ))
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn tool_names(&self) -> impl Iterator<Item = &str> {
        self.tools.iter().map(|t| t.name.as_str())
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.iter().any(|t| t.name == name)
    }

    pub fn has_resources(&self) -> bool {
        !self.resources.is_empty()
    }

    pub fn has_resource(&self, uri: &str) -> bool {
        self.resources.iter().any(|r| r.uri == uri)
    }

    /// `tools/list` entries.
    pub fn tool_definitions(&self, max_time: Duration) -> Vec<Json> {
        let caps = &self.caps;
        let may_write = [Capability::Write, Capability::Db, Capability::Run]
            .iter()
            .any(|c| caps.is_granted(*c));
        let open_world = [Capability::Net, Capability::Ai, Capability::Run]
            .iter()
            .any(|c| caps.is_granted(*c));
        self.tools
            .iter()
            .map(|tool| {
                let mut properties = Map::new();
                let mut required = Vec::new();
                for p in &tool.params {
                    let mut schema = p.ty.schema();
                    if let Some(d) = &p.description {
                        schema["description"] = json!(d);
                    }
                    match &p.default {
                        ParamDefault::Const(v) => schema["default"] = v.clone(),
                        ParamDefault::Expr => {}
                        ParamDefault::None if p.ty.is_optional() => {}
                        ParamDefault::None => required.push(json!(p.name)),
                    }
                    properties.insert(p.name.clone(), schema);
                }
                // What the policy makes impossible is a fact; the author's
                // hints may only narrow it further.
                let mut annotations = Map::new();
                annotations.insert("readOnlyHint".into(), json!(!may_write));
                annotations.insert("destructiveHint".into(), json!(may_write));
                annotations.insert("idempotentHint".into(), json!(false));
                annotations.insert("openWorldHint".into(), json!(open_world));
                for (hint, value) in &tool.hints {
                    let fixed_by_policy = match *hint {
                        "readOnlyHint" | "destructiveHint" => !may_write,
                        "openWorldHint" => !open_world,
                        _ => false,
                    };
                    if !fixed_by_policy {
                        annotations.insert((*hint).into(), json!(value));
                    }
                }
                let limit = tool.timeout.map_or(max_time, |t| t.min(max_time));
                let mut def = json!({
                    "name": tool.name,
                    "description": format!(
                        "{} (Runs in a Forge sandbox; time limit {}s.)",
                        tool.description,
                        super::fmt_secs(limit)
                    ),
                    "inputSchema": {
                        "type": "object",
                        "properties": properties,
                        "required": required,
                        "additionalProperties": false
                    },
                    "annotations": annotations,
                });
                if let Some(title) = &tool.title {
                    def["title"] = json!(title);
                }
                if let Some(ret) = &tool.returns {
                    def["outputSchema"] = if ret.is_object() {
                        ret.schema()
                    } else {
                        json!({
                            "type": "object",
                            "properties": { "result": ret.schema() },
                            "required": ["result"]
                        })
                    };
                }
                def
            })
            .collect()
    }

    /// `resources/list` entries.
    pub fn resource_definitions(&self) -> Vec<Json> {
        self.resources
            .iter()
            .map(|r| {
                let mut def = json!({ "uri": r.uri, "name": r.name });
                if let Some(t) = &r.title {
                    def["title"] = json!(t);
                }
                if let Some(d) = &r.description {
                    def["description"] = json!(d);
                }
                if let Some(m) = &r.mime_type {
                    def["mimeType"] = json!(m);
                }
                def
            })
            .collect()
    }

    /// Run `function` with `args` in a sandboxed fork of the template.
    fn invoke(
        &self,
        function: &str,
        args: Vec<Value>,
        limit: Duration,
        cancel: &CancelHandle,
    ) -> Result<(Produced, String), SandboxError> {
        let sandbox = Sandbox::with_capabilities(self.caps.clone())
            .max_time(limit)
            .max_output(CAPTURE_LIMIT)
            .limits(self.limits.clone())
            .source_label(self.path.clone());
        let function = function.to_string();
        match &self.template {
            Template::Interpreter(template) => {
                let template = template.clone();
                sandbox
                    .run_interpreter(
                        move || template.fork_for_serving(),
                        cancel,
                        move |interp| {
                            let f = interp.env.get(&function).ok_or_else(|| {
                                RuntimeError::new(&format!(
                                    "BUG: tool function `{}` is missing",
                                    function
                                ))
                            })?;
                            let value = interp.call_function(f, args)?;
                            produce(&value).map_err(|e| RuntimeError::new(&e))
                        },
                    )
                    .result
            }
            Template::Vm(template) => {
                let template = template.clone();
                sandbox
                    .run_contained(
                        || (),
                        cancel,
                        move |(), scope| {
                            // The fork is built on the call's worker, under
                            // the call's budget.
                            let mut vm = template.fork_in_current_budget(scope.cancel_flag());
                            scope.contain_vm(&mut vm);
                            let value = embed::call_global(&mut vm, &function, &args)?;
                            drop(vm);
                            produce(&value).map_err(JobError::new)
                        },
                    )
                    .result
            }
        }
    }

    /// Validate `args` against `tool`'s parameters and build the argument
    /// list. Omitted optional parameters get their constant default,
    /// `null`/`None`, or (last parameter only) the interpreter's default.
    fn arguments(tool: &ToolDef, args: &Map<String, Json>) -> Result<Vec<Value>, Vec<String>> {
        let mut problems = Vec::new();
        for key in args.keys() {
            if !tool.params.iter().any(|p| &p.name == key) {
                problems.push(format!("`{}`: unknown argument", key));
            }
        }
        let mut values = Vec::with_capacity(tool.params.len());
        for p in &tool.params {
            let given = args.get(&p.name);
            let value = match (given, &p.default) {
                (Some(v), _) => convert(&p.ty, v, &p.name),
                (None, ParamDefault::Const(d)) => convert(&p.ty, d, &p.name),
                // Last parameter: a shorter list lets the interpreter
                // evaluate the default.
                (None, ParamDefault::Expr) => break,
                (None, ParamDefault::None) if p.ty.is_optional() => {
                    convert(&p.ty, &Json::Null, &p.name)
                }
                (None, ParamDefault::None) => Err(format!(
                    "`{}`: missing required argument ({})",
                    p.name,
                    p.ty.describe()
                )),
            };
            match value {
                Ok(v) => values.push(v),
                Err(e) => problems.push(e),
            }
        }
        if problems.is_empty() {
            Ok(values)
        } else {
            Err(problems)
        }
    }

    /// Handle `tools/call` for a Forge tool. Bad arguments, failures and
    /// `Err` results are tool errors (`isError`), so the model can correct
    /// itself.
    pub fn call(
        &self,
        name: &str,
        args: &Map<String, Json>,
        max_time: Duration,
        max_response_bytes: usize,
        cancel: &CancelHandle,
    ) -> Json {
        let Some(tool) = self.tools.iter().find(|t| t.name == name) else {
            return tool_error(&format!("unknown tool `{}`", name));
        };
        let values = match Self::arguments(tool, args) {
            Ok(v) => v,
            Err(problems) => {
                return tool_error(&format!(
                    "invalid arguments for `{}`:\n- {}",
                    tool.name,
                    problems.join("\n- ")
                ))
            }
        };
        let limit = tool.timeout.map_or(max_time, |t| t.min(max_time));
        let (produced, stdout) = match self.invoke(&tool.function, values, limit, cancel) {
            Ok(r) => r,
            Err(e) => return failure(&e, max_response_bytes),
        };
        let mut content = Vec::new();
        if !stdout.is_empty() {
            content
                .push(json!({ "type": "text", "text": truncate_utf8(stdout, max_response_bytes) }));
        }
        let value = match produced {
            Produced::Err(v) => {
                content.push(json!({ "type": "text", "text": render_text(&v) }));
                return json!({ "content": content, "isError": true });
            }
            Produced::Ok(v) => v,
        };
        if let Some(ret) = &tool.returns {
            if let Err(e) = check(ret, &value, "result") {
                return tool_error(&format!(
                    "`{}` returned a value that does not match its declared return type: {}",
                    tool.name, e
                ));
            }
        }
        let wraps = match &tool.returns {
            Some(ret) => !ret.is_object(),
            None => !value.is_object(),
        };
        let structured = if wraps {
            json!({ "result": value })
        } else {
            value.clone()
        };
        let size = structured.to_string().len();
        if size > max_response_bytes {
            return tool_error(&format!(
                "`{}` returned {} bytes of JSON; this server returns at most {}",
                tool.name, size, max_response_bytes
            ));
        }
        if !value.is_null() || content.is_empty() {
            content.push(json!({ "type": "text", "text": render_text(&value) }));
        }
        json!({ "content": content, "structuredContent": structured })
    }

    /// Handle `resources/read`: `Ok(contents)` or a JSON-RPC error message.
    pub fn read_resource(
        &self,
        uri: &str,
        max_time: Duration,
        max_response_bytes: usize,
        cancel: &CancelHandle,
    ) -> Result<Json, String> {
        let Some(res) = self.resources.iter().find(|r| r.uri == uri) else {
            return Err(format!("unknown resource `{}`", uri));
        };
        let (produced, _stdout) = self
            .invoke(&res.function, Vec::new(), max_time, cancel)
            .map_err(|e| format!("reading `{}` failed: {}", uri, e))?;
        let value = match produced {
            Produced::Ok(v) => v,
            Produced::Err(v) => {
                return Err(format!("reading `{}` failed: {}", uri, render_text(&v)))
            }
        };
        let (text, mime) = match &value {
            Json::String(s) => (
                s.clone(),
                res.mime_type.clone().unwrap_or_else(|| "text/plain".into()),
            ),
            other => (
                serde_json::to_string_pretty(other).unwrap_or_default(),
                res.mime_type
                    .clone()
                    .unwrap_or_else(|| "application/json".into()),
            ),
        };
        if text.len() > max_response_bytes {
            return Err(format!(
                "`{}` is {} bytes; this server returns at most {}",
                uri,
                text.len(),
                max_response_bytes
            ));
        }
        Ok(json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": text }] }))
    }
}

/// Text for a result value: strings as is, everything else as JSON.
fn render_text(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Null => "null".to_string(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

/// A failed tool call, with whatever it printed first.
fn failure(e: &SandboxError, max_response_bytes: usize) -> Json {
    let mut text = e.to_string();
    let stdout = e.stdout();
    if !stdout.is_empty() {
        text.push_str("\n\nOutput before the error:\n");
        text.push_str(&truncate_utf8(stdout.to_string(), max_response_bytes));
    }
    let mut error = json!({ "kind": e.kind(), "message": e.to_string() });
    if let SandboxError::Runtime { line, message, .. } = e {
        if *line > 0 {
            error["line"] = json!(line);
        }
        // Stable code and hint (`forge explain <code>`), as in run_forge.
        error["code"] = json!(crate::semantics::errors::classify(message).code);
        error["hint"] = json!(crate::semantics::errors::hint_for(message));
    }
    let mut result = tool_error(&text);
    result["_meta"] = json!({ "forge/error": error });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINES: [Engine; 2] = [Engine::Vm, Engine::Interpreter];

    fn load(engine: Engine, src: &str) -> Result<ToolSet, String> {
        ToolSet::load(
            Path::new("t.fg"),
            src,
            Capabilities::deny_all(),
            Duration::from_secs(5),
            super::super::default_limits(),
            engine,
        )
        .map(|(t, _)| t)
    }

    fn args(v: Json) -> Map<String, Json> {
        v.as_object().cloned().unwrap_or_default()
    }

    fn call(set: &ToolSet, name: &str, a: Json) -> Json {
        set.call(
            name,
            &args(a),
            Duration::from_secs(5),
            65536,
            &CancelHandle::new(),
        )
    }

    #[test]
    fn schemas_follow_type_annotations() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            schemas_follow_type_annotations_on(engine);
        }
    }

    fn schemas_follow_type_annotations_on(engine: Engine) {
        let set = load(
            engine,
            "struct Point { x: Float, y: Float, label: String = \"p\" }\n\
             @tool(description: \"d\")\n\
             @param(n: \"count\")\n\
             fn t(n: Int, f: Float, s: String, b: Bool, xs: [Int], m: Map<String, Int>, \
                  o: Object, p: Point, maybe: ?String, opt: Option<Int>, any, d: Int = 3) -> Point { return p }",
        )
        .expect("loads");
        let defs = set.tool_definitions(Duration::from_secs(30));
        let schema = &defs[0]["inputSchema"];
        let props = &schema["properties"];
        assert_eq!(
            props["n"],
            json!({"type": "integer", "description": "count"})
        );
        assert_eq!(props["f"]["type"], "number");
        assert_eq!(props["s"]["type"], "string");
        assert_eq!(props["b"]["type"], "boolean");
        assert_eq!(
            props["xs"],
            json!({"type": "array", "items": {"type": "integer"}})
        );
        assert_eq!(props["m"]["additionalProperties"]["type"], "integer");
        assert_eq!(props["o"], json!({"type": "object"}));
        assert_eq!(props["p"]["required"], json!(["x", "y"]));
        assert_eq!(props["p"]["properties"]["label"]["default"], "p");
        assert_eq!(props["maybe"]["type"], json!(["string", "null"]));
        assert_eq!(props["opt"]["type"], json!(["integer", "null"]));
        assert_eq!(props["any"], json!({}));
        assert_eq!(props["d"]["default"], 3);
        assert_eq!(
            schema["required"],
            json!(["n", "f", "s", "b", "xs", "m", "o", "p", "any"])
        );
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(defs[0]["outputSchema"]["title"], "Point");
    }

    #[test]
    fn arguments_are_validated_and_converted() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            arguments_are_validated_and_converted_on(engine);
        }
    }

    fn arguments_are_validated_and_converted_on(engine: Engine) {
        let set = load(
            engine,
            "struct P { x: Int }\n\
             @tool(\"add\")\nfn add(a: Int, b: Float = 0.5, o: Option<Int>) -> Float {\n\
               let extra = if is_some(o) { unwrap(o) } else { 0 }\n  return a + b + extra }\n\
             @tool(\"px\")\nfn px(p: P) -> Int { return p.x }",
        )
        .expect("loads");
        let ok = call(&set, "add", json!({"a": 2}));
        assert_eq!(ok["structuredContent"], json!({"result": 2.5}), "{ok}");
        let ok = call(&set, "add", json!({"a": 2.0, "b": 1, "o": 4}));
        assert_eq!(ok["structuredContent"]["result"], 7.0, "{ok}");
        let bad = call(&set, "add", json!({"a": "2", "zzz": 1}));
        assert_eq!(bad["isError"], true);
        let text = bad["content"][0]["text"].as_str().unwrap_or_default();
        assert!(text.contains("`zzz`: unknown argument"), "{text}");
        assert!(
            text.contains("`a`: expected an integer, got a string"),
            "{text}"
        );
        let missing = call(&set, "add", json!({}));
        assert!(missing["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("`a`: missing required argument")));
        let nested = call(&set, "px", json!({"p": {"x": 1.5}}));
        assert!(
            nested["content"][0]["text"]
                .as_str()
                .is_some_and(|t| t.contains("`p.x`: expected an integer")),
            "{nested}"
        );
        assert_eq!(
            call(&set, "px", json!({"p": {"x": 7}}))["structuredContent"]["result"],
            7
        );
    }

    #[test]
    fn results_errors_and_return_checks() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            results_errors_and_return_checks_on(engine);
        }
    }

    fn results_errors_and_return_checks_on(engine: Engine) {
        let set = load(
            engine,
            "@tool(\"e\")\nfn e(x: Int) { if x > 0 { return Err(\"too big\") }\n return Ok({ v: x }) }\n\
             @tool(\"lie\")\nfn lie() -> Int { return \"nope\" }\n\
             @tool(\"talk\")\nfn talk() { say \"hello\" }\n\
             @tool(\"boom\")\nfn boom() { say \"partial\"\n let x = 1 / 0 }",
        )
        .expect("loads");
        let err = call(&set, "e", json!({"x": 1}));
        assert_eq!(err["isError"], true);
        assert_eq!(err["content"][0]["text"], "too big");
        let ok = call(&set, "e", json!({"x": 0}));
        assert_eq!(ok["structuredContent"], json!({"v": 0}));
        let lie = call(&set, "lie", json!({}));
        assert_eq!(lie["isError"], true, "{lie}");
        let talk = call(&set, "talk", json!({}));
        assert_eq!(talk["content"][0]["text"], "hello\n");
        assert_eq!(talk["structuredContent"], json!({"result": null}));
        let boom = call(&set, "boom", json!({}));
        assert_eq!(boom["isError"], true);
        assert_eq!(boom["_meta"]["forge/error"]["kind"], "runtime");
        assert!(boom["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("Output before the error:\npartial")));
    }

    #[test]
    fn calls_run_under_fresh_resource_budgets() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            calls_run_under_fresh_resource_budgets_on(engine);
        }
    }

    fn calls_run_under_fresh_resource_budgets_on(engine: Engine) {
        let (set, _) = ToolSet::load(
            Path::new("t.fg"),
            "@tool(\"spin\")\nfn spin(n: Int) { let mut i = 0\n while i < n { i = i + 1 }\n return i }\n\
             @tool(\"boom\")\nfn boom() { let x = 1 / 0 }",
            Capabilities::deny_all(),
            Duration::from_secs(30),
            Limits {
                max_fuel: Some(10_000),
                ..Limits::none()
            },
            engine,
        )
        .expect("loads");
        // Each call gets its own fuel: many small calls all succeed...
        for _ in 0..5 {
            let ok = call(&set, "spin", json!({"n": 1000}));
            assert_eq!(ok["structuredContent"]["result"], 1000, "{ok}");
        }
        // ...and one call that needs more than the budget fails.
        let spin = call(&set, "spin", json!({"n": 1000000}));
        assert_eq!(spin["isError"], true);
        assert_eq!(
            spin["_meta"]["forge/error"]["kind"], "fuel_exhausted",
            "{spin}"
        );
        // Runtime errors carry the stable error code and hint.
        let boom = call(&set, "boom", json!({}));
        let error = &boom["_meta"]["forge/error"];
        assert_eq!(error["kind"], "runtime");
        assert!(
            error["code"].as_str().is_some_and(|c| c.starts_with('E')),
            "{boom}"
        );
        assert!(error["hint"].as_str().is_some_and(|h| !h.is_empty()));
    }

    #[test]
    fn calls_are_isolated_forks() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            calls_are_isolated_forks_on(engine);
        }
    }

    fn calls_are_isolated_forks_on(engine: Engine) {
        let set = load(
            engine,
            "let mut hits = 0\n@tool(\"count\")\nfn count() -> Int { hits = hits + 1\n return hits }",
        )
        .expect("loads");
        for _ in 0..3 {
            assert_eq!(
                call(&set, "count", json!({}))["structuredContent"]["result"],
                1
            );
        }
    }

    #[test]
    fn load_errors_point_at_the_declaration() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            load_errors_point_at_the_declaration_on(engine);
        }
    }

    fn load_errors_point_at_the_declaration_on(engine: Engine) {
        for (src, needle) in [
            ("say 1", "no @tool"),
            ("@tool\nfn f() {}", "needs a description"),
            (
                "@tool(\"d\", bogus: 1)\nfn f() {}",
                "unknown argument `bogus`",
            ),
            (
                "@tool(\"d\")\nfn f(t: (Int, Int)) {}",
                "cannot be described as JSON",
            ),
            (
                "@tool(\"d\")\n@param(y: \"?\")\nfn f(x) {}",
                "`y` is not a parameter",
            ),
            (
                "@tool(\"d\")\nfn f(x = now(), y) {}",
                "must be on the last parameter",
            ),
            (
                "struct N { next: N }\n@tool(\"d\")\nfn f(n: N) {}",
                "contains itself",
            ),
            (
                "@tool(\"d\")\nfn f() {}\n@tool(\"d\", name: \"f\")\nfn g() {}",
                "two tools",
            ),
            ("@resource(description: \"x\")\nfn r() {}", "needs a uri"),
            (
                "@resource(\"forge://x\")\nfn r(a) {}",
                "must not take parameters",
            ),
            ("@tool(\"d\")\nfn f() {}\nlet x = 1 / 0", "top level failed"),
            (
                "@tool(\"d\")\nfn f() {}\nfs.read(\"/etc/hostname\")",
                "permission denied: fs.read",
            ),
        ] {
            match load(engine, src) {
                Ok(_) => panic!("{src:?} loaded"),
                Err(e) => assert!(e.contains(needle), "{src:?}: {e}"),
            }
        }
        let e = load(engine, "let a = 1\n\n@tool\nfn f() {}")
            .err()
            .unwrap_or_default();
        assert!(e.starts_with("t.fg:3: "), "{e}");
    }

    #[test]
    fn resources_are_read_through_a_fork() {
        for engine in ENGINES {
            eprintln!("engine: {engine}");
            resources_are_read_through_a_fork_on(engine);
        }
    }

    fn resources_are_read_through_a_fork_on(engine: Engine) {
        let set = load(
            engine,
            "@resource(uri: \"forge://units\", description: \"units\")\nfn units() { return [\"C\", \"F\"] }\n\
             @resource(\"forge://readme\")\nfn readme() { return \"hi\" }",
        )
        .expect("loads");
        let limit = Duration::from_secs(5);
        let c = CancelHandle::new();
        let units = set
            .read_resource("forge://units", limit, 1000, &c)
            .expect("reads");
        assert_eq!(units["contents"][0]["mimeType"], "application/json");
        let readme = set
            .read_resource("forge://readme", limit, 1000, &c)
            .expect("reads");
        assert_eq!(readme["contents"][0]["text"], "hi");
        assert!(set.read_resource("forge://nope", limit, 1000, &c).is_err());
        assert_eq!(set.resource_definitions()[0]["description"], "units");
    }
}
