# log

Structured logging with timestamps and severity levels. All log output is written to stderr with ANSI color formatting.

## Functions

### log.info(...args) -> null

Logs an informational message in green.

```forge
log.info("Server started on port", 8080)
// [14:30:15 INFO]  Server started on port 8080
```

### log.warn(...args) -> null

Logs a warning message in yellow.

```forge
log.warn("Disk usage above 80%")
// [14:30:15 WARN]  Disk usage above 80%
```

### log.error(...args) -> null

Logs an error message in red.

```forge
log.error("Failed to connect to database:", err)
// [14:30:15 ERROR] Failed to connect to database: connection refused
```

### log.debug(...args) -> null

Logs a debug message in gray. Useful for development diagnostics.

```forge
log.debug("Request payload:", data)
// [14:30:15 DEBUG] Request payload: {name: "Alice"}
```

## Notes

- All functions accept any number of arguments. Arguments are converted to strings and joined with spaces.
- Timestamps use the local time in `HH:MM:SS` format.
- Output goes to stderr, not stdout, so it does not interfere with piped output.
