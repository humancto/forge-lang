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
    m.insert(
        "valid".to_string(),
        Value::BuiltIn("json.valid".to_string()),
    );
    m.insert(
        "merge".to_string(),
        Value::BuiltIn("json.merge".to_string()),
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
        "json.valid" => match args.first() {
            Some(Value::String(s)) => Ok(Value::Bool(
                serde_json::from_str::<serde_json::Value>(s).is_ok(),
            )),
            _ => Ok(Value::Bool(false)),
        },
        "json.merge" => match (args.first(), args.get(1)) {
            (Some(Value::Object(a)), Some(Value::Object(b))) => Ok(Value::Object(deep_merge(a, b))),
            _ => Err("json.merge() requires two objects".to_string()),
        },
        _ => Err(format!("unknown json function: {}", name)),
    }
}

fn deep_merge(
    a: &indexmap::IndexMap<String, Value>,
    b: &indexmap::IndexMap<String, Value>,
) -> indexmap::IndexMap<String, Value> {
    let mut result = a.clone();
    for (key, b_val) in b {
        match (result.get(key), b_val) {
            (Some(Value::Object(a_inner)), Value::Object(b_inner)) => {
                result.insert(key.clone(), Value::Object(deep_merge(a_inner, b_inner)));
            }
            _ => {
                result.insert(key.clone(), b_val.clone());
            }
        }
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }

    #[test]
    fn module_has_all_functions() {
        if let Value::Object(m) = create_module() {
            for k in ["parse", "stringify", "pretty", "valid", "merge"] {
                assert!(m.contains_key(k), "missing {}", k);
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn parse_primitives() {
        assert_eq!(call("json.parse", vec![s("null")]).unwrap(), Value::Null);
        assert_eq!(
            call("json.parse", vec![s("true")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call("json.parse", vec![s("false")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(call("json.parse", vec![s("42")]).unwrap(), Value::Int(42));
        assert_eq!(
            call("json.parse", vec![s("3.14")]).unwrap(),
            Value::Float(3.14)
        );
        assert_eq!(call("json.parse", vec![s("\"hi\"")]).unwrap(), s("hi"));
    }

    #[test]
    fn parse_array() {
        let result = call("json.parse", vec![s("[1, 2, 3]")]).unwrap();
        assert_eq!(
            result,
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn parse_object() {
        let result = call("json.parse", vec![s("{\"a\": 1, \"b\": \"two\"}")]).unwrap();
        if let Value::Object(m) = result {
            assert_eq!(m.get("a"), Some(&Value::Int(1)));
            assert_eq!(m.get("b"), Some(&s("two")));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn parse_nested() {
        let result = call(
            "json.parse",
            vec![s("{\"users\": [{\"id\": 1}, {\"id\": 2}]}")],
        )
        .unwrap();
        if let Value::Object(m) = result {
            if let Some(Value::Array(users)) = m.get("users") {
                assert_eq!(users.len(), 2);
            } else {
                panic!("expected users array");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn parse_invalid_errors() {
        let result = call("json.parse", vec![s("not json")]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
    }

    #[test]
    fn stringify_primitives() {
        assert_eq!(
            call("json.stringify", vec![Value::Null]).unwrap(),
            s("null")
        );
        assert_eq!(
            call("json.stringify", vec![Value::Bool(true)]).unwrap(),
            s("true")
        );
        assert_eq!(
            call("json.stringify", vec![Value::Int(42)]).unwrap(),
            s("42")
        );
        assert_eq!(call("json.stringify", vec![s("hi")]).unwrap(), s("\"hi\""));
    }

    #[test]
    fn stringify_array() {
        let result = call(
            "json.stringify",
            vec![Value::Array(vec![Value::Int(1), Value::Int(2)])],
        )
        .unwrap();
        assert_eq!(result, s("[1, 2]"));
    }

    #[test]
    fn stringify_escapes_quotes() {
        let result = call("json.stringify", vec![s("she said \"hi\"")]).unwrap();
        assert_eq!(result, s("\"she said \\\"hi\\\"\""));
    }

    #[test]
    fn pretty_indents_object() {
        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), Value::Int(1));
        let result = call("json.pretty", vec![Value::Object(obj)]).unwrap();
        if let Value::String(out) = result {
            assert!(out.contains("\n"));
            assert!(out.contains("\"a\": 1"));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn pretty_empty_collections() {
        let result = call("json.pretty", vec![Value::Array(vec![])]).unwrap();
        assert_eq!(result, s("[]"));
        let result = call("json.pretty", vec![Value::Object(IndexMap::new())]).unwrap();
        assert_eq!(result, s("{}"));
    }

    #[test]
    fn valid_returns_bool() {
        assert_eq!(
            call("json.valid", vec![s("{\"a\": 1}")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call("json.valid", vec![s("not json")]).unwrap(),
            Value::Bool(false)
        );
        // Non-string input -> false (not error)
        assert_eq!(
            call("json.valid", vec![Value::Int(1)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn merge_shallow() {
        let mut a = IndexMap::new();
        a.insert("x".to_string(), Value::Int(1));
        let mut b = IndexMap::new();
        b.insert("y".to_string(), Value::Int(2));
        let result = call("json.merge", vec![Value::Object(a), Value::Object(b)]).unwrap();
        if let Value::Object(m) = result {
            assert_eq!(m.get("x"), Some(&Value::Int(1)));
            assert_eq!(m.get("y"), Some(&Value::Int(2)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn merge_deep() {
        // Nested objects should merge recursively rather than replace.
        let mut a_inner = IndexMap::new();
        a_inner.insert("p".to_string(), Value::Int(1));
        let mut a = IndexMap::new();
        a.insert("nested".to_string(), Value::Object(a_inner));

        let mut b_inner = IndexMap::new();
        b_inner.insert("q".to_string(), Value::Int(2));
        let mut b = IndexMap::new();
        b.insert("nested".to_string(), Value::Object(b_inner));

        let result = call("json.merge", vec![Value::Object(a), Value::Object(b)]).unwrap();
        if let Value::Object(outer) = result {
            if let Some(Value::Object(nested)) = outer.get("nested") {
                assert_eq!(nested.get("p"), Some(&Value::Int(1)));
                assert_eq!(nested.get("q"), Some(&Value::Int(2)));
            } else {
                panic!("expected nested object");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn merge_b_overrides_a_for_scalars() {
        let mut a = IndexMap::new();
        a.insert("x".to_string(), Value::Int(1));
        let mut b = IndexMap::new();
        b.insert("x".to_string(), Value::Int(99));
        let result = call("json.merge", vec![Value::Object(a), Value::Object(b)]).unwrap();
        if let Value::Object(m) = result {
            assert_eq!(m.get("x"), Some(&Value::Int(99)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn merge_requires_two_objects() {
        let result = call(
            "json.merge",
            vec![Value::Object(IndexMap::new()), Value::Int(1)],
        );
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_object() {
        let json_str = "{\"name\": \"Forge\", \"version\": 4}";
        let parsed = call("json.parse", vec![s(json_str)]).unwrap();
        let stringified = call("json.stringify", vec![parsed]).unwrap();
        // Re-parse and verify equality of the parsed forms.
        let reparsed = call("json.parse", vec![stringified]).unwrap();
        let original = call("json.parse", vec![s(json_str)]).unwrap();
        assert_eq!(reparsed, original);
    }

    #[test]
    fn unknown_function_errors() {
        let result = call("json.bogus", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown json function"));
    }
}
