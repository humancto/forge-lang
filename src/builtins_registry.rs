//! The single registry of Forge builtins.
//!
//! Forge has two execution engines — the tree-walking interpreter and the
//! bytecode VM — with different value representations. Before this module
//! each engine kept its own hand-written list of global builtins and its own
//! table of stdlib module members, and the lists drifted (the VM lacked the
//! whole `npc`, `url`, `toml` and `ws` modules, `io.args_*`, ...).
//!
//! This module is now the source of truth for:
//!
//! * [`GLOBALS`] — every user-visible global builtin function, with its
//!   accepted [`Arity`]. Both engines register their globals from this
//!   table and enforce the arity through [`check_arity`], so the arity
//!   error a program sees does not depend on the engine.
//! * [`modules`] — every stdlib module, with the function that builds its
//!   member table and the function that implements its members. Both
//!   engines register the module objects from here and dispatch every
//!   `module.member(...)` call through [`call_module`]: the implementation
//!   is shared, the VM only converts values at the boundary (it may keep a
//!   native fast path for a member, but must fall back to [`call_module`]
//!   for everything else).
//!
//! Engine-specific implementations of global builtins still live in
//! `interpreter/builtins.rs` and `vm/builtins.rs`. The tests at the bottom
//! (and `registry_parity_tests` in `vm/`) fail when a registered name is not
//! defined or not dispatched on either engine, so adding a builtin to one
//! engine only is caught in CI.
//!
//! Compiler intrinsics (`__forge_*`) are not user-visible and are not listed.

use crate::interpreter::Value;

/// Number of arguments a builtin accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    /// Between `min` and `max` arguments (inclusive).
    Range(usize, usize),
    /// At least `min` arguments (variadic).
    AtLeast(usize),
}

impl Arity {
    pub fn accepts(&self, argc: usize) -> bool {
        match *self {
            Arity::Range(min, max) => argc >= min && argc <= max,
            Arity::AtLeast(min) => argc >= min,
        }
    }

    pub fn describe(&self) -> String {
        let plural = |n: usize| if n == 1 { "argument" } else { "arguments" };
        match *self {
            Arity::Range(min, max) if min == max => format!("{} {}", min, plural(min)),
            Arity::Range(min, max) => format!("{} to {} arguments", min, max),
            Arity::AtLeast(min) => format!("at least {} {}", min, plural(min)),
        }
    }
}

/// A user-visible global builtin function.
#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    pub name: &'static str,
    pub arity: Arity,
}

const fn b(name: &'static str, min: usize, max: usize) -> Builtin {
    Builtin {
        name,
        arity: Arity::Range(min, max),
    }
}

const fn variadic(name: &'static str, min: usize) -> Builtin {
    Builtin {
        name,
        arity: Arity::AtLeast(min),
    }
}

