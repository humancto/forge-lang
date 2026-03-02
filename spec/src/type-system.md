# Type System

This chapter defines the type system of Forge. Forge is dynamically typed at runtime but provides structural mechanisms for organizing data and behavior: struct definitions, method attachment, interface contracts, composition via embedding, and structural satisfaction checking.

## Overview

Forge's type system is built on five pillars:

1. **Struct definitions** (`thing`/`struct`) — Define named record types with typed fields and optional defaults.
2. **Method blocks** (`give`/`impl`) — Attach instance and static methods to struct types after definition.
3. **Interface contracts** (`power`/`interface`) — Declare behavioral contracts that types may fulfill.
4. **Composition** (`has`) — Embed one struct inside another with automatic field and method delegation.
5. **Structural satisfaction** (`satisfies`) — Test whether a value's type fulfills an interface at runtime, regardless of explicit declaration.

## Dynamic Foundation

All Forge values are represented at runtime by the `Value` enum. There is no compile-time type erasure or monomorphization. Type annotations on struct fields and function parameters are documentation and future-proofing; the interpreter does not enforce them at assignment time in the current version.

The `typeof` builtin returns a string naming the runtime type:

```forge
typeof(42)        // "Int"
typeof("hello")   // "String"
typeof(true)      // "Bool"
typeof(null)      // "Null"
typeof([1, 2])    // "Array"
typeof({a: 1})    // "Object"
```

Struct instances are `Object` values with a `__type__` field that records the struct name.

## Type Identity

Every struct instance carries a `__type__` field set to the struct's name as a string. This field is automatically inserted during construction and is used by the runtime for method dispatch, interface satisfaction checking, and embedded-field delegation.

```forge
thing Point { x: Int, y: Int }
let p = Point { x: 1, y: 2 }
p.__type__    // "Point"
typeof(p)     // "Object"
```

The `typeof` builtin returns `"Object"` for all struct instances. The `__type__` field distinguishes between different struct types at a finer granularity.

## No Generics

Forge does not currently support generic types or parameterized type constructors. All collections (arrays, objects) are heterogeneous. Interface methods are checked by name and arity, not by parameter types.

## Subsections

The following subsections define each type system feature in detail:

- [Struct Definitions](type-system/struct-definitions.md) — Defining named record types.
- [Method Blocks](type-system/method-blocks.md) — Attaching methods to types.
- [Interface Contracts](type-system/interface-contracts.md) — Declaring and implementing behavioral contracts.
- [Composition](type-system/composition.md) — Embedding types and delegation.
- [Structural Satisfaction](type-system/structural-satisfaction.md) — Runtime interface checking.
- [Default Values](type-system/defaults.md) — Default field values in struct definitions.
- [Static Methods](type-system/static-methods.md) — Type-level methods without a receiver.
