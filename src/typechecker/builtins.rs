//! What the checker knows about the names every program starts with:
//! global builtins, stdlib modules and their members, and the prelude
//! values (`None`, ...).
//!
//! *Names and arities* come from `builtins_registry` (the table both engines
//! register from), so they cannot drift from the runtime. *Types* are
//! described here in annotation syntax. They are used to type results and to
//! give the parameters of callbacks a type (`map(xs, fn(x) { ... })`); they
//! are deliberately not used to reject arguments, because most builtins
//! accept several shapes (`len`, `reverse`, `contains`, ...) and the engines
//! already report a precise error for a wrong one.

use super::types::{FnTy, Ty};
use crate::parser::ast::TypeAnn;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

/// Typed signatures, in a compact notation:
/// `name<T>(param, optional_param?, ...variadic) -> Ret`.
const SIGNATURES: &[&str] = &[
    // Output
    "print(...Any) -> Null",
    "println(...Any) -> Null",
    "say(...Any) -> Null",
    "yell(...Any) -> Null",
    "whisper(...Any) -> Null",
    // Types and conversion
    "len(Any) -> Int",
    "type(Any) -> String",
    "typeof(Any) -> String",
    "str(Any) -> String",
    "int(Any) -> Int",
    "float(Any) -> Float",
    // Collections
    "push<T>([T], T) -> [T]",
    "pop<T>([T]) -> T",
    "contains(Any, Any) -> Bool",
    "has_key(Any, Any) -> Bool",
    "get(Any, Any, Any?) -> Any",
    "pick(Any, Any) -> Object",
    "omit(Any, Any) -> Object",
    "merge(...Any) -> Object",
    "find<T>([T], fn(T) -> Any) -> T",
    "flat_map<T>([T], fn(T) -> Any) -> [Any]",
    "entries(Any) -> [Any]",
    "from_entries(Any) -> Object",
    "range(Int, Int?, Int?) -> [Int]",
    "enumerate(Any) -> [Object]",
    "filter<T>([T], fn(T) -> Any) -> [T]",
    "sort<T>([T], fn(T, T) -> Any?) -> [T]",
    "min_of<T>([T]) -> T",
    "max_of<T>([T]) -> T",
    "any<T>([T], fn(T) -> Any) -> Bool",
    "all<T>([T], fn(T) -> Any) -> Bool",
    "unique<T>([T]) -> [T]",
    "zip(Any, Any) -> [[Any]]",
    "flatten(Any) -> [Any]",
    "group_by<T>([T], fn(T) -> Any) -> Object",
    "chunk<T>([T], Int) -> [[T]]",
    "partition<T>([T], fn(T) -> Any) -> [[T]]",
    "sort_by<T>([T], fn(T) -> Any, Any?) -> [T]",
    "first<T>([T]) -> T",
    "last<T>([T]) -> T",
    "compact<T>([T]) -> [T]",
    "take_n<T>([T], Int) -> [T]",
    "skip<T>([T], Int) -> [T]",
    "frequencies(Any) -> Object",
    "for_each<T>([T], fn(T) -> Any) -> Null",
    // Strings
    "split(String, String?) -> [String]",
    "join(Any, String?) -> String",
    "replace(String, String, String) -> String",
    "starts_with(String, String) -> Bool",
    "ends_with(String, String) -> Bool",
    "lines(String) -> [String]",
    "substring(String, Int, Int?) -> String",
    "index_of(Any, Any) -> Int",
    "last_index_of(Any, Any) -> Int",
    "pad_start(String, Int, String?) -> String",
    "pad_end(String, Int, String?) -> String",
    "capitalize(String) -> String",
    "title(String) -> String",
    "upper(String) -> String",
    "lower(String) -> String",
    "trim(String) -> String",
    "repeat_str(String, Int) -> String",
    "count(Any, Any) -> Int",
    "slugify(String) -> String",
    "snake_case(String) -> String",
    "camel_case(String) -> String",
    // Results and options
    "is_ok(Any) -> Bool",
    "is_err(Any) -> Bool",
    "is_some(Any) -> Bool",
    "is_none(Any) -> Bool",
    "Some<T>(T) -> ?T",
    // Validation
    "satisfies(Any, Any) -> Bool",
    "assert(Any, Any?) -> Null",
    "assert_eq(Any, Any, Any?) -> Null",
    "assert_ne(Any, Any, Any?) -> Null",
    "assert_throws(Any, Any?) -> Null",
    "bruh(Any?) -> Never",
    "bet(Any, Any?) -> Null",
    "no_cap(Any, Any, Any?) -> Null",
    "ick(Any, Any?) -> Null",
    // Shell and system
    "sh(String) -> String",
    "sh_lines(String) -> [String]",
    "sh_ok(String) -> Bool",
    "cwd() -> String",
    "input(Any?) -> String",
    "exit(Any?) -> Never",
    "uuid() -> String",
    "wait(Any) -> Null",
    "fetch(Any, Any?) -> Object",
];

