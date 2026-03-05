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

/// Parse a single CSV line respecting RFC 4180 quoted fields.
/// Handles: quoted commas, escaped quotes (""), CRLF line endings.
fn parse_csv_line(line: &str) -> Vec<String> {
    let line = line.trim_end_matches('\r');
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    // Escaped quote ""
                    current.push('"');
                    chars.next();
                } else {
                    // End of quoted field
                    in_quotes = false;
                }
            } else {
                current.push(ch);
            }
        } else {
            match ch {
                ',' => {
                    fields.push(current.trim().to_string());
                    current = String::new();
                }
                '"' => {
                    in_quotes = true;
                }
                _ => {
                    current.push(ch);
                }
            }
        }
    }
    fields.push(current.trim().to_string());
    fields
}

fn parse_csv(input: &str) -> Value {
    let mut lines = input.lines();
    let headers: Vec<String> = match lines.next() {
        Some(h) => parse_csv_line(h),
        None => return Value::Array(Vec::new()),
    };

    let rows: Vec<Value> = lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut map = IndexMap::new();
            let values = parse_csv_line(line);
            for (i, header) in headers.iter().enumerate() {
                let val = values.get(i).map(|v| v.as_str()).unwrap_or("");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_line_simple() {
        let fields = parse_csv_line("a,b,c");
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_csv_line_quoted_with_comma() {
        let fields = parse_csv_line(r#""Smith, John",42,true"#);
        assert_eq!(fields, vec!["Smith, John", "42", "true"]);
    }

    #[test]
    fn parse_csv_line_escaped_quotes() {
        let fields = parse_csv_line(r#""He said ""hello""",done"#);
        assert_eq!(fields, vec![r#"He said "hello""#, "done"]);
    }

    #[test]
    fn parse_csv_line_empty_fields() {
        let fields = parse_csv_line("a,,c");
        assert_eq!(fields, vec!["a", "", "c"]);
    }

    #[test]
    fn parse_csv_line_crlf() {
        let fields = parse_csv_line("a,b,c\r");
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_csv_line_quoted_empty() {
        let fields = parse_csv_line(r#""",hello"#);
        assert_eq!(fields, vec!["", "hello"]);
    }

    #[test]
    fn parse_csv_line_single_field() {
        let fields = parse_csv_line("hello");
        assert_eq!(fields, vec!["hello"]);
    }

    #[test]
    fn parse_csv_line_whitespace_trimmed() {
        let fields = parse_csv_line("  a  , b , c ");
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_csv_full_rfc4180() {
        let input = "name,age,city\n\"Smith, John\",42,\"New York\"\nJane,30,Boston";
        let result = parse_csv(input);
        if let Value::Array(rows) = result {
            assert_eq!(rows.len(), 2);
            if let Value::Object(row) = &rows[0] {
                assert_eq!(row.get("name"), Some(&Value::String("Smith, John".into())));
                assert_eq!(row.get("age"), Some(&Value::Int(42)));
                assert_eq!(row.get("city"), Some(&Value::String("New York".into())));
            } else {
                panic!("expected object row");
            }
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn stringify_csv_round_trip() {
        let input = "name,score\nAlice,100\nBob,95";
        let parsed = parse_csv(input);
        if let Value::Array(ref rows) = parsed {
            let output = stringify_csv(rows);
            assert!(output.contains("name,score"));
            assert!(output.contains("Alice,100"));
            assert!(output.contains("Bob,95"));
        }
    }

    #[test]
    fn stringify_csv_with_commas() {
        // Values containing commas should be quoted in output
        let mut row = IndexMap::new();
        row.insert("name".to_string(), Value::String("Smith, John".into()));
        row.insert("age".to_string(), Value::Int(42));
        let rows = vec![Value::Object(row)];
        let output = stringify_csv(&rows);
        assert!(output.contains("\"Smith, John\""), "got: {}", output);
    }
}
