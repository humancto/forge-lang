//! The `regex` module. Every entry point compiles its pattern through
//! [`compiled`], which keeps recently used patterns in a bounded cache, so a
//! loop calling `regex.test(line, "^[0-9]+$")` compiles the pattern once
//! instead of once per call (~40 µs each).

use crate::interpreter::Value;
use indexmap::IndexMap;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Most patterns one cache keeps; beyond that the least recently used one
/// is evicted, so a script that builds a new pattern per call cannot grow
/// it without bound.
pub const CACHE_CAPACITY: usize = 256;

/// Longer patterns are compiled for the call and not cached: a few huge
/// generated patterns could otherwise pin a lot of memory.
pub const MAX_CACHED_PATTERN_LEN: usize = 4096;

/// A bounded least-recently-used cache of compiled patterns, keyed by the
/// pattern text (inline flags such as `(?i)` are part of it). Thread-safe:
/// spawned tasks and request handlers share it, and `Regex` is safe to use
/// from several threads behind an `Arc`.
#[derive(Default)]
pub struct RegexCache {
    inner: Mutex<CacheState>,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<String, (Arc<Regex>, u64)>,
    tick: u64,
}

impl std::fmt::Debug for RegexCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegexCache")
            .field("len", &self.len())
            .finish()
    }
}

impl RegexCache {
    /// The compiled `pattern`, from the cache when it is there.
    pub fn get_or_compile(&self, pattern: &str) -> Result<Arc<Regex>, String> {
        if pattern.len() > MAX_CACHED_PATTERN_LEN {
            return compile(pattern).map(Arc::new);
        }
        {
            let mut state = self.lock();
            state.tick += 1;
            let tick = state.tick;
            if let Some((re, used)) = state.entries.get_mut(pattern) {
                *used = tick;
                return Ok(re.clone());
            }
        }
        // Compile without holding the lock (other tasks keep matching); a
        // concurrent compile of the same pattern just replaces the entry.
        // Invalid patterns are not cached: the error is rebuilt each time.
        let re = Arc::new(compile(pattern)?);
        let evicted = {
            let mut state = self.lock();
            state.tick += 1;
            let tick = state.tick;
            let mut evicted = None;
            if state.entries.len() >= CACHE_CAPACITY && !state.entries.contains_key(pattern) {
                // O(capacity), only when a new pattern is compiled anyway.
                let oldest = state
                    .entries
                    .iter()
                    .min_by_key(|(_, (_, used))| *used)
                    .map(|(k, _)| k.clone());
                if let Some(k) = oldest {
                    evicted = state.entries.remove(&k);
                }
            }
            state
                .entries
                .insert(pattern.to_string(), (re.clone(), tick));
            evicted
        };
        // Free the evicted program outside the lock.
        drop(evicted);
        Ok(re)
    }

    /// Number of cached patterns.
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// Whether no pattern is cached.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CacheState> {
        // Every update is a single insert or remove, so a panic elsewhere
        // cannot leave the map inconsistent: ignore poisoning.
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn compile(pattern: &str) -> Result<Regex, String> {
    Regex::new(pattern).map_err(|e| format!("invalid regex: {}", e))
}

/// The cache of runs without a resource budget (the plain CLI).
fn process_cache() -> &'static RegexCache {
    static CACHE: OnceLock<RegexCache> = OnceLock::new();
    CACHE.get_or_init(RegexCache::default)
}

/// Compile `pattern` through the current run's cache. A run with a
/// resource budget (sandbox, MCP, `--max-memory`, ...) uses the cache its
/// [`Budget`](crate::runtime::limits::Budget) owns: its compiled patterns
/// are charged to it and freed with it, and no run can evict or keep alive
/// another's. Runs without a budget share one process-wide cache.
pub fn compiled(pattern: &str) -> Result<Arc<Regex>, String> {
    match crate::runtime::limits::current() {
        Some(budget) => budget.regex_cache().get_or_compile(pattern),
        None => process_cache().get_or_compile(pattern),
    }
}

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
    m.insert(
        "captures".to_string(),
        Value::BuiltIn("regex.captures".to_string()),
    );
    m.insert(
        "captures_all".to_string(),
        Value::BuiltIn("regex.captures_all".to_string()),
    );
    Value::Object(m)
}

