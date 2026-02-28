# RFC 0003: Dual Syntax — Classic and Natural

- **Status:** Implemented
- **Author:** Archith Rapaka
- **Date:** 2026-01-25

## Summary

Forge supports two syntactic styles that coexist in the same program: a classic style familiar to programmers (`let`, `fn`, `else`) and a natural style that reads like English (`set`, `define`, `otherwise`). Both compile to identical AST nodes.

## Motivation

Programming languages force a choice: be approachable (Python, Ruby) or be precise (Rust, Go). Forge rejects this tradeoff.

Beginners and non-engineers benefit from syntax that reads like intent:

```
set name to "World"
say "Hello, {name}!"
repeat 3 times { say "Welcome!" }
```

Experienced developers benefit from concise, familiar syntax:

```
let name = "World"
println("Hello, {name}!")
for i in range(0, 3) { println("Welcome!") }
```

Both should work. Both should be idiomatic. Neither should be second-class.

## Design

### Keyword Mappings

| Natural             | Classic                | Purpose                |
| ------------------- | ---------------------- | ---------------------- |
| `set x to val`      | `let x = val`          | Variable declaration   |
| `change x to val`   | `x = val`              | Reassignment           |
| `define name() {}`  | `fn name() {}`         | Function definition    |
| `say "text"`        | `println("text")`      | Output                 |
| `yell "text"`       | _(unique)_             | Uppercase output       |
| `whisper "text"`    | _(unique)_             | Lowercase output       |
| `otherwise`         | `else`                 | Else branch            |
| `nah`               | `else`                 | Else branch (informal) |
| `for each x in arr` | `for x in arr`         | Iteration              |
| `repeat N times`    | _(unique)_             | Counted loop           |
| `grab x from "url"` | `let x = fetch("url")` | HTTP fetch             |
| `wait N seconds`    | `wait(N)`              | Sleep                  |

### Rules

1. Both styles parse to the same AST nodes — no runtime difference
2. Both styles can be mixed in the same file
3. Neither style is deprecated or discouraged
4. `yell`, `whisper`, and `repeat` have no classic equivalent — they are Forge-unique

## Alternatives Considered

### "Pick one style and commit"

Rejected. A single style alienates either beginners or experienced developers. Supporting both costs very little (a few extra keyword entries in the lexer) and doubles the addressable audience.

### "Make natural syntax a separate mode"

Rejected. Modes create confusion. Both styles should work everywhere, always.

### "Add even more natural keywords"

Partially accepted. Keywords like `otherwise` and `nah` were added for control flow, but we stopped short of full natural-language constructs (e.g., "if the user is logged in then show the dashboard"). The line is drawn at keywords that map cleanly to existing programming concepts.
