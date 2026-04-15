# Plan: Tuples (Revised)

## Goal

Add first-class tuple support to Forge: `(a, b, c)` literal syntax, indexing by position, destructuring, and type annotations `(int, string, bool)`.

## Current State

- `(expr)` parses as grouping (returns inner expr, no AST node)
- No `Expr::Tuple`, `TypeAnn::Tuple`, or `ObjKind::Tuple` anywhere
- Array infrastructure (parsing, opcodes, eval) is directly reusable

## Design Decisions

- **Disambiguation**: `(expr)` = grouping, `(e1, e2)` = tuple, `(e1,)` = single-element tuple (trailing comma)
- **Runtime representation**: Dedicated `ObjKind::Tuple(Vec<Value>)` — NOT reusing Array. Tuples are immutable, fixed-length, heterogeneous.
- **Immutability**: Tuples are frozen at creation. No `.push()`, `.pop()`, mutation.
- **Indexing**: `t[0]`, `t[1]` bracket syntax only. Skip `t.0` dot-integer to avoid lexer conflicts (`.0` can lex as part of a float). Can add dot-integer later.
- **Destructuring**: `let (a, b, c) = tuple_expr` — reuse existing destructure infrastructure
- **Type annotation**: `(int, string, bool)` — new `TypeAnn::Tuple(Vec<TypeAnn>)`
- **Display**: `(1, "hello", true)` — parenthesized, comma-separated
- **Equality**: Element-wise comparison. No ordering (`<`, `>`) — clear error.
- **Builtins**: `len()` works on tuples, `type()` returns `"tuple"`, `contains()` works. Other collection ops (`map`, `filter`, `sort`) → clear "not supported on tuples" error.
- **Iteration**: `for x in (1, 2, 3)` works — tuples are iterable (like Python).
- **Hashing**: Not a map key for now. Document as future work.
- **Match/when**: Out of scope for this PR. Tuple patterns in `match` arms = separate item.
- **Spread/rest**: Out of scope. `let (first, ..rest) = tup` = separate item.
- **String interpolation**: Display impl is used — confirm both interpreter and VM formatting paths call it.

## Implementation Steps

### 1. AST (`src/parser/ast.rs`)

- Add `Tuple(Vec<Expr>)` to `Expr` enum
- Add `Tuple(Vec<TypeAnn>)` to `TypeAnn` enum
- Add `Tuple { items: Vec<Pattern> }` to destructuring patterns

### 2. Parser (`src/parser/parser.rs`)

- Modify parenthesized expression parsing: parse first expr, if next is Comma → collect rest → `Expr::Tuple`
- Handle trailing comma for single-element: `(42,)` → Tuple
- Parse `(T1, T2)` type annotations
- Parse `let (a, b) = ...` tuple destructuring
- **Watch out**: `foo((x,))` = calling foo with single-element tuple arg. `foo(x,)` = trailing comma in call args (if supported). These are naturally disambiguated by the parser because tuple parsing happens inside the argument expression parser.

### 3. Value (`src/vm/value.rs`)

- Add `Tuple(Vec<Value>)` to `ObjKind`
- Implement Display, PartialEq, Clone for tuples
- `type_name()` returns `"tuple"`

### 4. GC (`src/vm/gc.rs`)

- Add trace arm for `ObjKind::Tuple` — walk all elements and mark reachable objects. **Critical for memory safety.**

### 5. Interpreter (`src/interpreter/mod.rs`)

- Evaluate `Expr::Tuple` → create tuple value
- Support `[0]`, `[1]` bracket access on tuples
- Support tuple destructuring in let/assignment
- `len()` returns tuple length
- `contains()` works on tuples
- `for x in tuple` iteration
- Tuples are immutable — reject mutation attempts with clear error
- String interpolation uses tuple Display

### 6. VM Compiler (`src/vm/compiler.rs`)

- Add **`NewTuple`** opcode (dedicated, NOT reusing NewArray)
- Compile `Expr::Tuple` → push elements → `NewTuple(dest, start, count)`
- Support tuple bracket indexing
- Support tuple destructuring

### 7. VM Machine (`src/vm/machine.rs`)

- Handle `NewTuple` opcode → allocate `ObjKind::Tuple`
- Handle tuple index access
- Handle tuple destructuring
- Handle tuple iteration in ForIter

### 8. VM Bytecode (`src/vm/bytecode.rs`)

- Add `NewTuple` opcode variant

### 9. JIT (`src/vm/jit/`)

- Check that the JIT's opcode match has a fallback/bail for unknown opcodes. If `NewTuple` is encountered, JIT should bail to VM (no native tuple compilation needed yet).

### 10. Bytecode Serialization (`src/vm/serialize.rs`)

- Serialize/deserialize `NewTuple` opcode
- Serialize/deserialize tuple values in constant pool if needed

### 11. Type Checker (`src/typechecker.rs`)

- Infer tuple types from elements
- Check tuple destructuring arity
- Validate tuple indexing bounds (when statically known)

### 12. Parity Tests (`tests/parity/`)

- Add parity fixture(s) for tuples — run across interpreter, VM, bytecode round-trip, and JIT

## Test Strategy (PLENTY)

### Rust unit tests

- Parser: tuple literal `(1, 2, 3)`, single-element `(x,)`, nested `((1,2), 3)`
- Parser: `(42)` stays as grouping (NOT tuple)
- Parser: tuple type annotation `(int, string)`
- Parser: tuple destructuring `let (a, b) = ...`
- Value: tuple display `"(1, 2, 3)"`, equality, clone
- Interpreter: tuple creation, bracket indexing `[0]`, out-of-bounds error
- Interpreter: tuple immutability (reject push/pop/mutation)
- Interpreter: tuple destructuring
- Interpreter: tuple in function return
- Interpreter: nested tuples
- Interpreter: tuple equality `==` and `!=`
- Interpreter: tuple ordering `<` → clear error
- Interpreter: `len()` on tuple
- Interpreter: `contains()` on tuple
- Interpreter: `for x in tuple` iteration
- Interpreter: `type(tup)` returns `"tuple"`
- Interpreter: string interpolation with tuple
- VM: tuple creation + bracket indexing
- VM: tuple destructuring
- VM: tuple in for loop
- Type checker: tuple type inference, arity mismatch warning
- Serialization: round-trip tuple through bytecode

### Parity tests (.fg fixtures)

- Basic tuple creation, access, destructuring
- Tuple as function return
- Tuple comparison
- Tuple iteration
- Mixed-type tuples
- Nested tuples
- len() and contains()

## Rollback

Revert the branch. Changes span parser, interpreter, VM, typechecker — all additive.

## Edge Cases

- `()` — empty parens: keep as unit/grouping, not empty tuple
- `(42)` — single expr without trailing comma: grouping, not tuple
- `(42,)` — trailing comma: single-element tuple
- `((1, 2))` — nested grouping of tuple: works naturally
- Tuple mutation attempt → clear error message
- `foo((1, 2))` — passing tuple as function argument: works naturally
- Out-of-bounds index → runtime error with clear message
