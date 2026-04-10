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

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }

    #[test]
    fn module_has_all_functions() {
        if let Value::Object(m) = create_module() {
            for k in ["test", "find", "find_all", "replace", "split"] {
                assert!(m.contains_key(k), "missing {}", k);
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_matches() {
        // Argument order is (text, pattern) — see CLAUDE.md.
        assert_eq!(
            call("regex.test", vec![s("hello world"), s(r"\w+")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call("regex.test", vec![s("12345"), s(r"^\d+$")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call("regex.test", vec![s("hello"), s(r"^\d+$")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn find_returns_first_match() {
        let result = call("regex.find", vec![s("the rain in Spain"), s(r"\w+ain")]).unwrap();
        assert_eq!(result, s("rain"));
    }

    #[test]
    fn find_no_match_returns_null() {
        let result = call("regex.find", vec![s("hello"), s(r"\d+")]).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn find_all_returns_array() {
        let result = call("regex.find_all", vec![s("a1 b2 c3"), s(r"\d")]).unwrap();
        assert_eq!(result, Value::Array(vec![s("1"), s("2"), s("3")]));
    }

    #[test]
    fn find_all_empty_when_no_matches() {
        let result = call("regex.find_all", vec![s("abc"), s(r"\d")]).unwrap();
        assert_eq!(result, Value::Array(vec![]));
    }

    #[test]
    fn replace_substitutes_all_matches() {
        let result = call("regex.replace", vec![s("hello world"), s(r"\w+"), s("X")]).unwrap();
        assert_eq!(result, s("X X"));
    }

    #[test]
    fn split_basic() {
        let result = call("regex.split", vec![s("a,b;c,d"), s(r"[,;]")]).unwrap();
        assert_eq!(result, Value::Array(vec![s("a"), s("b"), s("c"), s("d")]));
    }

    #[test]
    fn invalid_pattern_errors() {
        let result = call("regex.test", vec![s("anything"), s(r"(unclosed")]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid regex"));
    }

    #[test]
    fn wrong_arg_types_error() {
        assert!(call("regex.test", vec![s("text")]).is_err());
        assert!(call("regex.find", vec![Value::Int(1), s(r"\d")]).is_err());
        assert!(call("regex.replace", vec![s("a"), s("b")]).is_err());
    }

    #[test]
    fn unknown_function_errors() {
        let result = call("regex.bogus", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown regex function"));
    }
}
