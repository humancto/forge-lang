use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("test".to_string(), Value::BuiltIn("regex.test".to_string()));
    m.insert("find".to_string(), Value::BuiltIn("regex.find".to_string()));
    m.insert(
        "find_all".to_string(),
        Value::BuiltIn("regex.find_all".to_string()),
    );
    m.insert(
        "replace".to_string(),
        Value::BuiltIn("regex.replace".to_string()),
    );
    m.insert(
        "split".to_string(),
        Value::BuiltIn("regex.split".to_string()),
    );
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "regex.test" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = regex::Regex::new(pattern).map_err(|e| format!("invalid regex: {}", e))?;
                Ok(Value::Bool(re.is_match(text)))
            }
            _ => Err("regex.test() requires (text, pattern) strings".to_string()),
        },
        "regex.find" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = regex::Regex::new(pattern).map_err(|e| format!("invalid regex: {}", e))?;
                match re.find(text) {
                    Some(m) => Ok(Value::String(m.as_str().to_string())),
                    None => Ok(Value::Null),
                }
            }
            _ => Err("regex.find() requires (text, pattern) strings".to_string()),
        },
        "regex.find_all" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = regex::Regex::new(pattern).map_err(|e| format!("invalid regex: {}", e))?;
                let matches: Vec<Value> = re
                    .find_iter(text)
                    .map(|m| Value::String(m.as_str().to_string()))
                    .collect();
                Ok(Value::Array(matches))
            }
            _ => Err("regex.find_all() requires (text, pattern) strings".to_string()),
        },
        "regex.replace" => match (args.first(), args.get(1), args.get(2)) {
            (
                Some(Value::String(text)),
                Some(Value::String(pattern)),
                Some(Value::String(replacement)),
            ) => {
                let re = regex::Regex::new(pattern).map_err(|e| format!("invalid regex: {}", e))?;
                Ok(Value::String(
                    re.replace_all(text, replacement.as_str()).to_string(),
                ))
            }
            _ => Err("regex.replace() requires (text, pattern, replacement) strings".to_string()),
        },
        "regex.split" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = regex::Regex::new(pattern).map_err(|e| format!("invalid regex: {}", e))?;
                let parts: Vec<Value> = re
                    .split(text)
                    .map(|s| Value::String(s.to_string()))
                    .collect();
                Ok(Value::Array(parts))
            }
            _ => Err("regex.split() requires (text, pattern) strings".to_string()),
        },
        _ => Err(format!("unknown regex function: {}", name)),
    }
}