/// Members of stdlib modules with a known result type
/// (`module.member(params) -> Ret`). Members not listed return `Any`.
const MODULE_SIGNATURES: &[&str] = &[
    "math.sqrt(Any) -> Float",
    "math.pow(Any, Any) -> Any",
    "math.sin(Any) -> Float",
    "math.cos(Any) -> Float",
    "math.tan(Any) -> Float",
    "math.log(Any) -> Float",
    "math.random() -> Float",
    "math.random_int(Int, Int) -> Int",
    "math.floor(Any) -> Int",
    "math.ceil(Any) -> Int",
    "math.round(Any) -> Int",
    "fs.read(String) -> String",
    "fs.exists(String) -> Bool",
    "fs.is_dir(String) -> Bool",
    "fs.is_file(String) -> Bool",
    "fs.lines(String) -> [String]",
    "fs.list(String) -> [String]",
    "fs.ext(String) -> String",
    "fs.basename(String) -> String",
    "fs.dirname(String) -> String",
    "fs.join_path(...Any) -> String",
    "fs.temp_dir() -> String",
    "json.stringify(Any) -> String",
    "json.pretty(Any) -> String",
    "crypto.sha256(Any) -> String",
    "crypto.md5(Any) -> String",
    "crypto.base64_encode(Any) -> String",
    "crypto.hex_encode(Any) -> String",
    "os.hostname() -> String",
    "os.platform() -> String",
    "os.arch() -> String",
    "os.homedir() -> String",
    "path.join(...Any) -> String",
    "path.basename(String) -> String",
    "path.dirname(String) -> String",
    "path.extname(String) -> String",
    "path.is_absolute(String) -> Bool",
    "env.has(String) -> Bool",
    "regex.test(String, String) -> Bool",
    "regex.replace(String, String, String) -> String",
    "regex.split(String, String) -> [String]",
    "regex.find_all(String, String) -> [String]",
];

/// Prelude values that are neither builtins nor modules.
const PRELUDE_VALUES: &[&str] = &["None", "null", "true", "false"];

/// Names both engines bind at startup that are not listed in the builtin
/// registry: constructor aliases, the `time` module's call form, ... The
/// test `known_globals_cover_both_engines` keeps this in sync.
const EXTRA_GLOBALS: &[&str] = &["None", "null", "Ok", "Err", "Some", "time"];

pub struct Builtins {
    pub functions: HashMap<String, FnTy>,
    pub modules: HashMap<String, ModuleInfo>,
}

pub struct ModuleInfo {
    pub members: HashSet<String>,
    /// Member name → type (functions and constants with a known type).
    pub typed: HashMap<String, Ty>,
}

/// Parse `name<T, U>(P, Q?, ...R) -> Ret` into a name and function type.
fn parse_signature(sig: &str) -> Option<(String, FnTy)> {
    let open = sig.find('(')?;
    let close = sig.rfind(')')?;
    let head = sig[..open].trim();
    let (name, type_params) = match head.find('<') {
        Some(lt) => (
            head[..lt].trim().to_string(),
            head[lt + 1..head.rfind('>')?]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>(),
        ),
        None => (head.to_string(), Vec::new()),
    };
    let ret_src = sig[close + 1..].trim().strip_prefix("->")?.trim();
    let mut params = Vec::new();
    let mut required = 0;
    let mut variadic = false;
    for raw in split_top_level(&sig[open + 1..close]) {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let (raw, optional) = match raw.strip_suffix('?') {
            Some(r) => (r.trim(), true),
            None => (raw, false),
        };
        let (raw, is_variadic) = match raw.strip_prefix("...") {
            Some(r) => (r, true),
            None => (raw, false),
        };
        params.push(lower_signature_type(raw, &type_params)?);
        if is_variadic {
            variadic = true;
        } else if !optional {
            required = params.len();
        }
    }
    let ret = lower_signature_type(ret_src, &type_params)?;
    Some((
        name,
        FnTy {
            type_params,
            params,
            required,
            variadic,
            ret: Box::new(ret),
        },
    ))
}

