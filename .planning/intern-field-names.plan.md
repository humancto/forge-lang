# Plan: 11A.3 — Intern Field Names

## Goal

Object field lookups use interned GcRef keys instead of String keys, eliminating string hash computation on hot paths.

## Current State

- Objects are `ObjKind::Object(IndexMap<String, Value>)`
- `GetField` extracts field name from constant pool as `Constant::Str(String)`, clones it, does `map.get(&field)`
- `SetField` does `map.insert(field.clone(), val)`
- `NewObject` reads string keys from registers and inserts into IndexMap
- Every field access hashes the full string content

## Design

### Too invasive: change IndexMap key type

Changing `IndexMap<String, Value>` to `IndexMap<GcRef, Value>` would touch every place that creates, reads, or serializes objects — builtins, display, JSON, etc. This is a massive refactor.

### Better approach: cache field name GcRefs in GetField/SetField

Since `GetField` and `SetField` opcodes reference field names by constant pool index, we can cache the interned GcRef for each constant string. The constant pool already deduplicates strings. We just need to avoid cloning and re-hashing the string on every access.

But wait — the lookup still goes through `map.get(&field)` which hashes the `String`. The only way to avoid hashing is to change the map key type.

### Practical approach for this item

Actually, the simplest meaningful optimization: cache the `String` field name extraction from the constant pool to avoid repeated `.clone()` in GetField. But that's minor.

The real win per the roadmap is to change the object key type. Let me scope it:

1. Change `ObjKind::Object(IndexMap<String, Value>)` to `ObjKind::Object(IndexMap<u64, Value>)` where the key is a string hash precomputed at interning time. No — this loses the ability to iterate field names.

2. Better: keep `IndexMap<String, Value>` but use a parallel lookup structure. Too complex.

3. Simplest real win: Since field names from GetField/SetField come from the constant pool and are short strings, they're already interned. The IndexMap hash of a short string (typical field names are <20 chars) is already fast. The main cost is the `.clone()` of the field name string on every GetField.

### Final approach: eliminate field name cloning

The current code does `let field = field.clone()` on every GetField to satisfy borrow checker (can't hold a reference into `chunk.constants` while mutably accessing `self`). Instead:

1. Pre-intern all string constants when loading a chunk — store `Vec<Option<GcRef>>` mapping constant index → interned GcRef
2. In GetField, look up the pre-interned ref, get the string from GC to use as the map key
3. This avoids the String::clone() per field access

Actually, the clone is needed because the borrow checker won't let us hold a ref into constants while calling self.gc methods. Let me just focus on what the roadmap says: "eliminating hash computation on hot paths."

### Revised approach: SmallString for field names

Use a stack-allocated small string for field name lookups. Field names are typically short (<32 bytes). Use `SmallVec<[u8; 32]>` or similar to avoid heap allocation.

### FINAL approach (keeping it simple)

The `.clone()` in GetField is the real cost. Use `Cow<str>` or pre-compute an index. But the simplest effective change:

1. In `GetField`/`SetField`, avoid cloning the field string by restructuring the code to not need the clone
2. Use `chunk.constants[c].as_str()` method that returns `&str`, and restructure so the borrow ends before GC access

### Files to touch

1. `src/vm/machine.rs` — restructure GetField/SetField to avoid field name cloning
2. `src/vm/bytecode.rs` — add `Constant::as_str()` helper

## Test strategy

- All existing tests pass (transparent optimization)

## Rollback

Revert changes to `src/vm/machine.rs`, `src/vm/bytecode.rs`.
