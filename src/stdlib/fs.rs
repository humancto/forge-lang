use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("read".to_string(), Value::BuiltIn("fs.read".to_string()));
    m.insert("write".to_string(), Value::BuiltIn("fs.write".to_string()));
    m.insert(
        "append".to_string(),
        Value::BuiltIn("fs.append".to_string()),
    );
    m.insert(
        "exists".to_string(),
        Value::BuiltIn("fs.exists".to_string()),
    );
    m.insert("list".to_string(), Value::BuiltIn("fs.list".to_string()));
    m.insert(
        "remove".to_string(),
        Value::BuiltIn("fs.remove".to_string()),
    );
    m.insert("mkdir".to_string(), Value::BuiltIn("fs.mkdir".to_string()));
    m.insert("copy".to_string(), Value::BuiltIn("fs.copy".to_string()));
    m.insert(
        "rename".to_string(),
        Value::BuiltIn("fs.rename".to_string()),
    );
    m.insert("size".to_string(), Value::BuiltIn("fs.size".to_string()));
    m.insert("ext".to_string(), Value::BuiltIn("fs.ext".to_string()));
    m.insert(
        "read_json".to_string(),
        Value::BuiltIn("fs.read_json".to_string()),
    );
    m.insert(
        "write_json".to_string(),
        Value::BuiltIn("fs.write_json".to_string()),
    );
    m.insert("lines".to_string(), Value::BuiltIn("fs.lines".to_string()));
    m.insert(
        "dirname".to_string(),
        Value::BuiltIn("fs.dirname".to_string()),
    );
    m.insert(
        "basename".to_string(),
        Value::BuiltIn("fs.basename".to_string()),
    );
    m.insert(
        "join_path".to_string(),
        Value::BuiltIn("fs.join_path".to_string()),
    );
    m.insert(
        "is_dir".to_string(),
        Value::BuiltIn("fs.is_dir".to_string()),
    );
    m.insert(
        "is_file".to_string(),
        Value::BuiltIn("fs.is_file".to_string()),
    );
    m.insert(
        "temp_dir".to_string(),
        Value::BuiltIn("fs.temp_dir".to_string()),
    );
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "fs.read" => match args.first() {
            Some(Value::String(path)) => std::fs::read_to_string(path)
                .map(Value::String)
                .map_err(|e| format!("fs.read error: {}", e)),
            _ => Err("fs.read() requires a file path string".to_string()),
        },
        "fs.write" => match (args.first(), args.get(1)) {
            (Some(Value::String(path)), Some(Value::String(content))) => {
                std::fs::write(path, content)
                    .map(|_| Value::Null)
                    .map_err(|e| format!("fs.write error: {}", e))
            }
            _ => Err("fs.write() requires (path, content) strings".to_string()),
        },
        "fs.append" => match (args.first(), args.get(1)) {
            (Some(Value::String(path)), Some(Value::String(content))) => {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| format!("fs.append error: {}", e))?;
                file.write_all(content.as_bytes())
                    .map(|_| Value::Null)
                    .map_err(|e| format!("fs.append error: {}", e))
            }
            _ => Err("fs.append() requires (path, content) strings".to_string()),
        },
        "fs.exists" => match args.first() {
            Some(Value::String(path)) => Ok(Value::Bool(std::path::Path::new(path).exists())),
            _ => Err("fs.exists() requires a file path string".to_string()),
        },
        "fs.list" => match args.first() {
            Some(Value::String(path)) => {
                let entries =
                    std::fs::read_dir(path).map_err(|e| format!("fs.list error: {}", e))?;
                let mut items = Vec::new();
                for entry in entries.flatten() {
                    items.push(Value::String(
                        entry.file_name().to_string_lossy().to_string(),
                    ));
                }
                Ok(Value::Array(items))
            }
            _ => Err("fs.list() requires a directory path string".to_string()),
        },
        "fs.remove" => match args.first() {
            Some(Value::String(path)) => {
                let p = std::path::Path::new(path);
                if p.is_dir() {
                    std::fs::remove_dir_all(path)
                        .map(|_| Value::Null)
                        .map_err(|e| format!("fs.remove error: {}", e))
                } else {
                    std::fs::remove_file(path)
                        .map(|_| Value::Null)
                        .map_err(|e| format!("fs.remove error: {}", e))
                }
            }
            _ => Err("fs.remove() requires a file path string".to_string()),
        },
        "fs.mkdir" => match args.first() {
            Some(Value::String(path)) => std::fs::create_dir_all(path)
                .map(|_| Value::Null)
                .map_err(|e| format!("fs.mkdir error: {}", e)),
            _ => Err("fs.mkdir() requires a directory path string".to_string()),
        },
        "fs.copy" => match (args.first(), args.get(1)) {
            (Some(Value::String(src)), Some(Value::String(dst))) => std::fs::copy(src, dst)
                .map(|bytes| Value::Int(bytes as i64))
                .map_err(|e| format!("fs.copy error: {}", e)),
            _ => Err("fs.copy() requires (source, destination) strings".to_string()),
        },
        "fs.rename" => match (args.first(), args.get(1)) {
            (Some(Value::String(src)), Some(Value::String(dst))) => std::fs::rename(src, dst)
                .map(|_| Value::Null)
                .map_err(|e| format!("fs.rename error: {}", e)),
            _ => Err("fs.rename() requires (old_path, new_path) strings".to_string()),
        },
        "fs.size" => match args.first() {
            Some(Value::String(path)) => std::fs::metadata(path)
                .map(|m| Value::Int(m.len() as i64))
                .map_err(|e| format!("fs.size error: {}", e)),
            _ => Err("fs.size() requires a file path string".to_string()),
        },
        "fs.ext" => match args.first() {
            Some(Value::String(path)) => {
                let ext = std::path::Path::new(path)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Value::String(ext))
            }
            _ => Err("fs.ext() requires a file path string".to_string()),
        },
        "fs.read_json" => match args.first() {
            Some(Value::String(path)) => {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| format!("fs.read_json error: {}", e))?;
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(_) => {
                        crate::stdlib::json_module::call("json.parse", vec![Value::String(content)])
                    }
                    Err(e) => Err(format!("fs.read_json parse error: {}", e)),
                }
            }
            _ => Err("fs.read_json() requires a file path string".to_string()),
        },
        "fs.write_json" => match (args.first(), args.get(1)) {
            (Some(Value::String(path)), Some(val)) => {
                let json_str = crate::stdlib::json_module::call("json.pretty", vec![val.clone()])?;
                if let Value::String(content) = json_str {
                    std::fs::write(path, &content)
                        .map(|_| Value::Null)
                        .map_err(|e| format!("fs.write_json error: {}", e))
                } else {
                    Err("json serialization failed".to_string())
                }
            }
            _ => Err("fs.write_json() requires (path, value)".to_string()),
        },
        "fs.lines" => match args.first() {
            Some(Value::String(path)) => {
                let content =
                    std::fs::read_to_string(path).map_err(|e| format!("fs.lines error: {}", e))?;
                Ok(Value::Array(
                    content
                        .lines()
                        .map(|l| Value::String(l.to_string()))
                        .collect(),
                ))
            }
            _ => Err("fs.lines() requires a file path string".to_string()),
        },
        "fs.dirname" => match args.first() {
            Some(Value::String(path)) => {
                let p = std::path::Path::new(path);
                Ok(Value::String(
                    p.parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                ))
            }
            _ => Err("fs.dirname() requires a path string".to_string()),
        },
        "fs.basename" => match args.first() {
            Some(Value::String(path)) => {
                let p = std::path::Path::new(path);
                Ok(Value::String(
                    p.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                ))
            }
            _ => Err("fs.basename() requires a path string".to_string()),
        },
        "fs.join_path" => match (args.first(), args.get(1)) {
            (Some(Value::String(a)), Some(Value::String(b))) => {
                let p = std::path::Path::new(a).join(b);
                Ok(Value::String(p.to_string_lossy().to_string()))
            }
            _ => Err("fs.join_path() requires two path strings".to_string()),
        },
        "fs.is_dir" => match args.first() {
            Some(Value::String(path)) => Ok(Value::Bool(std::path::Path::new(path).is_dir())),
            _ => Err("fs.is_dir() requires a path string".to_string()),
        },
        "fs.is_file" => match args.first() {
            Some(Value::String(path)) => Ok(Value::Bool(std::path::Path::new(path).is_file())),
            _ => Err("fs.is_file() requires a path string".to_string()),
        },
        "fs.temp_dir" => Ok(Value::String(
            std::env::temp_dir().to_string_lossy().to_string(),
        )),
        _ => Err(format!("unknown fs function: {}", name)),
    }
}

