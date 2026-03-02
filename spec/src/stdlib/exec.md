# exec

External command execution. The `exec` module provides a single function, `run_command`, which is also available as a top-level builtin.

## Functions

### run_command(command) -> object

Executes an external command and returns a result object. The command string is split by whitespace into the program name and arguments. The command is **not** executed through a shell, which prevents shell injection.

**Returns:**

| Field    | Type     | Description                                   |
| -------- | -------- | --------------------------------------------- |
| `stdout` | `string` | Standard output (trailing whitespace trimmed) |
| `stderr` | `string` | Standard error (trailing whitespace trimmed)  |
| `status` | `int`    | Exit code (0 = success)                       |
| `ok`     | `bool`   | `true` if exit code is 0                      |

```forge
let result = run_command("ls -la")
say result.stdout
say result.ok      // true

let git = run_command("git status")
if git.ok {
    say git.stdout
} else {
    say "Git error: " + git.stderr
}
```

## Notes

- For shell features (pipes, redirects, globbing), use the `sh` builtin function instead, which executes through a shell.
- The command string is split by whitespace, so arguments with spaces are not supported. Use `sh` for complex commands.
