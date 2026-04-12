# 5A.2 — Add missing GC roots for method_tables, static_methods, struct_defaults

## Problem

The GC root scanning at `machine.rs:1940-1957` only scans registers, globals, and frame closures. It misses `method_tables`, `static_methods`, and `struct_defaults` — all `HashMap<String, IndexMap<String, Value>>`. Any `Value::Obj` GcRef in these tables could be freed during collection, causing use-after-free on subsequent method calls.

## Approach

Add three loops after the existing globals scanning (before `self.gc.collect(&roots)`):

```rust
for methods in self.method_tables.values() {
    for v in methods.values() {
        if let Value::Obj(gr) = v {
            roots.push(*gr);
        }
    }
}
for methods in self.static_methods.values() {
    for v in methods.values() {
        if let Value::Obj(gr) = v {
            roots.push(*gr);
        }
    }
}
for defaults in self.struct_defaults.values() {
    for v in defaults.values() {
        if let Value::Obj(gr) = v {
            roots.push(*gr);
        }
    }
}
```

## Files

- `src/vm/machine.rs:1940-1958` — add root scanning for method_tables, static_methods, struct_defaults

## Test strategy

- Existing 950 tests validate no regression
- The fix is defensive — use-after-free is non-deterministic and hard to trigger in a test

## Rollback

Revert the single commit.
