# Struct Definitions

Structs are named record types that group related fields into a single value. Forge provides dual syntax for defining structs: `thing` (natural) and `struct` (classic).

## Syntax

```
StructDef       = ("thing" | "struct") Identifier "{" FieldList "}"
FieldList       = Field ("," Field)* ","?
Field           = EmbedField | PlainField
PlainField      = Identifier (":" TypeAnnotation)? ("=" Expression)?
EmbedField      = "has" Identifier ":" TypeAnnotation
TypeAnnotation  = Identifier
```

## Defining a Struct

The `thing` keyword (or its classic alias `struct`) introduces a new struct type. The struct name must be a valid identifier and by convention uses PascalCase.

```forge
thing Person {
    name: String,
    age: Int
}
```

Classic syntax:

```forge
struct Person {
    name: String,
    age: Int
}
```

Both forms are semantically identical. The parser produces the same `StructDef` AST node regardless of which keyword is used.

## Fields

Each field has a name and an optional type annotation. Type annotations follow the field name after a colon. In the current implementation, type annotations are parsed and stored but not enforced at runtime.

```forge
thing Config {
    host: String,
    port: Int,
    debug: Bool
}
```

Fields without type annotations are permitted:

```forge
thing Pair {
    first,
    second
}
```

## Default Values

Fields may specify a default value using `=` after the type annotation (or after the field name if no annotation is present). See [Default Values](defaults.md) for full details.

```forge
thing Config {
    host: String = "localhost",
    port: Int = 8080,
    debug: Bool = false
}
```

## Construction

Struct instances are created using the struct name followed by a field initializer block. The result is an `Object` value with the specified fields plus an automatically inserted `__type__` field.

```forge
thing Point { x: Int, y: Int }

let p = Point { x: 10, y: 20 }
// p is { __type__: "Point", x: 10, y: 20 }
```

Fields may be provided in any order. Fields with default values may be omitted; default values are applied first, then explicitly provided fields override them.

```forge
thing Config {
    host: String = "localhost",
    port: Int = 8080
}

let c = Config { port: 3000 }
// c is { __type__: "Config", host: "localhost", port: 3000 }
```

## The `__type__` Field

Every struct instance automatically receives a `__type__` field set to the struct's name as a `String` value. This field is inserted after all user-specified fields during construction. It is used by the runtime for:

- Method dispatch in `give`/`impl` blocks
- Interface satisfaction checking in `satisfies`
- Embedded field delegation via `has`

The `__type__` field is a regular field and can be read like any other:

```forge
thing Dog { name: String }
let d = Dog { name: "Rex" }
say d.__type__    // "Dog"
```

Manually setting `__type__` in the constructor is permitted but will be overwritten by the automatic insertion.

## Registration

When a `StructDef` statement is executed, the interpreter:

1. Registers the struct name in the environment as a `BuiltIn("struct:Name")` sentinel value. This value is used to identify static method calls (`Name.method()`).
2. Records any embedded fields in the `embedded_fields` table for delegation.
3. Records any default values in the `struct_defaults` table, evaluating default expressions at definition time.

The struct name can subsequently be used as a constructor in `StructInit` expressions.

## Field Access

Fields are accessed using dot notation:

```forge
thing Person { name: String, age: Int }
let p = Person { name: "Alice", age: 30 }

say p.name    // "Alice"
say p.age     // 30
```

If the field does not exist on the object directly, the runtime checks embedded sub-objects before reporting an error. See [Composition](composition.md).
