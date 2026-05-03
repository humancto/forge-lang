# Final Verification Plan

## Goal

Close production-readiness H3 by running the final verification checklist after all preceding production-readiness items have merged.

## Checklist

Run from clean `main` on a verification branch:

1. `cargo test`
2. `cargo build --release`
3. `target/release/forge --allow-run test`
4. `target/release/forge run examples/hello.fg`
5. `target/release/forge run examples/functional.fg`
6. `target/release/forge --vm run examples/hello.fg`
7. `target/release/forge --jit run examples/hello.fg`

The release build uses the default Cargo feature set, which includes `jit`.
The Forge integration suite includes shell-execution tests, so the explicit
`--allow-run` permission flag is required. All commands must exit successfully;
any `forge --allow-run test` failure is a blocker and should be recorded in the
report instead of treated as a pass.

## Artifact

Create `.planning/final-verification-report.md` with:

- command list
- pass/fail result for each command
- any warnings or skipped items
- final git SHA verified

## Tests

The checklist is the test.

## Rollback

Remove the verification report and plan file if the verification PR should not be kept.
