# Final Verification Report

Verified SHA: `3d9001ee2a948cab1d0ec9558f1677080a8a1a19`

| Command | Status | Log |
|---|---:|---|
| `cargo test` | PASS | `/tmp/forge-h3-logs/cargo-test.log` |
| `cargo build --release` | PASS | `/tmp/forge-h3-logs/cargo-build-release.log` |
| `target/release/forge --allow-run test` | PASS | `/tmp/forge-h3-logs/forge-test-allow-run.log` |
| `target/release/forge run examples/hello.fg` | PASS | `/tmp/forge-h3-logs/forge-run-hello.log` |
| `target/release/forge run examples/functional.fg` | PASS | `/tmp/forge-h3-logs/forge-run-functional.log` |
| `target/release/forge --vm run examples/hello.fg` | PASS | `/tmp/forge-h3-logs/forge-vm-hello.log` |
| `target/release/forge --jit run examples/hello.fg` | PASS | `/tmp/forge-h3-logs/forge-jit-hello.log` |

## Notes

- Logs are stored under `/tmp/forge-h3-logs`.
- `target/release/forge test` without `--allow-run` failed on 7 shell-execution tests with the expected permission error: `Shell execution denied. Use --allow-run to enable sh/shell/run_command.` The checklist now uses `--allow-run` for the integration suite because those tests intentionally exercise shell helpers.
