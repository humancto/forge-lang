//! Characters and code points: the rules of `chr(n)` and `ord(s)`, shared by
//! both engines (same results, same error text).

/// The largest Unicode code point (`chr`'s upper bound).
pub const MAX_CODE_POINT: i64 = 0x10FFFF;

/// `chr(n)`: the one-character string for code point `n`. Surrogates
/// (U+D800..U+DFFF) are not characters and are refused.
pub fn chr(n: i64) -> Result<String, String> {
    u32::try_from(n)
        .ok()
        .and_then(char::from_u32)
        .map(String::from)
        .ok_or_else(|| {
            format!(
                "chr() requires a code point from 0 to {} that is not a surrogate (55296 to 57343), got {}",
                MAX_CODE_POINT, n
            )
        })
}

/// The error for a `chr` argument that is not an Int (the dispatcher adds
/// the argument's type).
pub fn chr_requires_int() -> String {
    "chr() requires an Int code point".to_string()
}

/// `ord(s)`: the code point of the only character of `s`.
pub fn ord(s: &str) -> Result<i64, String> {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(i64::from(u32::from(c))),
        (None, _) => {
            Err("ord() requires a string of exactly one character, got an empty string".to_string())
        }
        _ => Err(format!(
            "ord() requires a string of exactly one character, got {} characters",
            s.chars().count()
        )),
    }
}

/// The error for an `ord` argument that is not a String.
pub fn ord_requires_string() -> String {
    "ord() requires a string of exactly one character".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::errors::classify;

    #[test]
    fn chr_and_ord_round_trip() {
        for n in [0, 65, 0xE9, 0xFEFF, 0x1F600, MAX_CODE_POINT] {
            let s = chr(n).expect("valid");
            assert_eq!(ord(&s), Ok(n));
        }
        assert_eq!(chr(233).as_deref(), Ok("é"));
        assert_eq!(ord("€"), Ok(8364));
    }

    #[test]
    fn invalid_arguments_are_e0015() {
        for err in [
            chr(-1).unwrap_err(),
            chr(0xD800).unwrap_err(),
            chr(MAX_CODE_POINT + 1).unwrap_err(),
            chr_requires_int(),
            ord("").unwrap_err(),
            ord("ab").unwrap_err(),
            ord_requires_string(),
        ] {
            assert_eq!(classify(&err).code, "E0015", "{err}");
        }
        assert!(ord("abc").unwrap_err().ends_with("got 3 characters"));
    }
}
