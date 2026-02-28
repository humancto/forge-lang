use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("get".to_string(), Value::BuiltIn("env.get".to_string()));
    m.insert("set".to_string(), Value::BuiltIn("env.set".to_string()));
    m.insert("keys".to_string(), Value::BuiltIn("env.keys".to_string()));
    m.insert("has".to_string(), Value::BuiltIn("env.has".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "env.get" => {
            let key = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err("env.get() requires a string key".to_string()),
            };
            let default = args.get(1).and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            });
            match std::env::var(&key) {
                Ok(val) => Ok(Value::String(val)),
                Err(_) => match default {
                    Some(d) => Ok(Value::String(d)),
                    None => Ok(Value::Null),
                },
            }
        }
        "env.set" => match (args.first(), args.get(1)) {
            (Some(Value::String(key)), Some(Value::String(val))) => {
                std::env::set_var(key, val);
                Ok(Value::Null)
            }
            _ => Err("env.set() requires (key, value) strings".to_string()),
        },
        "env.keys" => {
            let keys: Vec<Value> = std::env::vars().map(|(k, _)| Value::String(k)).collect();
            Ok(Value::Array(keys))
        }
        "env.has" => match args.first() {
            Some(Value::String(key)) => Ok(Value::Bool(std::env::var(key).is_ok())),
            _ => Err("env.has() requires a string key".to_string()),
        },
        _ => Err(format!("unknown env function: {}", name)),
    }
}
