//! `__types`: the hidden module behind `--strict` runtime enforcement of
//! type annotations. Programs never call it directly; the front end inserts
//! `__types.check(value, type, context)` at annotated function boundaries
//! (see `typechecker::enforce`). The rule lives in `semantics::types`, and
//! both engines reach it through the module registry.

use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "check".to_string(),
        Value::BuiltIn("__types.check".to_string()),
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
        _ => Err(format!("unknown function: {}", name)),
    }
}
