# The Forge Language Specification

[Introduction](./introduction.md)

---

# Part I: Language Core

- [Lexical Structure](./lexical-structure.md)
  - [Source Text](./lexical-structure/source-text.md)
  - [Keywords](./lexical-structure/keywords.md)
  - [Identifiers](./lexical-structure/identifiers.md)
  - [Literals](./lexical-structure/literals.md)
  - [Operators and Punctuation](./lexical-structure/operators.md)
  - [Comments](./lexical-structure/comments.md)
- [Types](./types.md)
  - [Primitive Types](./types/primitives.md)
  - [Collection Types](./types/collections.md)
  - [Struct Types (thing)](./types/structs.md)
  - [Interface Types (power)](./types/interfaces.md)
  - [Function Types](./types/functions.md)
  - [Algebraic Data Types](./types/adt.md)
  - [Option and Result](./types/option-result.md)
  - [Type Conversions](./types/conversions.md)
- [Expressions](./expressions.md)
  - [Arithmetic](./expressions/arithmetic.md)
  - [Comparison and Logical](./expressions/comparison.md)
  - [String Interpolation](./expressions/string-interpolation.md)
  - [Field Access](./expressions/field-access.md)
  - [Method Calls](./expressions/method-calls.md)
  - [Closures and Lambdas](./expressions/closures.md)
  - [When Guards](./expressions/when-guards.md)
  - [Match Expressions](./expressions/match.md)
- [Statements](./statements.md)
  - [Variable Declaration](./statements/variables.md)
  - [Assignment](./statements/assignment.md)
  - [Control Flow](./statements/control-flow.md)
  - [Loops](./statements/loops.md)
  - [Function Declaration](./statements/functions.md)
  - [Return, Break, Continue](./statements/jump.md)
  - [Import and Export](./statements/modules.md)
- [The Type System](./type-system.md)
  - [Struct Definitions (thing / struct)](./type-system/struct-definitions.md)
  - [Method Blocks (give / impl)](./type-system/method-blocks.md)
  - [Interface Contracts (power / interface)](./type-system/interface-contracts.md)
  - [Composition (has)](./type-system/composition.md)
  - [Structural Satisfaction (satisfies)](./type-system/structural-satisfaction.md)
  - [Default Field Values](./type-system/defaults.md)
  - [Static Methods](./type-system/static-methods.md)
- [Error Handling](./error-handling.md)
  - [Result Type](./error-handling/result.md)
  - [The ? Operator](./error-handling/propagation.md)
  - [safe and must](./error-handling/safe-must.md)
  - [check Validation](./error-handling/check.md)
- [Concurrency](./concurrency.md)
  - [Channels](./concurrency/channels.md)
  - [Spawn](./concurrency/spawn.md)
  - [Async Functions (forge / async)](./concurrency/async.md)
  - [Await (hold / await)](./concurrency/await.md)

---

# Part II: Standard Library

- [Module System](./stdlib/overview.md)
- [math](./stdlib/math.md)
- [fs](./stdlib/fs.md)
- [crypto](./stdlib/crypto.md)
- [db (SQLite)](./stdlib/db.md)
- [pg (PostgreSQL)](./stdlib/pg.md)
- [json](./stdlib/json.md)
- [csv](./stdlib/csv.md)
- [regex](./stdlib/regex.md)
- [env](./stdlib/env.md)
- [log](./stdlib/log.md)
- [term](./stdlib/term.md)
- [http](./stdlib/http.md)
- [io](./stdlib/io.md)
- [exec](./stdlib/exec.md)
- [time](./stdlib/time.md)
- [npc](./stdlib/npc.md)

---

# Part III: Built-in Functions

- [Output Functions](./builtins/output.md)
- [Type Functions](./builtins/type-functions.md)
- [Collection Functions](./builtins/collections.md)
- [String Functions](./builtins/strings.md)
- [Object Functions](./builtins/objects.md)
- [Shell Functions](./builtins/shell.md)
- [Assertion and Testing](./builtins/assertions.md)
- [GenZ Debug Kit](./builtins/genz.md)
- [Execution Helpers](./builtins/execution.md)

---

# Part IV: Dual Syntax Reference

- [Syntax Philosophy](./dual-syntax/philosophy.md)
- [Complete Mapping Table](./dual-syntax/mapping.md)
- [Innovation Keywords](./dual-syntax/innovation.md)

---

# Part V: Runtime and Internals

- [Execution Model](./internals/execution-model.md)
- [The Interpreter](./internals/interpreter.md)
- [The Bytecode VM](./internals/bytecode-vm.md)
- [The JIT Compiler](./internals/jit.md)
- [HTTP Server Runtime](./internals/http-server.md)
- [Memory Model](./internals/memory.md)

---

# Appendices

- [Grammar (EBNF)](./appendix/grammar.md)
- [Keyword Index](./appendix/keywords.md)
- [Operator Precedence](./appendix/precedence.md)
- [Changelog](./appendix/changelog.md)
