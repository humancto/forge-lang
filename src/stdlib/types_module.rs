//! `__types`: the hidden module behind `--strict` runtime enforcement of
//! type annotations. Programs never call it directly; the front end inserts
//! `__types.check(value, type, context)` at annotated function boundaries
//! and `__types.bind(...)` at the entry of generic functions (see
//! `typechecker::enforce`). The rule lives in `semantics::types`, and
//! both engines reach it through the module registry.

use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "check".to_string(),
        Value::BuiltIn("__types.check".to_string()),
    );
    m.insert(
        "bind".to_string(),
        Value::BuiltIn("__types.bind".to_string()),
    );
    m.insert(
        "widen".to_string(),
        Value::BuiltIn("__types.widen".to_string()),
    );
    Value::Object(m)
}

/// `__types.check(value, type, context)`: return `value` if it fits `type`
/// (canonical notation, see `semantics::types`), with Ints widened where
/// `type` declares Float (`semantics::types::widen`), else fail with a type
/// error naming `context`. `__types.widen(value, type)`: only the widening,
/// never an error (annotated `let`s and struct fields, which are checked
/// statically).
pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "__types.check" => {
            let mut args = args.into_iter();
            let value = args.next().unwrap_or(Value::Null);
            let (Some(Value::String(ty)), Some(Value::String(context))) =
                (args.next(), args.next())
            else {
                return Err("__types.check(value, type, context) expects two strings".into());
            };
            let expected = crate::semantics::types::parse_cached(&ty)?;
            if crate::semantics::types::fits(&value, &expected) {
                Ok(crate::semantics::types::widen(&value, &expected).unwrap_or(value))
            } else {
                Err(crate::semantics::types::violation(
                    &context, &expected, &value,
                ))
            }
        }
        "__types.widen" => {
            let mut args = args.into_iter();
            let value = args.next().unwrap_or(Value::Null);
            let Some(Value::String(ty)) = args.next() else {
                return Err("__types.widen(value, type) expects a type string".into());
            };
            let expected = crate::semantics::types::parse_cached(&ty)?;
            if crate::semantics::types::fits(&value, &expected) {
                Ok(crate::semantics::types::widen(&value, &expected).unwrap_or(value))
            } else {
                Ok(value)
            }
        }
        // `__types.bind(type_param, function, names, values...)`: the type
        // a generic parameter stands for in this call (canonical notation),
        // from the arguments declared exactly as that parameter (`names` is
        // their comma-separated parameter names).
        "__types.bind" => {
            let mut args = args.into_iter();
            let (
                Some(Value::String(type_param)),
                Some(Value::String(function)),
                Some(Value::String(names)),
            ) = (args.next(), args.next(), args.next())
            else {
                return Err(
                    "__types.bind(type_param, function, names, values...) expects three strings"
                        .into(),
                );
            };
            let values: Vec<Value> = args.collect();
            let names: Vec<&str> = names.split(',').collect();
            let name = |i: usize| names.get(i).copied().unwrap_or("?").to_string();
            match crate::semantics::types::bind_param(&values) {
                Ok(bound) => Ok(Value::String(bound)),
                Err((i, first, bound)) => Err(crate::semantics::types::param_violation(
                    &function,
                    &name(i),
                    &type_param,
                    &name(first),
                    &bound,
                    &values[i],
                )),
            }
        }
        _ => Err(format!("unknown function: {}", name)),
    }
}
