//! Host-only stdlib modules, as seen by builds without the `host` feature
//! (the browser playground).
//!
//! These modules need an operating system (sockets, SQLite, processes) and
//! are not compiled there. So that `http.get(...)` still resolves and fails
//! with a clear runtime error — instead of "undefined variable" or a
//! compile error in the user's program — each one is registered (through
//! `builtins_registry::modules`) as a stand-in with the same members whose
//! every call returns [`crate::runtime::unavailable_message`].
//!
//! The member lists must match the real modules; the test below compares
//! them in `host` builds, so adding a member to e.g. `http` without listing
//! it here fails CI.

#![cfg_attr(feature = "host", allow(dead_code))]

use crate::interpreter::Value;
use indexmap::IndexMap;

/// `(module, members)` for every host-only module.
pub const HOST_ONLY: &[(&str, &[&str])] = &[
    (
        "db",
        &[
            "open",
            "query",
            "execute",
            "close",
            "last_insert_rowid",
            "begin",
            "commit",
            "rollback",
        ],
    ),
    (
        "http",
        &[
            "get", "post", "put", "delete", "patch", "head", "download", "crawl", "pretty",
        ],
    ),
    ("ws", &["connect", "send", "receive", "close"]),
    (
        "os",
        &["hostname", "platform", "arch", "pid", "cpus", "homedir"],
    ),
    (
        "pg",
        &[
            "connect", "query", "execute", "close", "begin", "commit", "rollback",
        ],
    ),
    (
        "mysql",
        &[
            "connect", "query", "execute", "close", "begin", "commit", "rollback",
        ],
    ),
];

fn members(module: &str) -> &'static [&'static str] {
    HOST_ONLY
        .iter()
        .find(|(name, _)| *name == module)
        .map(|(_, members)| *members)
        .unwrap_or(&[])
}

/// The stand-in module object: every member is a `BuiltIn` that fails.
pub fn create(module: &str) -> Value {
    let mut m = IndexMap::new();
    for member in members(module) {
        m.insert(
            member.to_string(),
            Value::BuiltIn(format!("{}.{}", module, member)),
        );
    }
    Value::Object(m)
}

/// Every stand-in member fails with the same message.
pub fn call(qualified: &str, _args: Vec<Value>) -> Result<Value, String> {
    Err(crate::runtime::unavailable_message(&format!(
        "`{}`",
        qualified
    )))
}

/// One `create_module`/`call` pair per stand-in, shaped like a real stdlib
/// module so `builtins_registry::module!` can register it.
macro_rules! stand_in {
    ($($module:ident => $name:literal),* $(,)?) => {
        $(
            pub mod $module {
                use crate::interpreter::Value;
                pub fn create_module() -> Value {
                    super::create($name)
                }
                pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
                    super::call(name, args)
                }
            }
        )*
    };
}

stand_in! {
    db => "db",
    http => "http",
    ws => "ws",
    os_module => "os",
    pg => "pg",
    mysql => "mysql",
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(module: Value) -> Vec<String> {
        match module {
            Value::Object(m) => m.keys().cloned().collect(),
            _ => panic!("module must be an object"),
        }
    }

    #[test]
    fn stand_ins_fail_with_a_clear_message() {
        let err = http::call("http.get", vec![]).unwrap_err();
        assert!(err.contains("`http.get`"), "{err}");
        assert!(
            err.contains("not available in the browser playground"),
            "{err}"
        );
        assert_eq!(keys(db::create_module())[0], "open");
    }

    /// The stand-ins must expose exactly the real modules' members.
    #[cfg(feature = "host")]
    #[test]
    fn member_lists_match_the_real_modules() {
        let mut real: Vec<(&str, Value)> = vec![
            ("db", crate::stdlib::db::create_module()),
            ("http", crate::stdlib::http::create_module()),
            ("ws", crate::stdlib::ws::create_module()),
            ("os", crate::stdlib::os_module::create_module()),
        ];
        #[cfg(feature = "postgres")]
        real.push(("pg", crate::stdlib::pg::create_module()));
        #[cfg(feature = "mysql")]
        real.push(("mysql", crate::stdlib::mysql::create_module()));
        for (name, module) in real {
            assert_eq!(
                keys(module),
                members(name)
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                "stdlib::unavailable::HOST_ONLY is out of date for `{}`",
                name
            );
        }
    }
}