/// Split on commas that are not nested in brackets.
fn split_top_level(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let mut prev = ' ';
    for (i, ch) in s.char_indices() {
        let arrow = prev == '-' && ch == '>';
        prev = ch;
        match ch {
            '(' | '[' | '<' => depth += 1,
            '>' if arrow => {}
            ')' | ']' | '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

fn lower_signature_type(src: &str, type_params: &[String]) -> Option<Ty> {
    let ann = crate::parser::parse_type_annotation(src).ok()?;
    Some(lower_static(&ann, type_params))
}

/// Lower an annotation that only uses builtin types and the given type
/// parameters (signatures in this file; also the fallback for annotations
/// with no user types in scope).
pub fn lower_static(ann: &TypeAnn, type_params: &[String]) -> Ty {
    match ann {
        TypeAnn::Simple(name) => {
            if type_params.iter().any(|p| p == name) {
                return Ty::Param(name.clone());
            }
            builtin_type(name).unwrap_or(Ty::Any)
        }
        TypeAnn::Array(inner) => Ty::array(lower_static(inner, type_params)),
        TypeAnn::Optional(inner) => Ty::option(lower_static(inner, type_params)),
        TypeAnn::Tuple(items) => {
            Ty::Tuple(items.iter().map(|t| lower_static(t, type_params)).collect())
        }
        TypeAnn::Function(params, ret) => Ty::func(
            params
                .iter()
                .map(|t| lower_static(t, type_params))
                .collect(),
            lower_static(ret, type_params),
        ),
        TypeAnn::Generic(name, args) => {
            let args: Vec<Ty> = args.iter().map(|t| lower_static(t, type_params)).collect();
            builtin_generic(name, &args).unwrap_or(Ty::Any)
        }
    }
}

/// The builtin type a simple type name denotes (case-insensitive aliases
/// included), or `None` for a user type name.
pub fn builtin_type(name: &str) -> Option<Ty> {
    Some(match name.to_lowercase().as_str() {
        "int" | "i64" | "integer" => Ty::Int,
        "float" | "f64" | "number" => Ty::Float,
        "string" | "str" => Ty::String,
        "bool" | "boolean" => Ty::Bool,
        "null" | "void" | "nil" => Ty::Null,
        "any" | "json" | "value" => Ty::Any,
        "object" => Ty::Object,
        "array" | "list" => Ty::array(Ty::Any),
        "map" => Ty::Map(Box::new(Ty::Any), Box::new(Ty::Any)),
        "set" => Ty::Set(Box::new(Ty::Any)),
        "tuple" => Ty::Any,
        "fn" | "function" | "func" => Ty::Any,
        "option" => Ty::option(Ty::Any),
        "result" => Ty::result(Ty::Any, Ty::Any),
        "never" => Ty::Never,
        _ => return None,
    })
}

/// Builtin generic types (`Option<T>`, `Result<T, E>`, `Map<K, V>`, ...).
pub fn builtin_generic(name: &str, args: &[Ty]) -> Option<Ty> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Ty::Any);
    Some(match (name.to_lowercase().as_str(), args.len()) {
        ("option", 1) => Ty::option(arg(0)),
        ("result", 2) => Ty::result(arg(0), arg(1)),
        ("result", 1) => Ty::result(arg(0), Ty::Any),
        ("array" | "list", 1) => Ty::array(arg(0)),
        ("set", 1) => Ty::Set(Box::new(arg(0))),
        ("map", 2) => Ty::Map(Box::new(arg(0)), Box::new(arg(1))),
        _ => return None,
    })
}

pub static BUILTINS: LazyLock<Builtins> = LazyLock::new(|| {
    let mut functions = HashMap::new();
    for sig in SIGNATURES {
        match parse_signature(sig) {
            Some((name, ty)) => {
                functions.insert(name, ty);
            }
            None => debug_assert!(false, "BUG: malformed builtin signature {}", sig),
        }
    }

    let mut typed_members: BTreeMap<String, HashMap<String, Ty>> = BTreeMap::new();
    for sig in MODULE_SIGNATURES {
        match parse_signature(sig) {
            Some((qualified, ty)) => {
                if let Some((module, member)) = qualified.split_once('.') {
                    typed_members
                        .entry(module.to_string())
                        .or_default()
                        .insert(member.to_string(), Ty::Fn(ty));
                }
            }
            None => debug_assert!(false, "BUG: malformed module signature {}", sig),
        }
    }

    let mut modules = HashMap::new();
    for module in crate::builtins_registry::modules() {
        let mut members = HashSet::new();
        let mut typed = typed_members.remove(module.name).unwrap_or_default();
        if let crate::interpreter::Value::Object(map) = (module.create)() {
            for (key, value) in map {
                // Constants (`math.pi`) get their value's type.
                let constant = match value {
                    crate::interpreter::Value::Int(_) => Some(Ty::Int),
                    crate::interpreter::Value::Float(_) => Some(Ty::Float),
                    crate::interpreter::Value::String(_) => Some(Ty::String),
                    crate::interpreter::Value::Bool(_) => Some(Ty::Bool),
                    _ => None,
                };
                if let Some(ty) = constant {
                    typed.entry(key.clone()).or_insert(ty);
                }
                members.insert(key);
            }
        }
        modules.insert(module.name.to_string(), ModuleInfo { members, typed });
    }
    Builtins { functions, modules }
});

/// Is `name` bound when a program starts (builtin function, module or
/// prelude value)?
pub fn is_global(name: &str) -> bool {
    crate::builtins_registry::global(name).is_some()
        || BUILTINS.modules.contains_key(name)
        || PRELUDE_VALUES.contains(&name)
        || EXTRA_GLOBALS.contains(&name)
        || crate::semantics::BUILTIN_MODULES.contains(&name)
}

/// Every global name, for "did you mean" suggestions.
pub fn global_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = crate::builtins_registry::GLOBALS
        .iter()
        .map(|b| b.name)
        .collect();
    names.extend(crate::builtins_registry::modules().iter().map(|m| m.name));
    names.extend(EXTRA_GLOBALS.iter().copied());
    names.sort_unstable();
    names.dedup();
    names
}

