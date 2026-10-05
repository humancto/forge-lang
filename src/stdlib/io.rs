use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "prompt".to_string(),
        Value::BuiltIn("io.prompt".to_string()),
    );
    m.insert("print".to_string(), Value::BuiltIn("io.print".to_string()));
    m.insert("args".to_string(), Value::BuiltIn("io.args".to_string()));
    m.insert(
        "args_parse".to_string(),
        Value::BuiltIn("io.args_parse".to_string()),
    );
    m.insert(
        "args_get".to_string(),
        Value::BuiltIn("io.args_get".to_string()),
    );
    m.insert(
        "args_has".to_string(),
        Value::BuiltIn("io.args_has".to_string()),
    );
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "io.prompt" => {
            let prompt_text = args.first().map(|v| format!("{}", v)).unwrap_or_default();
            use std::io::Write;
            print!("{}", prompt_text);
            std::io::stdout().flush().ok();
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .map_err(|e| format!("io.prompt error: {}", e))?;
            Ok(Value::String(input.trim_end_matches('\n').to_string()))
        }
        "io.print" => {
            let text: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
            print!("{}", text.join(" "));
            Ok(Value::Null)
        }
        "io.args" => {
            let args: Vec<Value> = std::env::args().map(Value::String).collect();
            Ok(Value::Array(args))
        }
        "io.args_parse" => {
            let cli_args: Vec<String> = std::env::args().collect();
            let mut result = IndexMap::new();
            let mut i = 0;
            while i < cli_args.len() {
                let arg = &cli_args[i];
                if arg.starts_with("--") {
                    let key = arg.clone();
                    if i + 1 < cli_args.len() && !cli_args[i + 1].starts_with("--") {
                        result.insert(key, Value::String(cli_args[i + 1].clone()));
                        i += 2;
                    } else {
                        result.insert(key, Value::Bool(true));
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            Ok(Value::Object(result))
        }
        "io.args_get" => {
            let flag = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err("io.args_get() requires a flag string".to_string()),
            };
            let cli_args: Vec<String> = std::env::args().collect();
            for (i, arg) in cli_args.iter().enumerate() {
                if *arg == flag {
                    if i + 1 < cli_args.len() && !cli_args[i + 1].starts_with("--") {
                        return Ok(Value::String(cli_args[i + 1].clone()));
                    }
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Null)
        }
        "io.args_has" => {
            let flag = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err("io.args_has() requires a flag string".to_string()),
            };
            let cli_args: Vec<String> = std::env::args().collect();
            Ok(Value::Bool(cli_args.contains(&flag)))
        }
        _ => Err(format!("unknown io function: {}", name)),
    }
}
