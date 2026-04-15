# 6A.3 — Fix And/Or short-circuit evaluation in VM

## Problem

The compiler treats `&&`/`||` like any binary op: compiles both sides into registers, then emits `OpCode::And`/`OpCode::Or`. This means the right side is always evaluated, even when the left side determines the result. The interpreter correctly short-circuits (lines 1805-1819 in mod.rs).

Example: `x != null && x.foo > 0` — the VM evaluates `x.foo` even when `x` is null, causing a runtime error that the interpreter avoids.

## Fix

Change the **compiler** to emit short-circuit jumps for `&&`/`||` instead of a simple binary opcode.

### For `&&` (And):

```
compile left → dst
JumpIfFalse dst, +skip   // if left is falsy, skip right side (result is already false)
compile right → dst       // overwrite dst with right result
// convert to bool
skip:
```

### For `||` (Or):

```
compile left → dst
JumpIfTrue dst, +skip    // if left is truthy, skip right side (result is already true)
compile right → dst
skip:
```

### Implementation in compiler.rs:

In the `Expr::BinOp` handler (line 1596), special-case `And`/`Or` before the general binary op path:

```rust
Expr::BinOp { left, op, right } => {
    if matches!(op, BinOp::And | BinOp::Or) {
        compile_expr(c, left, dst)?;
        // Emit a placeholder jump (offset patched later)
        let jump_op = if matches!(op, BinOp::And) { OpCode::JumpIfFalse } else { OpCode::JumpIfTrue };
        let jump_pc = c.emit(encode_asbx(jump_op, dst, 0), 0);
        // Compile right side into dst
        compile_expr(c, right, dst)?;
        // Patch jump to skip right side
        let offset = (c.current_pc() - jump_pc - 1) as i16;
        c.patch_sbx(jump_pc, offset);
        return Ok(());
    }
    // ... existing binary op code
}
```

### Machine.rs changes: None needed

`JumpIfFalse`/`JumpIfTrue` already work. The `And`/`Or` opcodes become dead but keep them for backward compatibility with serialized bytecode.

## Files to change

1. `src/vm/compiler.rs` — short-circuit And/Or compilation (~15 lines)

## Test strategy

- All 950 existing tests must pass
- Add a parity test: `let x = null; let r = x != null && x.foo > 0` — should return false (not crash)
- Verify `||` short-circuits: `true || crash()` should not call crash()

## Edge cases

- Nested: `a && b && c` — each `&&` is a separate BinOp node, short-circuits correctly
- Mixed: `a || b && c` — parser precedence handles grouping, compiler handles each level

## Rollback

Compiler-only change, ~15 lines.
