# Keywords

Alphabetical list of all reserved keywords in the Forge language. Keywords cannot be used as identifiers.

## Keyword Table

| Keyword     | Category       | Description                                              |
| ----------- | -------------- | -------------------------------------------------------- |
| `any`       | Innovation     | Existential check over collection                        |
| `ask`       | Innovation     | AI/LLM prompt call                                       |
| `async`     | Classic        | Async function modifier                                  |
| `await`     | Classic        | Await an async expression                                |
| `break`     | Control flow   | Exit the current loop                                    |
| `by`        | Innovation     | Sort/order modifier (`sort by`, `order by`)              |
| `catch`     | Error handling | Catch block in try/catch                                 |
| `change`    | Natural        | Reassign a variable (`change x to 10`)                   |
| `check`     | Innovation     | Declarative validation                                   |
| `continue`  | Control flow   | Skip to next loop iteration                              |
| `craft`     | Natural type   | Constructor call (`craft Person { }`)                    |
| `crawl`     | Innovation     | Web scraping                                             |
| `define`    | Natural        | Function definition (alias for `fn`)                     |
| `download`  | Innovation     | Download a file (`download url to path`)                 |
| `each`      | Natural        | Iterator keyword (`for each x in items`)                 |
| `else`      | Control flow   | Else branch in if/else                                   |
| `emit`      | Natural        | Yield a value (alias for `yield`)                        |
| `every`     | Innovation     | Interval modifier (`schedule every 5 seconds`)           |
| `false`     | Literal        | Boolean false                                            |
| `fn`        | Classic        | Function definition                                      |
| `for`       | Control flow   | For loop                                                 |
| `forge`     | Natural        | Async function modifier (alias for `async fn`)           |
| `freeze`    | Innovation     | Make a value immutable                                   |
| `from`      | Natural        | Source keyword (`grab x from url`, `from x import y`)    |
| `give`      | Natural type   | Impl block (alias for `impl`)                            |
| `grab`      | Natural        | Fetch from URL (`grab resp from "url"`)                  |
| `hold`      | Natural        | Await expression (alias for `await`)                     |
| `if`        | Control flow   | Conditional branch                                       |
| `impl`      | Classic        | Implementation block                                     |
| `import`    | Module         | Import from a module                                     |
| `in`        | Control flow   | Iterator membership (`for x in items`)                   |
| `interface` | Classic        | Interface definition                                     |
| `keep`      | Innovation     | Filter in pipe chain                                     |
| `let`       | Classic        | Variable declaration                                     |
| `limit`     | Innovation     | Limit results in query pipeline                          |
| `loop`      | Control flow   | Infinite loop                                            |
| `match`     | Control flow   | Pattern matching                                         |
| `must`      | Innovation     | Crash on error with clear message                        |
| `mut`       | Classic        | Mutable modifier                                         |
| `nah`       | Natural        | Else branch (alias for `else`)                           |
| `null`      | Literal        | Null value                                               |
| `order`     | Innovation     | Order results in query pipeline                          |
| `otherwise` | Natural        | Else branch (alias for `else`)                           |
| `power`     | Natural type   | Interface definition (alias for `interface`)             |
| `prompt`    | Innovation     | Prompt template definition                               |
| `pub`       | Visibility     | Public visibility modifier                               |
| `repeat`    | Natural        | Counted loop (`repeat 5 times { }`)                      |
| `retry`     | Innovation     | Automatic retry (`retry 3 times { }`)                    |
| `return`    | Control flow   | Return from function                                     |
| `safe`      | Innovation     | Null-safe execution block                                |
| `say`       | Natural        | Print with newline (alias for `println`)                 |
| `schedule`  | Innovation     | Cron-style scheduling                                    |
| `seconds`   | Natural        | Time unit for `wait` and `timeout`                       |
| `select`    | Innovation     | Select fields in query pipeline                          |
| `set`       | Natural        | Variable declaration (alias for `let`)                   |
| `spawn`     | Concurrency    | Spawn a concurrent task                                  |
| `struct`    | Classic        | Struct definition                                        |
| `table`     | Innovation     | Tabular data display                                     |
| `take`      | Innovation     | Take N items in pipe chain                               |
| `the`       | Natural type   | Connector (`give X the power Y`)                         |
| `thing`     | Natural type   | Struct definition (alias for `struct`)                   |
| `timeout`   | Innovation     | Time-limited execution block                             |
| `times`     | Natural        | Loop count modifier (`repeat 5 times`)                   |
| `to`        | Natural        | Assignment target (`set x to 5`, `download url to path`) |
| `transform` | Innovation     | Data transformation                                      |
| `true`      | Literal        | Boolean true                                             |
| `try`       | Error handling | Try block in try/catch                                   |
| `type`      | Classic        | Algebraic data type definition                           |
| `unless`    | Innovation     | Postfix conditional negation                             |
| `unpack`    | Natural        | Destructuring (alias for `let { }`)                      |
| `until`     | Innovation     | Postfix loop termination                                 |
| `wait`      | Natural        | Sleep with time units (`wait 2 seconds`)                 |
| `watch`     | Innovation     | File change detection block                              |
| `when`      | Innovation     | Guard-based conditional                                  |
| `where`     | Innovation     | Collection filter                                        |
| `while`     | Control flow   | While loop                                               |
| `whisper`   | Natural        | Print in lowercase                                       |
| `yield`     | Classic        | Yield a value from a generator                           |
| `yell`      | Natural        | Print in uppercase                                       |

