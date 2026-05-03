# Request ID Polish Plan

Target issues: #125, #126, #127.

## Goal

Tighten the server's recorded `request_id` extraction contract after PR #124:

- Empty inbound `X-Request-Id` records as `"unknown"` and emits a warning instead of recording an empty string.
- The 64-character cap lives in one helper used by both extraction sites.
- Unit coverage locks empty, non-ASCII, and oversized header behavior without needing a full tracing-log capture harness.

## Files

- `src/runtime/server.rs`
  - Add `truncate_id_str(&str) -> &str`.
  - Add `request_id_str_from_header(&HeaderValue) -> &str` or equivalent helper that handles UTF-8 validation, emptiness, warning, and truncation.
  - Keep `extract_request_id(&RequestId) -> String` as the owned wrapper for route handlers.
  - Use the shared helper in `TraceLayer::make_span_with`.
  - Add focused unit tests under the existing `#[cfg(test)] mod tests`.
- `tests/server_concurrency.rs`
  - Keep the response-header integration test focused on propagation, not span recording. `PropagateRequestIdLayer` should still echo the raw inbound header; span sanitization is covered by unit tests.
- `CHANGELOG.md`
  - Add an `[Unreleased]` fixed entry for empty request-id handling and tested edge cases.

## Approach

1. Extract truncation into:

   ```rust
   fn truncate_id_str(s: &str) -> &str
   ```

   It returns `&s[..REQUEST_ID_MAX_LEN]` only after checking `s.len() > REQUEST_ID_MAX_LEN`. This remains safe because HTTP header values are ASCII when `to_str()` succeeds, so the 64-byte cut is on a character boundary.
   Add `debug_assert!(s.is_ascii())` to make that invariant executable in debug builds.

2. Centralize validation in a borrowed helper:

   ```rust
   fn request_id_for_span(value: &http::HeaderValue) -> &str
   ```

   Behavior:

   - `Ok(s) if !s.is_empty()` -> `truncate_id_str(s)`.
   - `Ok(_)` -> warn on `forge.server`, return `"unknown"`.
   - `Err(_)` -> existing warn, return `"unknown"`.

   Whitespace-only values remain valid. The issue is specifically empty headers; trimming would silently alter caller-provided IDs and could collapse a deliberately significant value into `"unknown"`.

   For empty/invalid warnings, record the header byte length rather than raw bytes. The expert suggested raw bytes for observability, but logging attacker-controlled header bytes creates avoidable log-injection/log-amplification risk. Length gives operators a useful signal without putting untrusted bytes into logs.

3. Keep `extract_request_id(&RequestId) -> String` as a thin wrapper:

   ```rust
   request_id_for_span(rid.header_value()).to_string()
   ```

4. Update `TraceLayer::make_span_with` to use the same helper:

   ```rust
   .map(|id| request_id_for_span(id.header_value()))
   .unwrap_or("unknown")
   ```

## Tests

Add unit tests for the extraction contract:

- `truncate_id_str_caps_oversized_id`
- `truncate_id_str_keeps_exact_boundary_id`
- `request_id_for_span_accepts_normal_ascii`
- `request_id_for_span_empty_header_is_unknown`
- `request_id_for_span_non_ascii_is_unknown`
- `extract_request_id_returns_owned_truncated_id`

Run:

- `cargo test`

## Edge Cases

- Empty inbound header should not produce an empty span field.
- Non-ASCII bytes can be constructed with `HeaderValue::from_bytes` and must not panic.
- Oversized ASCII values are truncated only for recorded span fields and the handler span; the response header remains the raw inbound value because `PropagateRequestIdLayer` echoes the request header.

## Rollback

Revert the helper extraction and tests. This restores PR #124 behavior without affecting request-id generation, propagation, or layer order.
