//! Messages for network failures of the HTTP client (`http.*`, `fetch`,
//! `http.download`, `http.crawl`), shared so every entry point words them
//! the same way and `errors::classify` gives them their codes: a request
//! that timed out is E0036, one that could not connect (refused, unknown
//! host, TLS handshake, other connect errors) E0037. The callers prefix
//! them with the operation (`http.get error: `).
//!
//! The target is named as `host:port`, never as the full URL: query
//! strings can carry credentials.

use std::time::Duration;

/// Stable start of a timeout message (after the `<op> error: ` prefix).
pub const TIMED_OUT: &str = "timed out after ";
/// Stable starts of a connection failure message.
pub const CONNECTION_REFUSED: &str = "connection refused by ";
pub const UNKNOWN_HOST: &str = "cannot resolve host ";
pub const TLS_FAILED: &str = "TLS handshake failed with ";
pub const CONNECT_FAILED: &str = "could not connect to ";

/// Why a connection could not be made (a timeout is [`timed_out`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Refused,
    UnknownHost,
    Tls,
    Connect,
}

/// `1 second`, `2 seconds`, `0.5 seconds`.
pub fn describe_seconds(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs == 1.0 {
        "1 second".to_string()
    } else if secs.fract() == 0.0 {
        format!("{} seconds", secs as u64)
    } else {
        // Up to millisecond precision, without trailing zeros.
        let text = format!("{:.3}", secs);
        let text = text.trim_end_matches('0').trim_end_matches('.');
        format!("{} seconds", text)
    }
}

/// `timed out after 2 seconds waiting for example.com:443`.
pub fn timed_out(limit: Duration, target: &str) -> String {
    format!(
        "{}{} waiting for {}\n  hint: the `timeout` option is in seconds (default 30); raise it, or catch the error and retry",
        TIMED_OUT,
        describe_seconds(limit),
        target
    )
}

/// The message for a connection that could not be made. `detail` is the
/// underlying error's text (may be empty).
pub fn connect_failed(kind: Failure, target: &str, detail: &str) -> String {
    let detail = detail.trim();
    let with_detail = |head: String| {
        if detail.is_empty() {
            head
        } else {
            format!("{}: {}", head, detail)
        }
    };
    match kind {
        Failure::Refused => format!(
            "{}{}\n  hint: nothing is listening there; check the host and port, and that the server is running",
            CONNECTION_REFUSED, target
        ),
        Failure::UnknownHost => with_detail(format!("{}'{}'", UNKNOWN_HOST, target)),
        Failure::Tls => with_detail(format!("{}{}", TLS_FAILED, target)),
        Failure::Connect => with_detail(format!("{}{}", CONNECT_FAILED, target)),
    }
}

/// Whether a headline (`http.get error: timed out after ...`) reports a
/// timed-out request (E0036).
pub fn is_timeout_headline(h: &str) -> bool {
    after_error_prefix(h).is_some_and(|rest| rest.starts_with(TIMED_OUT))
}

/// Whether a headline reports a connection that could not be made (E0037).
pub fn is_connect_headline(h: &str) -> bool {
    after_error_prefix(h).is_some_and(|rest| {
        [CONNECTION_REFUSED, UNKNOWN_HOST, TLS_FAILED, CONNECT_FAILED]
            .iter()
            .any(|p| rest.starts_with(p))
    })
}

/// The text after `<op> error: ` (`http.get error: `, `fetch error: `,
/// `download error: `, `crawl error: `).
fn after_error_prefix(h: &str) -> Option<&str> {
    let (op, rest) = h.split_once(" error: ")?;
    let known = op == "fetch"
        || op == "download"
        || op == "crawl"
        || op
            .strip_prefix("http.")
            .is_some_and(|m| !m.is_empty() && m.chars().all(|c| c.is_ascii_lowercase()));
    known.then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_are_described_in_seconds() {
        assert_eq!(describe_seconds(Duration::from_secs(1)), "1 second");
        assert_eq!(describe_seconds(Duration::from_secs(30)), "30 seconds");
        assert_eq!(describe_seconds(Duration::from_millis(250)), "0.25 seconds");
        assert_eq!(describe_seconds(Duration::from_millis(1500)), "1.5 seconds");
    }

    #[test]
    fn headlines_are_recognised_only_after_a_client_prefix() {
        let t = format!(
            "http.get error: {}",
            timed_out(Duration::from_secs(1), "h:80")
        );
        assert!(is_timeout_headline(t.lines().next().unwrap_or("")));
        assert!(!is_connect_headline(&t));
        for kind in [
            Failure::Refused,
            Failure::UnknownHost,
            Failure::Tls,
            Failure::Connect,
        ] {
            let m = format!("fetch error: {}", connect_failed(kind, "h:80", "x"));
            assert!(is_connect_headline(m.lines().next().unwrap_or("")), "{m}");
            assert!(!is_timeout_headline(&m), "{m}");
        }
        assert!(!is_timeout_headline("timed out after 1 second"));
        assert!(!is_connect_headline("my error: connection refused by x"));
    }
}
