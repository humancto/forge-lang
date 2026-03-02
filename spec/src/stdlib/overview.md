# Standard Library Overview

Forge ships with **18 built-in modules** containing over 238 functions. All modules are available without any import statement. There is no `import math` or `require("fs")` -- every module is pre-loaded into the global scope.

## Accessing Modules

Modules are accessed via dot notation:

```forge
let root = math.sqrt(144)       // 12.0
let data = fs.read("config.json")
let hash = crypto.sha256("hello")
```

Each module is a first-class object. You can assign it to a variable:

```forge
let m = math
say m.pi    // 3.141592653589793
```

## Module Index

| Module                | Description                               | Functions |
| --------------------- | ----------------------------------------- | --------- |
| [`math`](math.md)     | Mathematical operations and constants     | 17        |
| [`fs`](fs.md)         | File system operations                    | 20        |
| [`io`](io.md)         | Input/output and command-line arguments   | 6         |
| [`crypto`](crypto.md) | Hashing, encoding, and decoding           | 6         |
| [`db`](db.md)         | SQLite database operations                | 4         |
| [`pg`](pg.md)         | PostgreSQL database operations            | 4         |
| [`mysql`](mysql.md)   | MySQL database with parameterized queries | 4         |
| [`jwt`](jwt.md)       | JSON Web Token authentication             | 4         |
| [`json`](json.md)     | JSON parsing and serialization            | 3         |
| [`csv`](csv.md)       | CSV parsing and serialization             | 4         |
| [`regex`](regex.md)   | Regular expression matching               | 5         |
| [`env`](env.md)       | Environment variables                     | 4         |
| [`log`](log.md)       | Structured logging with timestamps        | 4         |
| [`term`](term.md)     | Terminal colors, formatting, and widgets  | 25+       |
| [`http`](http.md)     | HTTP client and server decorators         | 9         |
| [`exec`](exec.md)     | External command execution                | 1         |
| [`time`](time.md)     | Date, time, and timezone operations       | 25        |
| [`npc`](npc.md)       | Fake data generation for testing          | 16        |

## Execution Tier Support

All modules are fully supported in the **interpreter** (default execution mode). The bytecode VM (`--vm`) and JIT (`--jit`) support a subset of modules -- primarily `math`, `fs`, `io`, and `npc`. For full module access, use the interpreter.
