# Plan: 8B.1 — Parse generic type parameters

## Goal

Parse generic type parameters on function and struct definitions: `fn map<T, U>(...)` and `struct Pair<T> { ... }`. Store them in the AST. No type checking of generics yet (that's 8B.2).

## Current behavior

```forge
fn map<T, U>(arr: [T], f: fn(T) -> U) -> [U] { ... }
// Parse error: unexpected '<'
```

## Desired behavior

```forge
fn map<T, U>(arr: [T], f: fn(T) -> U) -> [U] { ... }
// Parses successfully, type_params = ["T", "U"] stored in AST
```

## Approach

### 1. Add `type_params` to AST nodes

Add `type_params: Vec<String>` to:

- `Stmt::FnDef` — generic functions
- `Stmt::StructDef` — generic structs

### 2. Parse `<T, U>` after names

In `parse_fn_def`, after `let name = self.expect_ident()?;`:

```
let type_params = self.parse_type_params()?;
```

Same in `parse_struct_def`, after `let name = self.expect_ident()?;`.

### 3. `parse_type_params` helper

```rust
fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
    if !self.check(&Token::Lt) { return Ok(vec![]); }
    self.advance(); // consume <
    let mut params = Vec::new();
    loop {
        params.push(self.expect_ident()?);
        if self.check(&Token::Comma) {
            self.advance();
        } else {
            break;
        }
    }
    self.expect(Token::Gt)?;
    Ok(params)
}
```

### 4. Update all AST consumers

Every place that destructures `Stmt::FnDef` or `Stmt::StructDef` needs to handle the new field. Add `type_params: _` or use `..` to ignore it where appropriate.

Key consumers:

- `typechecker.rs` — `collect_definitions`, `check_stmt`, `infer_all_return_types`
- `interpreter/mod.rs` — function/struct handling
- `vm/compiler.rs` — function compilation
- `lsp/mod.rs` — go-to-definition
- Any other files that match on FnDef/StructDef

### 5. TypeAnn already handles generic usage

`TypeAnn::Generic(String, Vec<TypeAnn>)` already exists for type positions like `Result<Int, String>`. Type parameters like `T` in param types are already parsed as `TypeAnn::Simple("T")`. No changes needed to type annotation parsing.

## Edge cases

- Empty type params `fn f<>()` → parse error (require at least one)
- `<` ambiguity: only parse type params right after an identifier in fn/struct position — no ambiguity with comparison since `<` can't follow a bare ident in these contexts
- Existing code without generics: `type_params` is empty vec, no behavior change

## Files to touch

1. **`src/parser/ast.rs`** — add `type_params: Vec<String>` to FnDef and StructDef
2. **`src/parser/parser.rs`** — add `parse_type_params()`, call it in `parse_fn_def` and `parse_struct_def`
3. **`src/typechecker.rs`** — update destructure patterns for FnDef/StructDef (ignore type_params for now)
4. **`src/interpreter/mod.rs`** — update destructure patterns
5. **`src/vm/compiler.rs`** — update destructure patterns
6. **`src/lsp/mod.rs`** — update destructure patterns
7. **Other files** — any that match on FnDef/StructDef

## Test strategy

- Parse `fn f<T>(x: T) -> T { return x }` successfully
- Parse `struct Pair<T> { first: T, second: T }` successfully
- Parse `fn f<T, U>(x: T, y: U) -> T { return x }` (multiple params)
- Existing functions without generics still parse
- `type_params` is empty for non-generic functions

## Rollback

Revert all file changes (AST + parser + consumer updates).
