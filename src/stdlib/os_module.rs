use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert(
        "hostname".to_string(),
        Value::BuiltIn("os.hostname".to_string()),
    );
    m.insert(
        "platform".to_string(),
        Value::BuiltIn("os.platform".to_string()),
    );
    m.insert("arch".to_string(), Value::BuiltIn("os.arch".to_string()));
    m.insert("pid".to_string(), Value::BuiltIn("os.pid".to_string()));
    m.insert("cpus".to_string(), Value::BuiltIn("os.cpus".to_string()));
    m.insert(
        "homedir".to_string(),
        Value::BuiltIn("os.homedir".to_string()),
    );
    Value::Object(m)
}

pub fn call(name: &str, _args: Vec<Value>) -> Result<Value, String> {
    match name {
        "os.hostname" => {
            let hostname = gethostname::gethostname();
            Ok(Value::String(hostname.to_string_lossy().into_owned()))
        }
        "os.platform" => {
            let platform = match std::env::consts::OS {
                "macos" => "macos",
                "linux" => "linux",
                "windows" => "windows",
                "freebsd" => "freebsd",
                "openbsd" => "openbsd",
                "netbsd" => "netbsd",
                other => other,
            };
            Ok(Value::String(platform.to_string()))
        }
        "os.arch" => Ok(Value::String(std::env::consts::ARCH.to_string())),
        "os.pid" => Ok(Value::Int(std::process::id() as i64)),
        "os.cpus" => {
            let cpus = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);
            Ok(Value::Int(cpus as i64))
        }
        "os.homedir" => {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_default();
            if home.is_empty() {
                Ok(Value::Null)
            } else {
                Ok(Value::String(home))
            }
        }
        _ => Err(format!("unknown os function: {}", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_returns_nonempty_string() {
        let result = call("os.hostname", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(!s.is_empty());
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn platform_is_known() {
        let result = call("os.platform", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(
                ["macos", "linux", "windows", "freebsd", "openbsd", "netbsd"].contains(&s.as_str())
                    || !s.is_empty()
            );
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn arch_is_nonempty() {
        let result = call("os.arch", vec![]).unwrap();
        if let Value::String(s) = result {
            assert!(!s.is_empty());
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn pid_is_positive() {
        let result = call("os.pid", vec![]).unwrap();
        if let Value::Int(n) = result {
            assert!(n > 0);
        } else {
            panic!("expected int");
        }
    }

    #[test]
    fn cpus_is_at_least_one() {
        let result = call("os.cpus", vec![]).unwrap();
        if let Value::Int(n) = result {
            assert!(n >= 1);
        } else {
            panic!("expected int");
        }
    }

    #[test]
    fn homedir_returns_string_or_null() {
        let result = call("os.homedir", vec![]).unwrap();
        match result {
            Value::String(s) => assert!(!s.is_empty()),
            Value::Null => {} // OK in CI environments
            _ => panic!("expected string or null"),
        }
    }
}
