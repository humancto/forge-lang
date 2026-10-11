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
    Value::Object(m)
}

/// `__types.check(value, type, context)`: return `value` if it fits `type`
/// (canonical notation, see `semantics::types`), else fail with a type
/// error naming `context`.
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
                Ok(value)
            } else {
                Err(crate::semantics::types::violation(
                    &context, &expected, &value,
                ))
            }
        }
        // `__types.bind(values, names, type_param, function)`: the type a
        // generic parameter stands for in this call (canonical notation),
        // from the arguments declared exactly as that parameter.
        "__types.bind" => {
            let mut args = args.into_iter();
            let (
                Some(Value::Array(values)),
                Some(Value::Array(names)),
                Some(Value::String(type_param)),
                Some(Value::String(function)),
            ) = (args.next(), args.next(), args.next(), args.next())
            else {
                return Err(
                    "__types.bind(values, names, type_param, function) expects two arrays and two strings"
                        .into(),
                );
            };
            let name = |i: usize| match names.get(i) {
                Some(Value::String(n)) => n.clone(),
                _ => format!("#{}", i + 1),
            };
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