/// The groups of one match: group 0 is the whole match, then every group in
/// the pattern from left to right (named ones included); a group that did
/// not take part in the match is `null`.
fn groups(caps: &regex::Captures) -> Value {
    Value::Array(
        caps.iter()
            .map(|g| match g {
                Some(m) => Value::String(m.as_str().to_string()),
                None => Value::Null,
            })
            .collect(),
    )
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "regex.test" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = compiled(pattern)?;
                Ok(Value::Bool(re.is_match(text)))
            }
            _ => Err("regex.test() requires (text, pattern) strings".to_string()),
        },
        "regex.find" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = compiled(pattern)?;
                match re.find(text) {
                    Some(m) => Ok(Value::String(m.as_str().to_string())),
                    None => Ok(Value::Null),
                }
            }
            _ => Err("regex.find() requires (text, pattern) strings".to_string()),
        },
        "regex.find_all" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = compiled(pattern)?;
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
                let re = compiled(pattern)?;
                Ok(Value::String(
                    re.replace_all(text, replacement.as_str()).to_string(),
                ))
            }
            _ => Err("regex.replace() requires (text, pattern, replacement) strings".to_string()),
        },
        "regex.split" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = compiled(pattern)?;
                let parts: Vec<Value> = re
                    .split(text)
                    .map(|s| Value::String(s.to_string()))
                    .collect();
                Ok(Value::Array(parts))
            }
            _ => Err("regex.split() requires (text, pattern) strings".to_string()),
        },
        "regex.captures" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = compiled(pattern)?;
                Ok(re.captures(text).map_or(Value::Null, |c| groups(&c)))
            }
            _ => Err("regex.captures() requires (text, pattern) strings".to_string()),
        },
        "regex.captures_all" => match (args.first(), args.get(1)) {
            (Some(Value::String(text)), Some(Value::String(pattern))) => {
                let re = compiled(pattern)?;
                Ok(Value::Array(
                    re.captures_iter(text).map(|c| groups(&c)).collect(),
                ))
            }
            _ => Err("regex.captures_all() requires (text, pattern) strings".to_string()),
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
            for k in [
                "test",
                "find",
                "find_all",
                "replace",
                "split",
                "captures",
                "captures_all",
            ] {
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
    fn replace_expands_group_references() {
        let date = r"(?P<y>\d{4})-(\d{2})-(\d{2})";
        let result = call(
            "regex.replace",
            vec![s("on 2024-01-15"), s(date), s("${y}/$2/${3}x $$ $0")],
        )
        .unwrap();
        assert_eq!(result, s("on 2024/01/15x $ 2024-01-15"));
    }

    #[test]
    fn captures_returns_groups_of_the_first_match() {
        let result = call(
            "regex.captures",
            vec![s("user=ada id=7; user=bob"), s(r"user=(\w+)( id=(\d+))?")],
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Array(vec![s("user=ada id=7"), s("ada"), s(" id=7"), s("7")])
        );
        let none = call("regex.captures", vec![s("nothing"), s(r"user=(\w+)")]).unwrap();
        assert_eq!(none, Value::Null);
    }

    #[test]
    fn captures_all_returns_every_match_with_null_for_unmatched_groups() {
        let result = call(
            "regex.captures_all",
            vec![s("a=1 b c=3"), s(r"(?<key>\w)(=(\d))?")],
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::Array(vec![s("a=1"), s("a"), s("=1"), s("1")]),
                Value::Array(vec![s("b"), s("b"), Value::Null, Value::Null]),
                Value::Array(vec![s("c=3"), s("c"), s("=3"), s("3")]),
            ])
        );
        let empty = call("regex.captures_all", vec![s("abc"), s(r"(\d)")]).unwrap();
        assert_eq!(empty, Value::Array(vec![]));
        assert!(call("regex.captures_all", vec![s("abc"), s("(")]).is_err());
        assert!(call("regex.captures", vec![s("abc")]).is_err());
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
    fn invalid_pattern_errors_from_replace_too() {
        let result = call("regex.replace", vec![s("anything"), s(r"["), s("x")]);
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
    fn cache_returns_the_same_program_for_a_pattern() {
        let cache = RegexCache::default();
        let a = cache.get_or_compile(r"^\d+$").unwrap();
        let b = cache.get_or_compile(r"^\d+$").unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(cache.len(), 1);
        // Flags are part of the pattern text.
        let c = cache.get_or_compile(r"(?i)^\d+$").unwrap();
        assert!(!Arc::ptr_eq(&a, &c));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn cache_is_bounded_and_evicts_the_least_recently_used() {
        let cache = RegexCache::default();
        let keep = cache.get_or_compile("keep").unwrap();
        for i in 0..(CACHE_CAPACITY * 3) {
            cache.get_or_compile(&format!("p{}", i)).unwrap();
            // Using `keep` keeps it the most recently used.
            let again = cache.get_or_compile("keep").unwrap();
            assert!(
                Arc::ptr_eq(&keep, &again),
                "evicted a recently used pattern"
            );
            assert!(cache.len() <= CACHE_CAPACITY);
        }
        assert_eq!(cache.len(), CACHE_CAPACITY);
    }

    #[test]
    fn cache_does_not_keep_invalid_or_huge_patterns() {
        let cache = RegexCache::default();
        assert!(cache.get_or_compile("(").is_err());
        assert!(cache.get_or_compile("(").is_err());
        let huge = "a".repeat(MAX_CACHED_PATTERN_LEN + 1);
        assert!(cache.get_or_compile(&huge).unwrap().is_match(&huge));
        assert!(cache.is_empty());
    }

    #[test]
    fn a_budgeted_run_uses_its_own_cache() {
        use crate::runtime::limits::{self, Budget, Limits};
        let budget = Budget::new(Limits {
            max_fuel: Some(1_000_000),
            ..Limits::none()
        });
        let pattern = "budget-only-pattern-[0-9]+";
        {
            let _guard = limits::scope(Some(budget.clone()));
            assert_eq!(
                call("regex.test", vec![s("budget-only-pattern-7"), s(pattern)]).unwrap(),
                Value::Bool(true)
            );
        }
        assert_eq!(budget.regex_cache().len(), 1);
        // A run without a budget compiles into the process cache instead.
        let before = budget.regex_cache().len();
        call("regex.test", vec![s("x"), s("another-[a-z]")]).unwrap();
        assert_eq!(budget.regex_cache().len(), before);
    }

    #[test]
    fn cache_is_shared_by_threads() {
        let cache = Arc::new(RegexCache::default());
        let handles: Vec<_> = (0..4)
            .map(|t| {
                let cache = cache.clone();
                std::thread::spawn(move || {
                    for i in 0..200 {
                        let p = format!("t{}-{}", t % 2, i % 50);
                        assert!(cache.get_or_compile(&p).unwrap().is_match(&p));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(cache.len(), 100);
    }

    #[test]
    fn unknown_function_errors() {
        let result = call("regex.bogus", vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown regex function"));
    }
}
