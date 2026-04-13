# Plan: 8A.2 — Flow-sensitive type narrowing in if/match

## Goal

When the condition of an `if` or `match` tests for null/type, narrow the variable's type inside the corresponding branch. This enables the type checker to recognize patterns like `if x != null { x.field }` as safe.

## Current behavior

```forge
fn greet(name: ?String) {
    if name != null {
        say name       // checker still sees name as ?String
    }
}
```

The checker treats `name` as `?String` in both branches — no narrowing.

## Desired behavior

```forge
fn greet(name: ?String) {
    if name != null {
        say name       // checker sees name as String (narrowed)
    } else {
        // name is Null here
    }
}
```

## Approach

### 1. Extract narrowing facts from conditions

Add `extract_narrowing(expr: &Expr) -> Vec<(String, NarrowingFact)>` that pattern-matches:

- `x != null` / `x != None` → variable `x` is narrowed to non-null (unwrap Option)
- `x == null` / `x == None` → variable `x` is narrowed to Null
- `is_some(x)` → narrowed to non-null
- `is_none(x)` → narrowed to Null
- `is_ok(x)` → narrowed from Result to the Ok type
- `is_err(x)` → narrowed from Result to the Err type

Also support:

- Negation: `!(x == null)` → same as `x != null`
- `&&` chains: `x != null && y != null` → extract facts from both sides (recurse into BinOp::And)
- `||` chains: no narrowing in then branch (too complex for v1)

### 2. Apply narrowing in if branches

In `check_stmt` for `Stmt::If`:

1. Extract narrowing facts from `condition`
2. Before checking `then_body`: save current variable types, apply positive narrowing facts
3. Check `then_body` with narrowed types
4. Restore saved types
5. Before checking `else_body`: apply inverted narrowing facts
6. Check `else_body` with inverted narrowed types
7. Restore saved types

### 3. Early return narrowing

After an if-block where the then-body always returns/breaks (guaranteed exit), apply the _inverted_ narrowing to the rest of the function. Example:

```forge
if x == null { return }
// x is non-null here
```

Detection: check if the last statement in the then-body is `Stmt::Return`. If so, apply inverted facts to the current scope (not saved/restored).

### 4. Apply narrowing in match arms

In `check_stmt` for `Stmt::Match`:

1. Infer the subject type (must be `Expr::Ident` to narrow)
2. For each arm, if the pattern is `Pattern::Literal(Expr::Ident("null"))` → narrow subject to Null in that arm's body
3. For arms with `Pattern::Constructor { name: "Some", .. }` → narrow to inner type
4. For arms with `Pattern::Constructor { name: "Ok", .. }` / `Pattern::Constructor { name: "Err", .. }` → narrow Result
5. Non-null arms (any pattern other than null literal) → narrow to non-null

### 5. Type narrowing mechanics

`narrow_type(current: &InferredType, fact: &NarrowingFact) -> InferredType`:

- `Option(T)` + NonNull → `T`
- `Option(T)` + IsNull → `Null`
- `Result(T, E)` + IsOk → `T`
- `Result(T, E)` + IsErr → `E`
- Any type + NonNull → same type (already non-null)
- Unknown + any → Unknown (can't narrow what we don't know)

### Implementation detail

Use a simple save/restore pattern for variable scopes:

```rust
let saved = self.variables.clone();
// apply narrowing
self.variables.insert(var, narrowed_type);
// check body
for s in body { self.check_stmt(&s.stmt); }
// restore
self.variables = saved;
```

Forge uses block scoping (`push_scope`/`pop_scope` in the interpreter), so the save/restore is semantically correct — variables declared inside the if-body are properly scoped to it.

## Edge cases

- Or conditions: `if x == null || y == null` → no narrowing in then (too complex for v1)
- Reassignment inside branch: narrowing may be invalidated. This is advisory-only — OK to be optimistic. Known false-negative, documented.
- Non-identifier subjects: `if foo.bar != null` → skip (only narrow simple idents for v1)
- `when` guards: MatchArm has no guard field in the AST — structurally impossible, not deferred
- `null` in AST: parser emits `Expr::Ident("null")` for null literals (verified)
- Interaction with 8A.1: `collect_return_types` (return type inference) doesn't apply narrowing when walking branches. This means `fn f(x: ?String) { if x != null { return x } }` infers return type as `?String` not `String`. Acceptable for v1 — return type inference is conservative. Can be improved later by having `infer_body_return_type` apply narrowing.

## Files to touch

1. **`src/typechecker.rs`** — add `NarrowingFact` enum, `extract_narrowing()`, `narrow_type()`, `invert_fact()`, update `check_stmt` for If and Match

## Test strategy

- `x != null` narrows `?String` to `String` in then branch
- `x == null` narrows to `Null` in then branch, non-null in else
- `is_some(x)` narrows Option
- `is_ok(x)` narrows Result
- Match with null pattern narrows subject
- Match with Some/Ok/Err constructor narrows
- No narrowing for Unknown types
- Negation: `!(x == null)` narrows same as `x != null`
- `&&` chains: `x != null && y != null` narrows both
- Narrowing doesn't leak out of if scope
- Early return: `if x == null { return }` narrows x after the if

## Rollback

Revert the single file change.
