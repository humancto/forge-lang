use crate::interpreter::Value;
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("join".to_string(), Value::BuiltIn("path.join".to_string()));
    m.insert(
        "resolve".to_string(),
        Value::BuiltIn("path.resolve".to_string()),
    );
    m.insert(
        "relative".to_string(),
        Value::BuiltIn("path.relative".to_string()),
    );
    m.insert(
        "is_absolute".to_string(),
        Value::BuiltIn("path.is_absolute".to_string()),
    );
    m.insert(
        "dirname".to_string(),
        Value::BuiltIn("path.dirname".to_string()),
    );
    m.insert(
        "basename".to_string(),
        Value::BuiltIn("path.basename".to_string()),
    );
    m.insert(
        "extname".to_string(),
        Value::BuiltIn("path.extname".to_string()),
    );
    m.insert(
        "separator".to_string(),
        Value::String(std::path::MAIN_SEPARATOR_STR.to_string()),
    );
    Value::Object(m)
}

fn require_string(args: &[Value], idx: usize, fn_name: &str) -> Result<String, String> {
    match args.get(idx) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(format!(
            "{}() argument {} must be a string, got {}",
            fn_name,
            idx + 1,
            other.type_name()
        )),
        None => Err(format!(
            "{}() requires at least {} argument(s)",
            fn_name,
            idx + 1
        )),
    }
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "path.join" => {
            if args.is_empty() {
                return Ok(Value::String(String::new()));
            }
            let first = require_string(&args, 0, "path.join")?;
            let mut buf = PathBuf::from(first);
            for (i, arg) in args.iter().enumerate().skip(1) {
                match arg {
                    Value::String(s) => buf.push(s),
                    _ => {
                        return Err(format!(
                            "path.join() argument {} must be a string, got {}",
                            i + 1,
                            arg.type_name()
                        ))
                    }
                }
            }
            Ok(Value::String(buf.to_string_lossy().into_owned()))
        }
        "path.resolve" => {
            let p = require_string(&args, 0, "path.resolve")?;
            match std::fs::canonicalize(&p) {
                Ok(abs) => Ok(Value::String(abs.to_string_lossy().into_owned())),
                Err(e) => Err(format!("path.resolve('{}') failed: {}", p, e)),
            }
        }
        "path.relative" => {
            let from = require_string(&args, 0, "path.relative")?;
            let to = require_string(&args, 1, "path.relative")?;

            let from_abs = std::fs::canonicalize(&from)
                .map_err(|e| format!("path.relative(): cannot resolve '{}': {}", from, e))?;
            let to_abs = std::fs::canonicalize(&to)
                .map_err(|e| format!("path.relative(): cannot resolve '{}': {}", to, e))?;

            let from_components: Vec<_> = from_abs.components().collect();
            let to_components: Vec<_> = to_abs.components().collect();

            let common_len = from_components
                .iter()
                .zip(to_components.iter())
                .take_while(|(a, b)| a == b)
                .count();

            let mut rel = PathBuf::new();
            for _ in common_len..from_components.len() {
                rel.push("..");
            }
            for component in &to_components[common_len..] {
                rel.push(component);
            }

            if rel.as_os_str().is_empty() {
                Ok(Value::String(".".to_string()))
            } else {
                Ok(Value::String(rel.to_string_lossy().into_owned()))
            }
        }
        "path.is_absolute" => {
            let p = require_string(&args, 0, "path.is_absolute")?;
            Ok(Value::Bool(Path::new(&p).is_absolute()))
        }
        "path.dirname" => {
            let p = require_string(&args, 0, "path.dirname")?;
            let result = Path::new(&p)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(Value::String(result))
        }
        "path.basename" => {
            let p = require_string(&args, 0, "path.basename")?;
            let result = Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(Value::String(result))
        }
        "path.extname" => {
            let p = require_string(&args, 0, "path.extname")?;
            let result = Path::new(&p)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            Ok(Value::String(result))
        }
        _ => Err(format!("unknown path function: {}", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_two_segments() {
        let result = call(
            "path.join",
            vec![Value::String("a".into()), Value::String("b".into())],
        )
        .unwrap();
        assert_eq!(result, Value::String("a/b".into()));
    }

    #[test]
    fn join_three_segments() {
        let result = call(
            "path.join",
            vec![
                Value::String("a".into()),
                Value::String("b".into()),
                Value::String("c.txt".into()),
            ],
        )
        .unwrap();
        assert_eq!(result, Value::String("a/b/c.txt".into()));
    }

    #[test]
    fn join_empty_args() {
        let result = call("path.join", vec![]).unwrap();
        assert_eq!(result, Value::String(String::new()));
    }

    #[test]
    fn join_rejects_non_string() {
        let result = call("path.join", vec![Value::String("a".into()), Value::Int(42)]);
        assert!(result.is_err());
    }

    #[test]
    fn is_absolute_detects_absolute() {
        let result = call("path.is_absolute", vec![Value::String("/usr/bin".into())]).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn is_absolute_detects_relative() {
        let result = call(
            "path.is_absolute",
            vec![Value::String("src/main.rs".into())],
        )
        .unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn dirname_of_file_path() {
        let result = call("path.dirname", vec![Value::String("/usr/bin/env".into())]).unwrap();
        assert_eq!(result, Value::String("/usr/bin".into()));
    }

    #[test]
    fn dirname_of_empty() {
        let result = call("path.dirname", vec![Value::String(String::new())]).unwrap();
        assert_eq!(result, Value::String(String::new()));
    }

    #[test]
    fn basename_of_file_path() {
        let result = call("path.basename", vec![Value::String("/usr/bin/env".into())]).unwrap();
        assert_eq!(result, Value::String("env".into()));
    }

    #[test]
    fn basename_of_empty() {
        let result = call("path.basename", vec![Value::String(String::new())]).unwrap();
        assert_eq!(result, Value::String(String::new()));
    }

    #[test]
    fn extname_of_file() {
        let result = call("path.extname", vec![Value::String("file.txt".into())]).unwrap();
        assert_eq!(result, Value::String(".txt".into()));
    }

    #[test]
    fn extname_no_extension() {
        let result = call("path.extname", vec![Value::String("Makefile".into())]).unwrap();
        assert_eq!(result, Value::String(String::new()));
    }

    #[test]
    fn resolve_existing_path() {
        let result = call("path.resolve", vec![Value::String(".".into())]).unwrap();
        if let Value::String(s) = result {
            assert!(Path::new(&s).is_absolute());
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn resolve_nonexistent_errors() {
        let result = call(
            "path.resolve",
            vec![Value::String("/nonexistent_path_xyz_123".into())],
        );
        assert!(result.is_err());
    }

    #[test]
    fn relative_same_dir() {
        let result = call(
            "path.relative",
            vec![Value::String(".".into()), Value::String(".".into())],
        )
        .unwrap();
        assert_eq!(result, Value::String(".".into()));
    }

    #[test]
    fn separator_is_correct() {
        let module = create_module();
        if let Value::Object(m) = module {
            assert_eq!(
                m.get("separator"),
                Some(&Value::String(std::path::MAIN_SEPARATOR_STR.to_string()))
            );
        } else {
            panic!("expected object");
        }
    }
}
