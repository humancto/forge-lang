//! Example Forge plugin. Build it with `cargo build` in this directory, then
//! run `forge --allow-ffi run main.fg`.

use forge_plugin::{export, forge_fn, Bytes, Value};
use std::collections::BTreeMap;

/// Integers in, integer out.
#[forge_fn]
fn add(a: i64, b: i64) -> i64 {
    a.wrapping_add(b)
}

/// Strings are copied in and out as UTF-8.
#[forge_fn]
fn greet(name: String) -> String {
    format!("Hello, {}! (from Rust)", name)
}

/// `Result::Err` becomes a Forge runtime error (catchable with try/catch).
#[forge_fn]
fn divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err("division by zero".to_string())
    } else {
        Ok(a / b)
    }
}

/// Arrays map to `Vec<T>`.
#[forge_fn]
fn sum(values: Vec<f64>) -> f64 {
    values.iter().sum()
}

/// Objects map to maps (or `Value` for anything); `Option` maps to null.
#[forge_fn]
fn word_stats(text: String, top: Option<i64>) -> BTreeMap<String, Value> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let longest = words.iter().copied().max_by_key(|w| w.len()).unwrap_or("");
    let limit = top.unwrap_or(3).max(0) as usize;
    let mut stats = BTreeMap::new();
    stats.insert("words".to_string(), Value::Int(words.len() as i64));
    stats.insert("longest".to_string(), Value::String(longest.to_string()));
    stats.insert(
        "first".to_string(),
        Value::Array(
            words
                .iter()
                .take(limit)
                .map(|w| Value::String(w.to_string()))
                .collect(),
        ),
    );
    stats
}

/// A typed hash in native code: FNV-1a over the UTF-8 bytes.
#[forge_fn]
fn fnv1a(text: String) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// Bytes cross as arrays of ints 0-255.
#[forge_fn]
fn reverse_bytes(data: Bytes) -> Bytes {
    Bytes(data.0.into_iter().rev().collect())
}

/// Panics never unwind into Forge; they become runtime errors.
#[forge_fn]
fn explode(message: String) -> i64 {
    panic!("{}", message)
}

export!(
    name = "hello_rust",
    functions = [
        add,
        greet,
        divide,
        sum,
        word_stats,
        fnv1a,
        reverse_bytes,
        explode
    ]
);
