use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("get".to_string(), Value::BuiltIn("env.get".to_string()));
    m.insert("set".to_string(), Value::BuiltIn("env.set".to_string()));
    m.insert("keys".to_string(), Value::BuiltIn("env.keys".to_string()));
    m.insert("has".to_string(), Value::BuiltIn("env.has".to_string()));
    m.insert("load".to_string(), Value::BuiltIn("env.load".to_string()));
    m.insert("all".to_string(), Value::BuiltIn("env.all".to_string()));
    Value::Object(m)
}

/// Environment variables that configure Forge's own safety checks or route
/// its traffic: the SSRF guard (`FORGE_HTTP_ALLOW_PRIVATE`), file
/// confinement (`FORGE_FS_BASE`), the AI endpoint and key, the recursion
/// limit, the shell, the package registry, logging, telemetry export
/// (`OTEL_*`) and HTTP proxies (`HTTPS_PROXY=http://user:secret@attacker`
/// would send every request, and the credentials, to a host outside the
/// `net` allowlist).
///
/// Every `FORGE_*` variable the runtime reads must be listed here; other
/// names (including application `FORGE_*` variables) stay settable.
const RESERVED_KEYS: &[&str] = &[
    "FORGE_AI_KEY",
    "FORGE_AI_MODEL",
    "FORGE_AI_URL",
    "FORGE_CACHE_TTL",
    "FORGE_FS_BASE",
    "FORGE_GC_STRESS",
    "FORGE_HTTP_ALLOW_PRIVATE",
    "FORGE_LIB_DIR",
    "FORGE_LOG",
    "FORGE_LOG_FORMAT",
    "FORGE_MAX_DEPTH",
    "FORGE_NATIVE_FORGE_BIN",
    "FORGE_REGISTRY_PATH",
    "FORGE_REGISTRY_URL",
    "FORGE_SHELL",
    "OPENAI_API_KEY",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "RUST_LOG",
];

fn is_reserved_key(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    k.starts_with("OTEL_") || RESERVED_KEYS.contains(&k.as_str())
}

/// A script may change its own environment, but under a bounded policy
/// (scoped or denied `fs.read`/`fs.write`/`net`) it may not rewrite the
/// variables the host uses to configure Forge's sandbox and network stack.
fn check_settable(key: &str, value: &str) -> Result<(), String> {
    // std::env::set_var panics on these; report an error instead.
    if key.is_empty() || key.contains('=') || key.contains('\0') || value.contains('\0') {
        return Err(format!(
            "env.set(): invalid variable name or value for '{}'",
            key.replace('\0', "\\0")
        ));
    }
    if is_reserved_key(key) {
        let caps = crate::permissions::current();
        let bounded = [
            crate::permissions::Capability::Read,
            crate::permissions::Capability::Write,
            crate::permissions::Capability::Net,
        ]
        .into_iter()
        .any(|cap| !caps.is_unrestricted(cap));
        if bounded {
            return Err(format!(
                "permission denied: env ({} is host configuration; a script with a \
                 restricted fs/net policy cannot change it)",
                key
            ));
        }
    }
    Ok(())
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    crate::permissions::require(crate::permissions::Capability::Env, name)?;
    match name {
        "env.get" => {
            let key = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err("env.get() requires a string key".to_string()),
            };
            let default = args.get(1).and_then(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            });
            match std::env::var(&key) {
                Ok(val) => Ok(Value::String(val)),
                Err(_) => match default {
                    Some(d) => Ok(Value::String(d)),
                    None => Ok(Value::Null),
                },
            }
        }
        "env.set" => match (args.first(), args.get(1)) {
            (Some(Value::String(key)), Some(Value::String(val))) => {
                check_settable(key, val)?;
                std::env::set_var(key, val);
                Ok(Value::Null)
            }
            _ => Err("env.set() requires (key, value) strings".to_string()),
        },
        "env.keys" => {
            let keys: Vec<Value> = std::env::vars().map(|(k, _)| Value::String(k)).collect();
            Ok(Value::Array(keys))
        }
        "env.has" => match args.first() {
            Some(Value::String(key)) => Ok(Value::Bool(std::env::var(key).is_ok())),
            _ => Err("env.has() requires a string key".to_string()),
        },
        "env.load" => {
            let path = match args.first() {
                Some(Value::String(s)) => s.clone(),
                _ => ".env".to_string(),
            };
            let checked = crate::stdlib::fs::confine_read(&path)?;
            // Same semantics as dotenvy::from_filename (existing variables
            // win), but every assignment goes through the env.set checks.
            let load = || -> Result<(), String> {
                let iter = dotenvy::from_filename_iter(&checked).map_err(|e| e.to_string())?;
                for item in iter {
                    let (key, val) = item.map_err(|e| e.to_string())?;
                    if std::env::var_os(&key).is_none() {
                        check_settable(&key, &val)?;
                        std::env::set_var(&key, &val);
                    }
                }
                Ok(())
            };
            match load() {
                Ok(_) => Ok(Value::Bool(true)),
                Err(e) if e.starts_with("permission denied") => Err(e),
                Err(e) => {
                    if path == ".env" {
                        // Silent fail for default .env — file may not exist
                        Ok(Value::Bool(false))
                    } else {
                        Err(format!("env.load() error: {}", e))
                    }
                }
            }
        }
        "env.all" => {
            let mut map = indexmap::IndexMap::new();
            for (k, v) in std::env::vars() {
                map.insert(k, Value::String(v));
            }
            Ok(Value::Object(map))
        }
        _ => Err(format!("unknown env function: {}", name)),
    }
}
