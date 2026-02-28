use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("parse".to_string(), Value::BuiltIn("csv.parse".to_string()));
    m.insert(
        "stringify".to_string(),
        Value::BuiltIn("csv.stringify".to_string()),
    );
    m.insert("read".to_string(), Value::BuiltIn("csv.read".to_string()));
    m.insert("write".to_string(), Value::BuiltIn("csv.write".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "csv.parse" => match args.first() {
            Some(Value::String(s)) => Ok(parse_csv(s)),
            _ => Err("csv.parse() requires a string".to_string()),
        },
        "csv.stringify" => match args.first() {
            Some(Value::Array(rows)) => Ok(Value::String(stringify_csv(rows))),
            _ => Err("csv.stringify() requires an array of objects".to_string()),
        },
        "csv.read" => match args.first() {
            Some(Value::String(path)) => {
                let content =
                    std::fs::read_to_string(path).map_err(|e| format!("csv.read error: {}", e))?;
                Ok(parse_csv(&content))
            }
            _ => Err("csv.read() requires a file path".to_string()),
        },
        "csv.write" => match (args.first(), args.get(1)) {
            (Some(Value::String(path)), Some(Value::Array(rows))) => {
                let content = stringify_csv(rows);
                std::fs::write(path, &content)
                    .map(|_| Value::Null)
                    .map_err(|e| format!("csv.write error: {}", e))
            }
            _ => Err("csv.write() requires (path, array)".to_string()),
        },
        _ => Err(format!("unknown csv function: {}", name)),
    }
}

fn parse_csv(input: &str) -> Value {
    let mut lines = input.lines();
    let headers: Vec<String> = match lines.next() {
        Some(h) => h
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .collect(),
        None => return Value::Array(Vec::new()),
    };

    let rows: Vec<Value> = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut map = IndexMap::new();
            let values: Vec<&str> = line.split(',').collect();
            for (i, header) in headers.iter().enumerate() {
                let val = values
                    .get(i)
                    .map(|v| v.trim().trim_matches('"'))
                    .unwrap_or("");
                let forge_val = if let Ok(n) = val.parse::<i64>() {
                    Value::Int(n)
                } else if let Ok(n) = val.parse::<f64>() {
                    Value::Float(n)
                } else if val == "true" || val == "false" {
                    Value::Bool(val == "true")
                } else {
                    Value::String(val.to_string())
                };
                map.insert(header.clone(), forge_val);
            }
            Value::Object(map)
        })
        .collect();

    Value::Array(rows)
}

fn stringify_csv(rows: &[Value]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let headers: Vec<String> = if let Some(Value::Object(first)) = rows.first() {
        first.keys().cloned().collect()
    } else {
        return String::new();
    };

    let mut output = headers.join(",");
    output.push('\n');

    for row in rows {
        if let Value::Object(map) = row {
            let values: Vec<String> = headers
                .iter()
                .map(|h| {
                    let val = map.get(h).map(|v| format!("{}", v)).unwrap_or_default();
                    if val.contains(',') || val.contains('"') {
                        format!("\"{}\"", val.replace('"', "\"\""))
                    } else {
                        val
                    }
                })
                .collect();
            output.push_str(&values.join(","));
            output.push('\n');
        }
    }

    output
}