pub enum FsResult {
    StringVal(String),
    BoolVal(bool),
    ArrayVal(Vec<String>),
    NullVal,
}

/// VM-compatible fs dispatch.
pub fn call_vm(
    name: &str,
    args: &[crate::vm::value::Value],
    gc: &crate::vm::gc::Gc,
) -> Result<FsResult, String> {
    let get_str = |v: &crate::vm::value::Value| -> Option<String> {
        if let crate::vm::value::Value::Obj(r) = v {
            if let Some(obj) = gc.get(*r) {
                if let crate::vm::value::ObjKind::String(s) = &obj.kind {
                    return Some(s.clone());
                }
            }
        }
        None
    };

    match name {
        "fs.read" => {
            let path = get_str(args.first().ok_or("fs.read() requires a path")?)
                .ok_or("fs.read() requires a string path")?;
            std::fs::read_to_string(&path)
                .map(FsResult::StringVal)
                .map_err(|e| format!("fs.read error: {}", e))
        }
        "fs.write" => {
            let path = get_str(args.first().ok_or("fs.write() requires a path")?)
                .ok_or("string path required")?;
            let content = get_str(args.get(1).ok_or("fs.write() requires content")?)
                .ok_or("string content required")?;
            std::fs::write(&path, &content)
                .map(|_| FsResult::NullVal)
                .map_err(|e| format!("fs.write error: {}", e))
        }
        "fs.append" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let content =
                get_str(args.get(1).ok_or("content required")?).ok_or("string required")?;
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| format!("{}", e))?;
            f.write_all(content.as_bytes())
                .map(|_| FsResult::NullVal)
                .map_err(|e| format!("{}", e))
        }
        "fs.exists" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            Ok(FsResult::BoolVal(std::path::Path::new(&path).exists()))
        }
        "fs.list" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let entries = std::fs::read_dir(&path).map_err(|e| format!("{}", e))?;
            let items: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            Ok(FsResult::ArrayVal(items))
        }
        "fs.remove" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let p = std::path::Path::new(&path);
            if p.is_dir() {
                std::fs::remove_dir_all(&path)
                    .map(|_| FsResult::NullVal)
                    .map_err(|e| format!("{}", e))
            } else {
                std::fs::remove_file(&path)
                    .map(|_| FsResult::NullVal)
                    .map_err(|e| format!("{}", e))
            }
        }
        "fs.mkdir" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            std::fs::create_dir_all(&path)
                .map(|_| FsResult::NullVal)
                .map_err(|e| format!("{}", e))
        }
        "fs.lines" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let content =
                std::fs::read_to_string(&path).map_err(|e| format!("fs.lines error: {}", e))?;
            Ok(FsResult::ArrayVal(
                content.lines().map(|l| l.to_string()).collect(),
            ))
        }
        "fs.dirname" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let p = std::path::Path::new(&path);
            Ok(FsResult::StringVal(
                p.parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ))
        }
        "fs.basename" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let p = std::path::Path::new(&path);
            Ok(FsResult::StringVal(
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ))
        }
        "fs.join_path" => {
            let a = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let b = get_str(args.get(1).ok_or("second path required")?).ok_or("string required")?;
            let p = std::path::Path::new(&a).join(&b);
            Ok(FsResult::StringVal(p.to_string_lossy().to_string()))
        }
        "fs.is_dir" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            Ok(FsResult::BoolVal(std::path::Path::new(&path).is_dir()))
        }
        "fs.is_file" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            Ok(FsResult::BoolVal(std::path::Path::new(&path).is_file()))
        }
        "fs.temp_dir" => Ok(FsResult::StringVal(
            std::env::temp_dir().to_string_lossy().to_string(),
        )),
        _ => Err(format!("unknown fs function: {}", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fs_lines() {
        let dir = std::env::temp_dir();
        let path = dir.join("forge_test_lines.txt");
        std::fs::write(&path, "line1\nline2\nline3").unwrap();
        let result = call(
            "fs.lines",
            vec![Value::String(path.to_string_lossy().to_string())],
        )
        .unwrap();
        if let Value::Array(lines) = result {
            assert_eq!(lines.len(), 3);
            assert_eq!(lines[0], Value::String("line1".to_string()));
        } else {
            panic!("expected array");
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_fs_dirname() {
        assert_eq!(
            call(
                "fs.dirname",
                vec![Value::String("/home/user/file.txt".to_string())]
            )
            .unwrap(),
            Value::String("/home/user".to_string())
        );
    }

    #[test]
    fn test_fs_basename() {
        assert_eq!(
            call(
                "fs.basename",
                vec![Value::String("/home/user/file.txt".to_string())]
            )
            .unwrap(),
            Value::String("file.txt".to_string())
        );
    }

    #[test]
    fn test_fs_join_path() {
        assert_eq!(
            call(
                "fs.join_path",
                vec![
                    Value::String("/home".to_string()),
                    Value::String("user".to_string())
                ]
            )
            .unwrap(),
            Value::String("/home/user".to_string())
        );
    }

    #[test]
    fn test_fs_is_dir() {
        assert_eq!(
            call("fs.is_dir", vec![Value::String("/tmp".to_string())]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "fs.is_dir",
                vec![Value::String("/nonexistent_path_xyz".to_string())]
            )
            .unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_fs_is_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("forge_test_is_file.txt");
        std::fs::write(&path, "test").unwrap();
        assert_eq!(
            call(
                "fs.is_file",
                vec![Value::String(path.to_string_lossy().to_string())]
            )
            .unwrap(),
            Value::Bool(true)
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_fs_temp_dir() {
        let result = call("fs.temp_dir", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(!s.is_empty());
        } else {
            panic!("expected string");
        }
    }
}
