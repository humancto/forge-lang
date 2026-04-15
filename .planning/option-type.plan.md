# Plan: Option<T> with Some/None (no more raw null)

## Goal

Make `Option<T>` a first-class type that the typechecker enforces. Variables typed as `Option<T>` must be checked before use. Variables without `Option` cannot be assigned `None`.

## Current State (verified)

- `Some(x)` and `None` work at runtime (interpreter + VM)
- `is_some(x)` / `is_none(x)` builtins exist
- The typechecker has `InferredType::Option(Box<InferredType>)` already
- The typechecker has narrowing facts (`NonNull`, `IsNull`) that work with `is_some`/`is_none`
- **Parser ALREADY parses `Option<T>` via generic syntax** (`TypeAnn::Generic("Option", [arg])`)
- **Parser ALREADY parses `?T` shorthand** (`TypeAnn::Optional(inner)`)
- **Typechecker ALREADY converts both to `InferredType::Option(inner)`**
- **Existing tests pass:** `?Int` accepts None/Some, rejects bare Int, narrowing via `is_some`/`is_none`/`match Some(v)` all work

## What's Actually Missing

The parsing and basic type checking are done. The real gaps are:

### 1. Type `unwrap()` / `unwrap_or()` return values

**src/typechecker.rs** — in `infer_expr` for `Expr::Call`:

- When function is `unwrap` and arg is `Option<T>`, return `T` (not `Unknown`)
- When function is `unwrap_or` and arg is `Option<T>`, return `T` (the fallback type should also be checked compatible with `T`)
- This enables: `let x: ?Int = Some(42); let y: Int = unwrap(x)` — no warning

### 2. Warn when Option<T> is used in operator position without narrowing

**src/typechecker.rs** — in `infer_expr` for `Expr::BinOp`:

- If left or right operand is `Option<T>` and the op is arithmetic/comparison (not `==`/`!=`), emit a warning: "Option type used in arithmetic; check with is_some() first"
- `==` and `!=` are exempt (needed for `x == null` / `x != None` checks)

### 3. Warn on reassignment of None to non-Option variable

**src/typechecker.rs** — in `check_stmt` for `Stmt::Assign`:

- If the target variable has a known non-Option type and the value is `None`/`null`, emit a warning
- Currently only `Stmt::Let` with explicit type annotation catches this

### 4. Tests for the new checks

- `unwrap(Some(42))` returns `Int` — no warning when assigned to `Int`
- `unwrap(None)` returns `Unknown` — accepted anywhere
- `unwrap_or(opt_val, 0)` returns `Int`
- `Option<Int>` in `+` operator warns
- `Option<Int>` in `==` does NOT warn
- Reassignment `x = None` where `x: Int` warns
- All existing tests continue to pass

## Explicitly Out of Scope

- **No parser changes needed** — `Option<T>` and `?T` already parse correctly
- **No runtime/VM/interpreter changes** — typechecker is advisory (warnings), not blocking
- **No method-call narrowing** (`x.is_some()`) — Forge uses free-function style
- **`null` vs `None` duality** — both are accepted for `Option<T>` today, keep it

## Edge Cases

- `Option<Option<T>>` — `is_some()` narrows to `Option<T>`, which is correct
- `None` literal — inferred as `Option<Unknown>`, unified on assignment (already works)
- Return type inference: function returning `Some(x)` or `None` → `Option<T>` (already works)

## Risk Mitigation

- Without `--strict`, all new checks emit warnings (not errors)
- All existing programs continue to work unchanged
- Changes are confined to `src/typechecker.rs` only

## Rollback

Revert typechecker changes — single file (`src/typechecker.rs`).
