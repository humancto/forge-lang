# Keywords

Keywords are reserved identifiers with special syntactic meaning. A keyword cannot be used as a variable name, function name, or type name.

Forge's keyword set is organized into five categories: classic keywords (familiar from other languages), natural-language keywords (Forge's English-like alternatives), innovation keywords (unique to Forge), error handling keywords, and type keywords.

## Classic Keywords

These keywords provide syntax familiar to developers coming from Rust, JavaScript, Go, or Python.

| Keyword     | Purpose                                 |
| ----------- | --------------------------------------- |
| `let`       | Variable declaration                    |
| `mut`       | Mutable variable modifier               |
| `fn`        | Function declaration                    |
| `return`    | Return from function                    |
| `if`        | Conditional branch                      |
| `else`      | Alternative branch                      |
| `match`     | Pattern matching                        |
| `for`       | For loop                                |
| `in`        | Iterator binding (used with `for`)      |
| `while`     | While loop                              |
| `loop`      | Infinite loop                           |
| `break`     | Exit a loop                             |
| `continue`  | Skip to next loop iteration             |
| `struct`    | Struct type definition                  |
| `type`      | Algebraic data type definition          |
| `interface` | Interface definition                    |
| `impl`      | Method block / interface implementation |
| `pub`       | Public visibility modifier              |
| `import`    | Module import                           |
| `spawn`     | Spawn a concurrent task                 |
| `true`      | Boolean literal true                    |
| `false`     | Boolean literal false                   |
| `null`      | Null literal                            |
| `async`     | Async function declaration              |
| `await`     | Await an async expression               |
| `yield`     | Yield a value from a generator          |

## Natural-Language Keywords

These keywords provide English-like alternatives to classic syntax. Each natural keyword maps to an equivalent classic construct.

| Natural Keyword | Classic Equivalent | Usage                            |
| --------------- | ------------------ | -------------------------------- |
| `set`           | `let`              | `set x to 5`                     |
| `to`            | `=`                | Used with `set` and `change`     |
| `change`        | (reassignment)     | `change x to 10`                 |
| `define`        | `fn`               | `define greet(name) { }`         |
| `otherwise`     | `else`             | `} otherwise { }`                |
| `nah`           | `else`             | `} nah { }` (informal)           |
| `each`          | (loop modifier)    | `for each x in items { }`        |
| `repeat`        | (counted loop)     | `repeat 5 times { }`             |
| `times`         | (loop count)       | Used with `repeat`               |
| `grab`          | (fetch)            | `grab resp from "url"`           |
| `from`          | (source)           | Used with `grab` and `unpack`    |
| `wait`          | (sleep)            | `wait 2 seconds`                 |
| `seconds`       | (time unit)        | Used with `wait` and `timeout`   |
| `say`           | `println`          | `say "hello"`                    |
| `yell`          | (uppercase print)  | `yell "hello"` prints `HELLO`    |
| `whisper`       | (lowercase print)  | `whisper "HELLO"` prints `hello` |
| `thing`         | `struct`           | `thing Person { }`               |
| `power`         | `interface`        | `power Describable { }`          |
| `give`          | `impl`             | `give Person { }`                |
| `craft`         | (constructor)      | `craft Person { name: "A" }`     |
| `the`           | (connector)        | `give X the power Y { }`         |
| `forge`         | `async`            | `forge fetch_data() { }`         |
| `hold`          | `await`            | `hold expr`                      |
| `emit`          | `yield`            | `emit value`                     |
| `unpack`        | (destructure)      | `unpack {a, b} from obj`         |

## Dual Syntax Mapping

The following table shows equivalent forms for the most common constructs:

| Construct          | Classic            | Natural                     |
| ------------------ | ------------------ | --------------------------- |
| Variable           | `let x = 5`        | `set x to 5`                |
| Mutable variable   | `let mut x = 0`    | `set mut x to 0`            |
| Reassignment       | `x = 10`           | `change x to 10`            |
| Function           | `fn add(a, b) { }` | `define add(a, b) { }`      |
| Else branch        | `else { }`         | `otherwise { }` / `nah { }` |
| Struct definition  | `struct Point { }` | `thing Point { }`           |
| Interface          | `interface I { }`  | `power I { }`               |
| Impl block         | `impl T { }`       | `give T { }`                |
| Impl for interface | `impl I for T { }` | `give T the power I { }`    |
| Constructor        | `Point { x: 1 }`   | `craft Point { x: 1 }`      |
| Async function     | `async fn f() { }` | `forge f() { }`             |
| Await              | `await expr`       | `hold expr`                 |
| Yield              | `yield value`      | `emit value`                |
| Destructure        | `let {a, b} = obj` | `unpack {a, b} from obj`    |