/// Every global builtin function, on both engines.
///
/// Arity bounds are what the implementations read; the per-builtin type
/// checks inside each engine still produce the specific error messages.
pub static GLOBALS: &[Builtin] = &[
    // Output
    variadic("print", 0),
    variadic("println", 0),
    variadic("say", 0),
    variadic("yell", 0),
    variadic("whisper", 0),
    // Types and conversion
    b("len", 1, 1),
    b("type", 1, 1),
    b("typeof", 1, 1),
    b("str", 1, 1),
    b("int", 1, 1),
    b("float", 1, 1),
    // Collections
    b("push", 2, 2),
    b("pop", 1, 1),
    b("keys", 1, 1),
    b("values", 1, 1),
    b("contains", 2, 2),
    b("has_key", 2, 2),
    b("get", 2, 3),
    b("pick", 2, 2),
    b("omit", 2, 2),
    variadic("merge", 0),
    b("find", 2, 2),
    b("flat_map", 2, 2),
    b("entries", 1, 1),
    b("from_entries", 1, 1),
    b("range", 1, 3),
    b("set", 0, 1),
    b("enumerate", 1, 1),
    b("map", 0, 2),
    b("filter", 2, 2),
    b("reduce", 3, 3),
    b("sort", 1, 2),
    b("reverse", 1, 1),
    b("sum", 1, 1),
    b("min_of", 1, 1),
    b("max_of", 1, 1),
    b("any", 2, 2),
    b("all", 2, 2),
    b("unique", 1, 1),
    b("zip", 2, 2),
    b("flatten", 1, 1),
    b("group_by", 2, 2),
    b("chunk", 2, 2),
    b("slice", 2, 3),
    b("sample", 1, 2),
    variadic("shuffle", 1),
    b("partition", 2, 2),
    variadic("diff", 2),
    b("sort_by", 2, 3),
    b("first", 1, 1),
    b("last", 1, 1),
    b("compact", 1, 1),
    b("take_n", 2, 2),
    b("skip", 2, 2),
    b("frequencies", 1, 1),
    b("for_each", 2, 2),
    // Strings
    b("split", 1, 2),
    b("join", 1, 2),
    b("replace", 3, 3),
    b("starts_with", 2, 2),
    b("ends_with", 2, 2),
    b("lines", 1, 1),
    b("substring", 2, 3),
    b("index_of", 2, 2),
    b("last_index_of", 2, 2),
    b("pad_start", 2, 3),
    b("pad_end", 2, 3),
    b("capitalize", 1, 1),
    b("title", 1, 1),
    b("upper", 1, 1),
    b("lower", 1, 1),
    b("trim", 1, 1),
    b("repeat_str", 2, 2),
    b("count", 2, 2),
    b("slugify", 1, 1),
    b("snake_case", 1, 1),
    b("camel_case", 1, 1),
    // Results and options
    b("Ok", 0, 1),
    b("ok", 0, 1),
    b("Err", 0, 1),
    b("err", 0, 1),
    b("is_ok", 1, 1),
    b("is_err", 1, 1),
    b("unwrap", 1, 1),
    b("unwrap_or", 2, 2),
    b("unwrap_err", 1, 1),
    b("Some", 1, 1),
    b("is_some", 1, 1),
    b("is_none", 1, 1),
    // Validation
    b("satisfies", 2, 2),
    b("assert", 1, 2),
    b("assert_eq", 2, 3),
    b("assert_ne", 2, 3),
    b("assert_throws", 1, 2),
    // GenZ debug kit
    variadic("sus", 1),
    b("bruh", 0, 1),
    b("bet", 1, 2),
    b("no_cap", 2, 3),
    b("ick", 1, 2),
    // Execution helpers
    b("cook", 1, 1),
    b("yolo", 1, 1),
    b("ghost", 1, 1),
    b("slay", 1, 2),
    // Shell and system
    variadic("run_command", 1),
    b("shell", 1, 1),
    b("sh", 1, 1),
    b("sh_lines", 1, 1),
    b("sh_json", 1, 1),
    b("sh_ok", 1, 1),
    b("which", 1, 1),
    b("cwd", 0, 0),
    b("cd", 1, 1),
    b("pipe_to", 2, 2),
    b("input", 0, 1),
    b("exit", 0, 1),
    b("uuid", 0, 0),
    b("wait", 1, 1),
    b("fetch", 1, 2),
    // Concurrency
    b("channel", 0, 1),
    b("send", 2, 2),
    b("receive", 1, 1),
    b("close", 1, 1),
    b("try_send", 2, 2),
    b("try_receive", 1, 1),
    b("select", 1, 2),
    b("await_all", 1, 1),
    b("await_timeout", 2, 2),
];

/// Look up a global builtin by name.
pub fn global(name: &str) -> Option<&'static Builtin> {
    GLOBALS.iter().find(|b| b.name == name)
}

/// Shared arity rule for global builtins. Names that are not global
/// builtins (module members, intrinsics) are accepted unchanged.
pub fn check_arity(name: &str, argc: usize) -> Result<(), String> {
    match global(name) {
        Some(builtin) if !builtin.arity.accepts(argc) => Err(format!(
            "{}() expects {}, got {}",
            name,
            builtin.arity.describe(),
            argc
        )),
        _ => Ok(()),
    }
}

/// A stdlib module: `name.member(...)`.
pub struct Module {
    pub name: &'static str,
    /// Builds the module object (members are `Value::BuiltIn("name.member")`
    /// plus constants such as `math.pi`).
    pub create: fn() -> Value,
    /// Implements every member. Receives the qualified name (`"npc.name"`).
    pub call: fn(&str, Vec<Value>) -> Result<Value, String>,
}

macro_rules! module {
    ($name:literal, $path:path) => {{
        use $path as m;
        Module {
            name: $name,
            create: m::create_module,
            call: m::call,
        }
    }};
}

/// A module that needs the host runtime: the real one with the `host`
/// feature, otherwise its stand-in from `stdlib::unavailable` (same members,
/// every call fails with a clear "not available" error).
macro_rules! host_module {
    ($name:literal, $host:ident) => {{
        #[cfg(feature = "host")]
        let m = module!($name, crate::stdlib::$host);
        #[cfg(not(feature = "host"))]
        let m = module!($name, crate::stdlib::unavailable::$host);
        m
    }};
}