## Built-in Type Names

These identifiers are recognized as type annotations. They are reserved in type position but can be used as regular identifiers in value position.

| Name     | Type           |
| -------- | -------------- |
| `Int`    | 64-bit integer |
| `Float`  | 64-bit float   |
| `String` | UTF-8 string   |
| `Bool`   | Boolean        |
| `Json`   | JSON value     |

## Categories

| Category       | Count | Keywords                                                                                                                                                                                                                                     |
| -------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Classic        | 10    | `async`, `await`, `fn`, `impl`, `interface`, `let`, `mut`, `struct`, `type`, `yield`                                                                                                                                                         |
| Control flow   | 12    | `break`, `continue`, `else`, `for`, `if`, `in`, `loop`, `match`, `return`, `while`, `each`, `from`                                                                                                                                           |
| Natural        | 13    | `change`, `define`, `emit`, `forge`, `grab`, `hold`, `nah`, `otherwise`, `say`, `set`, `to`, `unpack`, `whisper`, `yell`                                                                                                                     |
| Natural type   | 5     | `craft`, `give`, `power`, `the`, `thing`                                                                                                                                                                                                     |
| Innovation     | 21    | `any`, `ask`, `by`, `check`, `crawl`, `download`, `every`, `freeze`, `keep`, `limit`, `must`, `order`, `prompt`, `retry`, `safe`, `schedule`, `select`, `table`, `take`, `timeout`, `transform`, `unless`, `until`, `watch`, `when`, `where` |
| Error handling | 2     | `catch`, `try`                                                                                                                                                                                                                               |
| Concurrency    | 1     | `spawn`                                                                                                                                                                                                                                      |
| Literal        | 3     | `false`, `null`, `true`                                                                                                                                                                                                                      |
| Natural time   | 2     | `repeat`, `seconds`, `times`, `wait`                                                                                                                                                                                                         |
| Visibility     | 1     | `pub`                                                                                                                                                                                                                                        |
| Module         | 1     | `import`                                                                                                                                                                                                                                     |

## Non-Keywords

The following identifiers are **not** reserved keywords. They are built-in functions or contextual identifiers that can be shadowed:

- `has` -- parsed contextually inside struct/thing bodies
- `print`, `println` -- built-in functions, not keywords
- `Ok`, `Err`, `Some`, `None` -- built-in constructors
- `self` -- not reserved; methods receive `self` as a regular parameter name
- Module names (`math`, `fs`, `io`, etc.) -- pre-loaded global variables, not keywords