## Innovation Keywords

These keywords introduce constructs unique to Forge that have no direct equivalent in other mainstream languages.

| Keyword     | Purpose                                  | Example                        |
| ----------- | ---------------------------------------- | ------------------------------ |
| `when`      | Guard expression (multi-way conditional) | `when age { < 13 -> "kid" }`   |
| `unless`    | Postfix negative conditional             | `expr unless condition`        |
| `until`     | Postfix loop-until                       | `expr until condition`         |
| `must`      | Crash on error with message              | `must parse_int(s)`            |
| `check`     | Declarative validation                   | `check name is not empty`      |
| `safe`      | Null-safe execution block                | `safe { risky_code() }`        |
| `where`     | Collection filter                        | `items where x > 5`            |
| `timeout`   | Time-limited execution                   | `timeout 5 seconds { }`        |
| `retry`     | Automatic retry with count               | `retry 3 times { }`            |
| `schedule`  | Cron-style scheduling                    | `schedule every 5 minutes { }` |
| `every`     | Used with `schedule`                     | `schedule every N { }`         |
| `any`       | Existential quantifier                   | `any x in items`               |
| `ask`       | AI/LLM prompt                            | `ask "summarize this"`         |
| `prompt`    | Prompt template definition               | `prompt summarize() { }`       |
| `transform` | Data transformation block                | `transform data { }`           |
| `table`     | Table display                            | `table [...]`                  |
| `select`    | Query-style select                       | `from X select Y`              |
| `order`     | Query-style ordering                     | `order by field`               |
| `by`        | Used with `order` and `sort`             | `order by name`                |
| `limit`     | Query-style limit                        | `limit 10`                     |
| `keep`      | Filter synonym                           | `keep where condition`         |
| `take`      | Take N items                             | `take 5`                       |
| `freeze`    | Freeze/immobilize a value                | `freeze expr`                  |
| `watch`     | File change detection                    | `watch "file.txt" { }`         |
| `download`  | Download a file from URL                 | `download "url" to "path"`     |
| `crawl`     | Web scraping                             | `crawl "url"`                  |

## Error Handling Keywords

| Keyword | Purpose     | Example         |
| ------- | ----------- | --------------- |
| `try`   | Try block   | `try { }`       |
| `catch` | Catch block | `catch err { }` |

## Type Keywords

These identifiers are reserved as built-in type names. They are recognized by the lexer as keyword tokens, not as general identifiers.

| Keyword  | Type                  |
| -------- | --------------------- |
| `Int`    | 64-bit signed integer |
| `Float`  | 64-bit IEEE 754 float |
| `String` | UTF-8 string          |
| `Bool`   | Boolean               |
| `Json`   | JSON value type       |

## Operators as Keywords

The following operators are lexed as keyword tokens rather than punctuation:

| Token | Keyword     | Meaning                 |
| ----- | ----------- | ----------------------- |
| `\|>` | `Pipe`      | Pipe-forward operator   |
| `>>`  | `PipeRight` | Alternate pipe operator |
| `...` | `DotDotDot` | Spread operator         |
| `+=`  | `PlusEq`    | Add-assign              |
| `-=`  | `MinusEq`   | Subtract-assign         |
| `*=`  | `StarEq`    | Multiply-assign         |
| `/=`  | `SlashEq`   | Divide-assign           |

## Complete Alphabetical Index

For reference, the complete set of keyword strings recognized by the lexer (case-sensitive):

```
Int, Float, String, Bool, Json,
any, ask, async, await, break, by, catch, change, continue, craft,
crawl, define, download, each, else, emit, every, false, fn, for,
forge, freeze, from, give, grab, hold, if, impl, import, in,
interface, keep, let, limit, loop, match, mut, nah, null, order,
otherwise, power, prompt, pub, repeat, retry, return, safe, say,
schedule, seconds, select, set, spawn, struct, table, take, the,
thing, timeout, times, to, transform, true, try, type, unless,
unpack, until, wait, watch, when, where, while, whisper, yell, yield
```

All keywords are case-sensitive. `Let` and `LET` are identifiers, not keywords. The type keywords `Int`, `Float`, `String`, `Bool`, and `Json` are the only keywords that begin with an uppercase letter.