/// Every stdlib module available in this build.
pub fn modules() -> &'static [Module] {
    use std::sync::OnceLock;
    static MODULES: OnceLock<Vec<Module>> = OnceLock::new();
    MODULES.get_or_init(|| {
        #[allow(unused_mut)]
        let mut all = vec![
            module!("math", crate::stdlib::math),
            module!("fs", crate::stdlib::fs),
            module!("io", crate::stdlib::io),
            module!("crypto", crate::stdlib::crypto),
            host_module!("db", db),
            module!("env", crate::stdlib::env),
            module!("json", crate::stdlib::json_module),
            module!("regex", crate::stdlib::regex_module),
            module!("log", crate::stdlib::log),
            module!("term", crate::stdlib::term),
            host_module!("http", http),
            module!("csv", crate::stdlib::csv),
            module!("time", crate::stdlib::time),
            module!("npc", crate::stdlib::npc),
            module!("url", crate::stdlib::url_module),
            module!("toml", crate::stdlib::toml_module),
            host_module!("ws", ws),
            module!("jwt", crate::stdlib::jwt),
            host_module!("os", os_module),
            module!("path", crate::stdlib::path_module),
            // Hidden: runtime checks inserted by `--strict` (typechecker::enforce).
            module!("__types", crate::stdlib::types_module),
        ];
        #[cfg(feature = "postgres")]
        all.push(module!("pg", crate::stdlib::pg));
        #[cfg(feature = "mysql")]
        all.push(module!("mysql", crate::stdlib::mysql));
        #[cfg(not(feature = "host"))]
        all.extend([
            module!("pg", crate::stdlib::unavailable::pg),
            module!("mysql", crate::stdlib::unavailable::mysql),
        ]);
        all
    })
}

/// The module that implements a qualified member name (`"npc.name"`).
pub fn module_for(qualified: &str) -> Option<&'static Module> {
    let (prefix, _) = qualified.split_once('.')?;
    modules().iter().find(|m| m.name == prefix)
}

/// Call a qualified module member through the shared implementation.
/// Returns `None` when no module owns the name.
pub fn call_module(qualified: &str, args: Vec<Value>) -> Option<Result<Value, String>> {
    module_for(qualified).map(|m| (m.call)(qualified, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn global_names_are_unique() {
        let mut seen = HashSet::new();
        for builtin in GLOBALS {
            assert!(seen.insert(builtin.name), "duplicate {}", builtin.name);
        }
    }

    #[test]
    fn module_names_are_unique_and_importable() {
        let mut seen = HashSet::new();
        for module in modules() {
            assert!(seen.insert(module.name), "duplicate module {}", module.name);
            assert!(
                crate::semantics::BUILTIN_MODULES.contains(&module.name),
                "module '{}' must be listed in semantics::BUILTIN_MODULES so `import \"{}\"` works",
                module.name,
                module.name
            );
        }
    }

    #[test]
    fn module_members_are_dispatched_by_their_module() {
        for module in modules() {
            let Value::Object(members) = (module.create)() else {
                panic!("module '{}' must be an object", module.name);
            };
            for (key, member) in members {
                if let Value::BuiltIn(qualified) = member {
                    assert_eq!(
                        module_for(&qualified).map(|m| m.name),
                        Some(module.name),
                        "{}.{} is registered as '{}'",
                        module.name,
                        key,
                        qualified
                    );
                }
            }
        }
    }

    #[test]
    fn arity_rule_and_message() {
        assert!(check_arity("len", 1).is_ok());
        assert_eq!(
            check_arity("len", 2).unwrap_err(),
            "len() expects 1 argument, got 2"
        );
        assert_eq!(
            check_arity("slice", 4).unwrap_err(),
            "slice() expects 2 to 3 arguments, got 4"
        );
        assert!(check_arity("say", 7).is_ok());
        assert!(check_arity("not_a_builtin", 99).is_ok());
    }

    /// Every registered global must be implemented by both engines'
    /// dispatch. The engines' dispatch tables are `match name { ... }`
    /// blocks, so this checks that each name appears as a match arm.
    #[test]
    fn every_global_is_dispatched_by_both_engines() {
        let interp = [
            include_str!("interpreter/builtins.rs"),
            include_str!("interpreter/mod.rs"),
        ]
        .concat();
        let vm = [
            include_str!("vm/builtins.rs"),
            include_str!("vm/machine.rs"),
        ]
        .concat();
        let is_arm = |src: &str, name: &str| {
            let quoted = format!("\"{}\"", name);
            src.match_indices(&quoted).any(|(i, _)| {
                let after = src[i + quoted.len()..].trim_start();
                let before = src[..i].trim_end();
                after.starts_with("=>") || after.starts_with('|') || before.ends_with('|')
            })
        };
        let mut missing = Vec::new();
        for builtin in GLOBALS {
            if !is_arm(&interp, builtin.name) {
                missing.push(format!("interpreter: {}", builtin.name));
            }
            if !is_arm(&vm, builtin.name) {
                missing.push(format!("vm: {}", builtin.name));
            }
        }
        assert!(missing.is_empty(), "builtins not dispatched: {:?}", missing);
    }
}
