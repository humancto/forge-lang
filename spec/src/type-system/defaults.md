# Default Values

Struct fields may specify default values that are applied when the field is omitted during construction. Default expressions are evaluated at definition time and stored in the interpreter's `struct_defaults` table.

## Syntax

```
FieldWithDefault = Identifier (":" TypeAnnotation)? "=" Expression
```

The default value follows an `=` sign after the field name and optional type annotation.

## Defining Defaults

```forge
thing Config {
    host: String = "localhost",
    port: Int = 8080,
    debug: Bool = false
}
```

A field may have a type annotation, a default, both, or neither:

| Field Form            | Example                  |
| --------------------- | ------------------------ |
| Name only             | `data`                   |
| Name + type           | `data: String`           |
| Name + default        | `data = "hello"`         |
| Name + type + default | `data: String = "hello"` |

## Evaluation Timing

Default expressions are evaluated **at definition time** — when the `thing`/`struct` statement is executed, not when an instance is constructed. This means:

```forge
let counter = 0

thing Widget {
    id: Int = counter
}

change counter to 10

let w = Widget {}
say w.id    // 0, not 10 — default was captured at definition time
```

The default value is the result of evaluating the expression at the point where the struct is defined. Subsequent changes to variables referenced in the default expression do not affect the stored default.

## Application During Construction

When a struct instance is constructed, defaults are applied first, then explicitly provided fields override them:

1. The interpreter retrieves the defaults from `struct_defaults[StructName]`.
2. All default key-value pairs are inserted into the new object.
3. Explicitly provided fields in the constructor are evaluated and inserted, overwriting any defaults with the same key.
4. The `__type__` field is inserted last.

```forge
thing Server {
    host: String = "0.0.0.0",
    port: Int = 3000,
    workers: Int = 4
}

// All defaults
let s1 = Server {}
// s1 = { host: "0.0.0.0", port: 3000, workers: 4, __type__: "Server" }

// Partial override
let s2 = Server { port: 8080 }
// s2 = { host: "0.0.0.0", port: 8080, workers: 4, __type__: "Server" }

// Full override
let s3 = Server { host: "127.0.0.1", port: 443, workers: 16 }
// s3 = { host: "127.0.0.1", port: 443, workers: 16, __type__: "Server" }
```

## Default Expressions

Default values may be any valid expression, not just literals. They are evaluated in the current scope at definition time:

```forge
let default_name = "World"

thing Greeter {
    greeting: String = "Hello, " + default_name + "!"
}

let g = Greeter {}
say g.greeting    // "Hello, World!"
```

Function calls, arithmetic, string concatenation, and other expressions are all valid defaults.

## Storage

The interpreter stores defaults in a `struct_defaults` table:

```
struct_defaults: HashMap<String, IndexMap<String, Value>>
```

The outer key is the struct name. The inner `IndexMap` maps field names to their default `Value`. Only fields with defaults are stored; fields without defaults are absent from the map.

## Fields Without Defaults

Fields without defaults must be provided during construction. If a field without a default is omitted, the constructed object simply will not have that field — no error is raised at construction time, but accessing the missing field later will produce a runtime error.
