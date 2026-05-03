# Production Readiness Changelog Plan

## Goal

Close production-readiness H2 by updating `[Unreleased]` in `CHANGELOG.md` for the roadmap work merged after the existing entries.

## Current State

`CHANGELOG.md` already includes entries for:

- PR #136 request-id extraction hardening
- PR #137 OTel hardening
- PR #138 VM source columns and standalone decorator rejection

It is missing entries for:

- PR #140 MySQL transaction APIs

## Implementation

1. Add one `Added` entry for PR #140:
   - MySQL now exposes `mysql.begin(conn_id)`, `mysql.commit(tx)`, and `mysql.rollback(tx)` through opaque transaction handles, with `mysql.query` / `mysql.execute` accepting transaction ids to run on the pinned physical connection.
2. Mention that this supersedes the earlier v0.5.0 deferral where MySQL transactions were intentionally omitted because pooled calls could hit different physical connections.
3. Do not add PR #139, #141, or #142:
   - #139 and #141 are test-only.
   - #142 is internal diagnostic/chore-only.
4. Use Keep-a-Changelog section order already present in the file. Keep wording concise and include the PR link.

## Tests

- Review by eye.
- No code tests required; this is markdown-only.

## Rollback

Remove the new changelog bullets.