/// A stdlib module available in this build.
pub fn module(name: &str) -> Option<&'static ModuleInfo> {
    BUILTINS.modules.get(name)
}

/// Typed signature of a global builtin, if one is described.
pub fn function(name: &str) -> Option<&'static FnTy> {
    BUILTINS.functions.get(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_signature_parses_and_names_a_registered_builtin() {
        for sig in SIGNATURES {
            let (name, ty) = parse_signature(sig).unwrap_or_else(|| panic!("bad sig {}", sig));
            let registered = crate::builtins_registry::global(&name)
                .unwrap_or_else(|| panic!("{} is not a registered builtin", name));
            // The described arity must agree with the registry's.
            assert!(
                registered.arity.accepts(ty.required),
                "{}: registry rejects {} required args",
                name,
                ty.required
            );
            if !ty.variadic {
                assert!(
                    registered.arity.accepts(ty.params.len()),
                    "{}: registry rejects {} args",
                    name,
                    ty.params.len()
                );
            }
        }
    }

    #[test]
    fn every_module_signature_names_a_real_member() {
        for sig in MODULE_SIGNATURES {
            let (qualified, _) = parse_signature(sig).expect("module sig parses");
            let (module_name, member) = qualified.split_once('.').expect("qualified");
            if let Some(m) = module(module_name) {
                assert!(
                    m.members.contains(member),
                    "{}.{} is not a member",
                    module_name,
                    member
                );
            }
        }
    }

    /// Every name the interpreter binds at startup must be known to the
    /// checker, or programs using it would get "unknown name" diagnostics.
    /// (The VM registers its globals from the same registry; the registry
    /// tests keep the two engines' tables equal.)
    #[test]
    fn known_globals_cover_both_engines() {
        let interp = crate::interpreter::Interpreter::new();
        let missing: Vec<String> = interp
            .env
            .all_names()
            .into_iter()
            .filter(|n| !n.starts_with("__") && !is_global(n))
            .collect();
        assert!(
            missing.is_empty(),
            "checker does not know globals: {:?}",
            missing
        );
    }

    #[test]
    fn signature_notation() {
        let (name, ty) = parse_signature("sort<T>([T], fn(T, T) -> Any?) -> [T]").unwrap();
        assert_eq!(name, "sort");
        assert_eq!(ty.required, 1);
        assert_eq!(ty.params.len(), 2);
        assert_eq!(*ty.ret, Ty::array(Ty::Param("T".into())));
        let (_, ty) = parse_signature("print(...Any) -> Null").unwrap();
        assert!(ty.variadic);
        assert_eq!(ty.required, 0);
    }

    #[test]
    fn module_constants_are_typed() {
        let math = module("math").expect("math module");
        assert_eq!(math.typed.get("pi"), Some(&Ty::Float));
        assert!(math.members.contains("sqrt"));
    }
}
