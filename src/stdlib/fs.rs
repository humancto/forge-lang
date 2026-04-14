use crate::interpreter::Value;
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

/// Reads `FORGE_FS_BASE` from the environment and, when set, confines every
/// filesystem operation to paths under that base directory. Symlinks are
/// resolved during canonicalisation, so a symlink that points outside the
/// base is rejected just like a literal `..` traversal would be.
///
/// When `FORGE_FS_BASE` is unset (or empty), this is a no-op and returns the
/// path unchanged so existing scripts keep working.
pub fn confine_path(path: &str) -> Result<PathBuf, String> {
    let base = std::env::var("FORGE_FS_BASE")
        .ok()
        .filter(|s| !s.is_empty());
    confine_path_with(path, base.as_deref())
}

/// Test-friendly variant of [`confine_path`]. `base` is the literal base
/// directory; pass `None` to disable confinement.
pub fn confine_path_with(path: &str, base: Option<&str>) -> Result<PathBuf, String> {
    let Some(base_str) = base else {
        return Ok(PathBuf::from(path));
    };

    let base = std::fs::canonicalize(base_str)
        .map_err(|e| format!("FORGE_FS_BASE '{}' is not accessible: {}", base_str, e))?;

    let target = PathBuf::from(path);

    // For paths that don't yet exist (e.g. write/mkdir destinations), we
    // canonicalise the parent directory and re-attach the final component.
    // This still resolves symlinks in every existing ancestor, so a symlinked
    // parent that points outside the base is caught.
    let canonical = if target.exists() {
        std::fs::canonicalize(&target)
            .map_err(|e| format!("cannot canonicalise '{}': {}", path, e))?
    } else {
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        let parent_canonical = if parent.as_os_str().is_empty() {
            std::env::current_dir().map_err(|e| format!("cwd error: {}", e))?
        } else {
            std::fs::canonicalize(parent)
                .map_err(|e| format!("cannot canonicalise parent of '{}': {}", path, e))?
        };
        match target.file_name() {
            Some(name) => parent_canonical.join(name),
            None => parent_canonical,
        }
    };

    if !canonical.starts_with(&base) {
        return Err(format!(
            "path '{}' escapes FORGE_FS_BASE '{}'",
            canonical.display(),
            base.display()
        ));
    }
    Ok(canonical)
}

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
            Some(Value::String(path)) => {
                let path = confine_path(path)?;
                std::fs::read_to_string(&path)
                    .map(Value::String)
                    .map_err(|e| format!("fs.read error: {}", e))
            }
            _ => Err("fs.read() requires a file path string".to_string()),
        },
        "fs.write" => match (args.first(), args.get(1)) {
            (Some(Value::String(path)), Some(Value::String(content))) => {
                let path = confine_path(path)?;
                std::fs::write(&path, content)
                    .map(|_| Value::Null)
                    .map_err(|e| format!("fs.write error: {}", e))
            }
            _ => Err("fs.write() requires (path, content) strings".to_string()),
        },
        "fs.append" => match (args.first(), args.get(1)) {
            (Some(Value::String(path)), Some(Value::String(content))) => {
                let path = confine_path(path)?;
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|e| format!("fs.append error: {}", e))?;
                file.write_all(content.as_bytes())
                    .map(|_| Value::Null)
                    .map_err(|e| format!("fs.append error: {}", e))
            }
            _ => Err("fs.append() requires (path, content) strings".to_string()),
        },
        "fs.exists" => match args.first() {
            Some(Value::String(path)) => {
                // exists() must not error when confinement rejects — return false
                // so user code can branch on it. Outside-base existence is none of
                // the script's business.
                match confine_path(path) {
                    Ok(p) => Ok(Value::Bool(p.exists())),
                    Err(_) => Ok(Value::Bool(false)),
                }
            }
            _ => Err("fs.exists() requires a file path string".to_string()),
        },
        "fs.list" => match args.first() {
            Some(Value::String(path)) => {
                let path = confine_path(path)?;
                let entries =
                    std::fs::read_dir(&path).map_err(|e| format!("fs.list error: {}", e))?;
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
                let path = confine_path(path)?;
                if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                        .map(|_| Value::Null)
                        .map_err(|e| format!("fs.remove error: {}", e))
                } else {
                    std::fs::remove_file(&path)
                        .map(|_| Value::Null)
                        .map_err(|e| format!("fs.remove error: {}", e))
                }
            }
            _ => Err("fs.remove() requires a file path string".to_string()),
        },
        "fs.mkdir" => match args.first() {
            Some(Value::String(path)) => {
                let path = confine_path(path)?;
                std::fs::create_dir_all(&path)
                    .map(|_| Value::Null)
                    .map_err(|e| format!("fs.mkdir error: {}", e))
            }
            _ => Err("fs.mkdir() requires a directory path string".to_string()),
        },
        "fs.copy" => match (args.first(), args.get(1)) {
            (Some(Value::String(src)), Some(Value::String(dst))) => {
                let src = confine_path(src)?;
                let dst = confine_path(dst)?;
                std::fs::copy(&src, &dst)
                    .map(|bytes| Value::Int(bytes as i64))
                    .map_err(|e| format!("fs.copy error: {}", e))
            }
            _ => Err("fs.copy() requires (source, destination) strings".to_string()),
        },
        "fs.rename" => match (args.first(), args.get(1)) {
            (Some(Value::String(src)), Some(Value::String(dst))) => {
                let src = confine_path(src)?;
                let dst = confine_path(dst)?;
                std::fs::rename(&src, &dst)
                    .map(|_| Value::Null)
                    .map_err(|e| format!("fs.rename error: {}", e))
            }
            _ => Err("fs.rename() requires (old_path, new_path) strings".to_string()),
        },
        "fs.size" => match args.first() {
            Some(Value::String(path)) => {
                let path = confine_path(path)?;
                std::fs::metadata(&path)
                    .map(|m| Value::Int(m.len() as i64))
                    .map_err(|e| format!("fs.size error: {}", e))
            }
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
                let path = confine_path(path)?;
                let content = std::fs::read_to_string(&path)
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
                let path = confine_path(path)?;
                let json_str = crate::stdlib::json_module::call("json.pretty", vec![val.clone()])?;
                if let Value::String(content) = json_str {
                    std::fs::write(&path, &content)
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
                let path = confine_path(path)?;
                let content =
                    std::fs::read_to_string(&path).map_err(|e| format!("fs.lines error: {}", e))?;
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
            Some(Value::String(path)) => match confine_path(path) {
                Ok(p) => Ok(Value::Bool(p.is_dir())),
                Err(_) => Ok(Value::Bool(false)),
            },
            _ => Err("fs.is_dir() requires a path string".to_string()),
        },
        "fs.is_file" => match args.first() {
            Some(Value::String(path)) => match confine_path(path) {
                Ok(p) => Ok(Value::Bool(p.is_file())),
                Err(_) => Ok(Value::Bool(false)),
            },
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
        if let Some(r) = v.as_obj() {
            if let Some(obj) = gc.get(r) {
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
            let path = confine_path(&path)?;
            std::fs::read_to_string(&path)
                .map(FsResult::StringVal)
                .map_err(|e| format!("fs.read error: {}", e))
        }
        "fs.write" => {
            let path = get_str(args.first().ok_or("fs.write() requires a path")?)
                .ok_or("string path required")?;
            let content = get_str(args.get(1).ok_or("fs.write() requires content")?)
                .ok_or("string content required")?;
            let path = confine_path(&path)?;
            std::fs::write(&path, &content)
                .map(|_| FsResult::NullVal)
                .map_err(|e| format!("fs.write error: {}", e))
        }
        "fs.append" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let content =
                get_str(args.get(1).ok_or("content required")?).ok_or("string required")?;
            let path = confine_path(&path)?;
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
            match confine_path(&path) {
                Ok(p) => Ok(FsResult::BoolVal(p.exists())),
                Err(_) => Ok(FsResult::BoolVal(false)),
            }
        }
        "fs.list" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let path = confine_path(&path)?;
            let entries = std::fs::read_dir(&path).map_err(|e| format!("{}", e))?;
            let items: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            Ok(FsResult::ArrayVal(items))
        }
        "fs.remove" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let path = confine_path(&path)?;
            if path.is_dir() {
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
            let path = confine_path(&path)?;
            std::fs::create_dir_all(&path)
                .map(|_| FsResult::NullVal)
                .map_err(|e| format!("{}", e))
        }
        "fs.lines" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            let path = confine_path(&path)?;
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
            match confine_path(&path) {
                Ok(p) => Ok(FsResult::BoolVal(p.is_dir())),
                Err(_) => Ok(FsResult::BoolVal(false)),
            }
        }
        "fs.is_file" => {
            let path = get_str(args.first().ok_or("path required")?).ok_or("string required")?;
            match confine_path(&path) {
                Ok(p) => Ok(FsResult::BoolVal(p.is_file())),
                Err(_) => Ok(FsResult::BoolVal(false)),
            }
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

    // ----- Confinement tests -----
    //
    // We use `confine_path_with` (not `confine_path`) so we don't race with
    // other tests on the `FORGE_FS_BASE` env var. Each test gets its own
    // tempdir and asserts behaviour by passing the base explicitly.

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("forge_confine_{}_{}_{}", tag, pid, nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn confine_path_no_base_is_passthrough() {
        // When base is None we expect the literal path back, no canonicalisation,
        // no error — back-compat with scripts that don't opt in.
        let p = confine_path_with("/totally/imaginary/path", None).unwrap();
        assert_eq!(p, PathBuf::from("/totally/imaginary/path"));
    }

    #[test]
    fn confine_path_inside_base_is_allowed() {
        let base = unique_temp_dir("inside");
        let inside = base.join("hello.txt");
        std::fs::write(&inside, "hi").unwrap();

        let resolved =
            confine_path_with(inside.to_str().unwrap(), Some(base.to_str().unwrap())).unwrap();
        assert!(resolved.starts_with(std::fs::canonicalize(&base).unwrap()));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn confine_path_traversal_escape_is_rejected() {
        let base = unique_temp_dir("escape");
        // ../ outside the base — must error.
        let escape = base.join("..").join("escape.txt");
        let err = confine_path_with(escape.to_str().unwrap(), Some(base.to_str().unwrap()))
            .expect_err("escape should be rejected");
        assert!(err.contains("escapes FORGE_FS_BASE"), "got: {}", err);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn confine_path_absolute_outside_is_rejected() {
        let base = unique_temp_dir("absolute");
        let err = confine_path_with("/etc/passwd", Some(base.to_str().unwrap()))
            .expect_err("absolute outside path should be rejected");
        assert!(err.contains("escapes FORGE_FS_BASE"), "got: {}", err);

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn confine_path_nonexistent_target_inside_base_is_allowed() {
        // mkdir/write often target paths that don't yet exist. As long as the
        // parent canonicalises into the base, we should accept.
        let base = unique_temp_dir("new");
        let new_file = base.join("about_to_create.txt");
        let resolved =
            confine_path_with(new_file.to_str().unwrap(), Some(base.to_str().unwrap())).unwrap();
        assert!(resolved.starts_with(std::fs::canonicalize(&base).unwrap()));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    #[cfg(unix)]
    fn confine_path_symlink_pointing_outside_is_rejected() {
        use std::os::unix::fs::symlink;

        let base = unique_temp_dir("symlink");
        let outside = unique_temp_dir("symlink_outside");
        let outside_file = outside.join("secret.txt");
        std::fs::write(&outside_file, "shh").unwrap();

        // Create a symlink inside the base that points outside.
        let link = base.join("link.txt");
        symlink(&outside_file, &link).unwrap();

        let err = confine_path_with(link.to_str().unwrap(), Some(base.to_str().unwrap()))
            .expect_err("symlink escaping base should be rejected");
        assert!(err.contains("escapes FORGE_FS_BASE"), "got: {}", err);

        std::fs::remove_dir_all(&base).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    // Note: end-to-end tests that set FORGE_FS_BASE on the process can't run
    // safely under cargo's parallel test runner — sibling fs tests like
    // test_fs_is_dir would observe the var and start failing midway. The
    // helper-level confine_path_with tests above already exercise the same
    // logic (confine_path is a one-line wrapper that reads the env var and
    // delegates), so we get the coverage without the race.
}
