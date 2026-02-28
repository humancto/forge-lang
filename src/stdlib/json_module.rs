use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "parse".to_string(),
        Value::BuiltIn("json.parse".to_string()),
    );
    m.insert(
        "stringify".to_string(),
        Value::BuiltIn("json.stringify".to_string()),
    );
    m.insert(
        "pretty".to_string(),
        Value::BuiltIn("json.pretty".to_string()),
    );
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "json.parse" => match args.first() {
            Some(Value::String(s)) => match serde_json::from_str::<serde_json::Value>(s) {
                Ok(v) => Ok(json_to_forge(v)),
                Err(e) => Err(format!("JSON parse error: {}", e)),
            },
            _ => Err("json.parse() requires a string".to_string()),
        },
        "json.stringify" => match args.first() {
            Some(v) => Ok(Value::String(forge_to_json_compact(v))),
            None => Err("json.stringify() requires a value".to_string()),
        },
        "json.pretty" => match args.first() {
            Some(v) => {
                let indent = args
                    .get(1)
                    .and_then(|v| {
                        if let Value::Int(n) = v {
                            Some(*n as usize)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(2);
                Ok(Value::String(forge_to_json_pretty(v, 0, indent)))
            }
            None => Err("json.pretty() requires a value".to_string()),
        },
        _ => Err(format!("unknown json function: {}", name)),
    }
}

fn json_to_forge(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(items) => {
            Value::Array(items.into_iter().map(json_to_forge).collect())
        }
        serde_json::Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, json_to_forge(v)))
                .collect(),
        ),
    }
}

fn forge_to_json_compact(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Float(n) => format!("{}", n),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Array(items) => {
            let entries: Vec<String> = items.iter().map(forge_to_json_compact).collect();
            format!("[{}]", entries.join(", "))
        }
        Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, forge_to_json_compact(v)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        _ => "null".to_string(),
    }
}

fn forge_to_json_pretty(v: &Value, depth: usize, indent: usize) -> String {
    let pad = " ".repeat(depth * indent);
    let inner_pad = " ".repeat((depth + 1) * indent);
    match v {
        Value::Int(n) => n.to_string(),
        Value::Float(n) => format!("{}", n),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }
            let entries: Vec<String> = items
                .iter()
                .map(|v| {
                    format!(
                        "{}{}",
                        inner_pad,
                        forge_to_json_pretty(v, depth + 1, indent)
                    )
                })
                .collect();
            format!("[\n{}\n{}]", entries.join(",\n"), pad)
        }
        Value::Object(map) => {
            if map.is_empty() {
                return "{}".to_string();
            }
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}\"{}\": {}",
                        inner_pad,
                        k,
                        forge_to_json_pretty(v, depth + 1, indent)
                    )
                })
                .collect();
            format!("{{\n{}\n{}}}", entries.join(",\n"), pad)
        }
        _ => "null".to_string(),
    }
}
