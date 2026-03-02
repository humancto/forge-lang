use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "parse".to_string(),
        Value::BuiltIn("toml.parse".to_string()),
    );
    m.insert(
        "stringify".to_string(),
        Value::BuiltIn("toml.stringify".to_string()),
    );
    m.insert("read".to_string(), Value::BuiltIn("toml.read".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "toml.parse" => match args.first() {
            Some(Value::String(s)) => {
                let val: toml::Value =
                    toml::from_str(s).map_err(|e| format!("TOML parse error: {}", e))?;
                Ok(toml_to_forge(val))
            }
            _ => Err("toml.parse() requires a string".to_string()),
        },
        "toml.stringify" => match args.first() {
            Some(v) => {
                let toml_val = forge_to_toml(v)?;
                let s = toml::to_string_pretty(&toml_val)
                    .map_err(|e| format!("TOML stringify error: {}", e))?;
                Ok(Value::String(s))
            }
            None => Err("toml.stringify() requires a value".to_string()),
        },
        "toml.read" => match args.first() {
            Some(Value::String(path)) => {
                let content =
                    std::fs::read_to_string(path).map_err(|e| format!("file read error: {}", e))?;
                let val: toml::Value =
                    toml::from_str(&content).map_err(|e| format!("TOML parse error: {}", e))?;
                Ok(toml_to_forge(val))
            }
            _ => Err("toml.read() requires a file path string".to_string()),
        },
        _ => Err(format!("unknown toml function: {}", name)),
    }
}

fn toml_to_forge(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Int(i),
        toml::Value::Float(f) => Value::Float(f),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.into_iter().map(toml_to_forge).collect()),
        toml::Value::Table(table) => {
            let mut map = IndexMap::new();
            for (k, v) in table {
                map.insert(k, toml_to_forge(v));
            }
            Value::Object(map)
        }
    }
}

fn forge_to_toml(v: &Value) -> Result<toml::Value, String> {
    match v {
        Value::String(s) => Ok(toml::Value::String(s.clone())),
        Value::Int(n) => Ok(toml::Value::Integer(*n)),
        Value::Float(f) => Ok(toml::Value::Float(*f)),
        Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        Value::Array(arr) => {
            let items: Result<Vec<toml::Value>, String> = arr.iter().map(forge_to_toml).collect();
            Ok(toml::Value::Array(items?))
        }
        Value::Object(map) => {
            let mut table = toml::map::Map::new();
            for (k, v) in map {
                table.insert(k.clone(), forge_to_toml(v)?);
            }
            Ok(toml::Value::Table(table))
        }
        Value::Null => Ok(toml::Value::String("null".to_string())),
        _ => Err(format!("cannot convert {} to TOML", v)),
    }
}
