# Shell Functions

Functions for executing shell commands and interacting with the operating system. Unlike `run_command`, these functions execute through a system shell (`/bin/sh` on Unix, `cmd` on Windows), so pipes, redirects, and globbing work.

## sh(command) -> string

Executes a shell command and returns stdout as a string. Errors if the command fails.

```forge
let files = sh("ls -la")
say files

let count = sh("wc -l < data.txt")
say count
```

## shell(command) -> object

Executes a shell command and returns a detailed result object with `stdout`, `stderr`, `status`, and `ok` fields.

```forge
let result = shell("git status")
if result.ok {
    say result.stdout
} else {
    say "Error: " + result.stderr
}
```

## sh_lines(command) -> array

Executes a shell command and returns stdout split into an array of lines.

```forge
let files = sh_lines("ls *.fg")
for file in files {
    say "Found: " + file
}
```

## sh_json(command) -> any

Executes a shell command and parses stdout as JSON.

```forge
let config = sh_json("cat package.json")
say config.name
say config.version
```

## sh_ok(command) -> bool

Executes a shell command and returns `true` if the exit code is 0.

```forge
if sh_ok("which python3") {
    say "Python 3 is installed"
}
```

## which(program) -> string | null

Returns the full path to `program`, or `null` if not found. Equivalent to the Unix `which` command.

```forge
which("node")    // "/usr/local/bin/node"
which("foobar")  // null
```

## cwd() -> string

Returns the current working directory as a string.

```forge
say cwd()  // "/home/alice/project"
```

## cd(path) -> null

Changes the current working directory.

```forge
cd("/tmp")
say cwd()  // "/tmp"
```

## pipe_to(command, input) -> string

Pipes `input` as stdin to the given command and returns stdout.

```forge
let sorted = pipe_to("sort", "banana\napple\ncherry")
say sorted
// apple
// banana
// cherry
```
