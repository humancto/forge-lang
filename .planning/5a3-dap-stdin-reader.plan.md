# 5A.3 — Fix DAP stdin reader to use single BufReader

## Problem

`run_dap()` uses `stdin.lock().lines()` for the outer loop but creates fresh `io::stdin()` handles at lines 35 and 39 to read the separator and body. Under pipelining (multiple messages queued), the separate handles read from an independent buffer position, corrupting or losing bytes.

## Approach

Replace the `for line in stdin.lock().lines()` pattern with a single `BufReader<Stdin>` and a manual loop:

```rust
let mut reader = io::BufReader::new(io::stdin());
loop {
    let mut header = String::new();
    if reader.read_line(&mut header).unwrap_or(0) == 0 { break; }

    if header.starts_with("Content-Length:") {
        let len: usize = header.trim_start_matches("Content-Length:")
            .trim().parse().unwrap_or(0);

        // Read empty separator line
        let mut sep = String::new();
        reader.read_line(&mut sep).ok();

        // Read body
        let mut content = vec![0u8; len];
        reader.read_exact(&mut content).ok();
        let body = String::from_utf8_lossy(&content).to_string();

        // ... parse and handle
    }
}
```

## Files

- `src/dap/mod.rs:10-45` — restructure reading loop

## Test strategy

- Existing tests + manual DAP protocol test
- The fix is straightforward I/O refactoring

## Rollback

Revert the single commit.
