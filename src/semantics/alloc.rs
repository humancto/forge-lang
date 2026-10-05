//! Fallible construction of values whose size a script chooses.
//!
//! # Invariant
//!
//! A builtin that allocates a buffer sized by a script argument
//! (`repeat_str("x", n)`, `pad_start(s, n)`, `range(0, n)`, ...) must reserve
//! it through these helpers, never through `str::repeat`, `collect()` or
//! `Vec::with_capacity`. Rust *aborts the process* when an infallible
//! allocation fails, so one line like `repeat_str("x", 100000000000)` used to
//! kill the whole host: the `forge mcp` server, or a Python program running
//! a [`crate::sandbox::Sandbox`]. Here the request is size-checked
//! (overflow) and reserved with `try_reserve_exact`, so an impossible
//! request becomes an ordinary runtime error on both engines.
//!
//! This is not a memory limit: a request the allocator grants is still
//! granted (bounding total memory is a host/resource-limit concern).

/// The error both engines report when a sized allocation cannot be made.
/// `size` is a user-meaningful count (`"5000000000 elements"`), identical on
/// both engines whatever their value representation.
pub fn too_large(what: &str, size: Option<String>) -> String {
    match size {
        Some(size) => format!("{}: result too large ({}); not enough memory", what, size),
        None => format!("{}: result too large; not enough memory", what),
    }
}

/// An empty `Vec` with room for exactly `len` elements, or an error.
pub fn vec_with_capacity<T>(len: usize, what: &str) -> Result<Vec<T>, String> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| too_large(what, Some(format!("{} elements", len))))?;
    Ok(v)
}

/// An empty `String` with room for exactly `bytes` bytes, or an error.
pub fn string_with_capacity(bytes: usize, what: &str) -> Result<String, String> {
    let mut s = String::new();
    s.try_reserve_exact(bytes)
        .map_err(|_| too_large(what, Some(format!("{} bytes", bytes))))?;
    Ok(s)
}

/// `s` repeated `n` times (`repeat_str`).
pub fn repeat_str(s: &str, n: usize, what: &str) -> Result<String, String> {
    let bytes = s
        .len()
        .checked_mul(n)
        .ok_or_else(|| too_large(what, None))?;
    let mut out = string_with_capacity(bytes, what)?;
    for _ in 0..n {
        out.push_str(s);
    }
    Ok(out)
}

/// `count` copies of `pad` (`pad_start` / `pad_end`).
pub fn padding(pad: char, count: usize, what: &str) -> Result<String, String> {
    let bytes = pad
        .len_utf8()
        .checked_mul(count)
        .ok_or_else(|| too_large(what, None))?;
    let mut out = string_with_capacity(bytes, what)?;
    out.extend(std::iter::repeat_n(pad, count));
    Ok(out)
}

/// Number of integers in `start..end` (0 when empty).
pub fn range_len(start: i64, end: i64) -> usize {
    if end <= start {
        return 0;
    }
    // i128 so `i64::MIN..i64::MAX` cannot overflow; saturate on 32-bit.
    usize::try_from(end as i128 - start as i128).unwrap_or(usize::MAX)
}

/// The integers `start..end`, mapped with `f`, as a pre-reserved `Vec`.
pub fn int_range<T>(
    start: i64,
    end: i64,
    what: &str,
    mut f: impl FnMut(i64) -> T,
) -> Result<Vec<T>, String> {
    let mut out = vec_with_capacity(range_len(start, end), what)?;
    for n in start..end {
        out.push(f(n));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impossible_requests_are_errors_not_aborts() {
        assert!(repeat_str("x", usize::MAX, "repeat_str()").is_err());
        assert!(repeat_str("ab", usize::MAX / 2 + 1, "repeat_str()").is_err());
        assert!(padding('é', usize::MAX, "pad_start()").is_err());
        assert!(vec_with_capacity::<u64>(usize::MAX, "range()").is_err());
        assert!(int_range(i64::MIN, i64::MAX, "range()", |n| n).is_err());
    }

    #[test]
    fn normal_requests_work() {
        assert_eq!(repeat_str("ab", 3, "r").as_deref(), Ok("ababab"));
        assert_eq!(repeat_str("ab", 0, "r").as_deref(), Ok(""));
        assert_eq!(padding('é', 2, "p").as_deref(), Ok("éé"));
        assert_eq!(int_range(2, 5, "r", |n| n), Ok(vec![2, 3, 4]));
        assert_eq!(int_range(5, 2, "r", |n| n), Ok(vec![]));
        assert_eq!(range_len(i64::MIN, i64::MAX), usize::MAX);
    }
}
