use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("info".to_string(), Value::BuiltIn("log.info".to_string()));
    m.insert("warn".to_string(), Value::BuiltIn("log.warn".to_string()));
    m.insert("error".to_string(), Value::BuiltIn("log.error".to_string()));
    m.insert("debug".to_string(), Value::BuiltIn("log.debug".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
    let message = text.join(" ");
    let now = chrono::Local::now().format("%H:%M:%S");

    match name {
        "log.info" => {
            eprintln!("\x1B[32m[{} INFO]\x1B[0m  {}", now, message);
        }
        "log.warn" => {
            eprintln!("\x1B[33m[{} WARN]\x1B[0m  {}", now, message);
        }
        "log.error" => {
            eprintln!("\x1B[31m[{} ERROR]\x1B[0m {}", now, message);
        }
        "log.debug" => {
            eprintln!("\x1B[90m[{} DEBUG]\x1B[0m {}", now, message);
        }
        _ => return Err(format!("unknown log function: {}", name)),
    }
    Ok(Value::Null)
}
