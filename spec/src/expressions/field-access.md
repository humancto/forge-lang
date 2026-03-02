# Field Access

The dot operator (`.`) accesses fields on objects and struct instances. Forge resolves field access through direct lookup followed by delegation through embedded fields.

## Syntax

```
expression.identifier
```

The left operand is evaluated to produce an object. The identifier names the field to retrieve.

## Object Field Access

Objects are unordered collections of key-value pairs. Dot notation retrieves a value by key.

```forge
let user = { name: "Alice", age: 30 }
say user.name       // "Alice"
say user.age        // 30
```

If the field does not exist on the object, a runtime error is produced:

```forge
let user = { name: "Alice" }
say user.email      // runtime error: no field 'email' on object
```

## Struct Field Access

Struct instances (created from `thing`/`struct` definitions) are objects with a `__type__` field. Field access works identically.

```forge
thing Point { x: int, y: int }
let p = Point { x: 10, y: 20 }
say p.x     // 10
say p.y     // 20
```

## Embedded Field Delegation

When a struct uses `has` to embed another type, field access delegates to the embedded field if the field is not found directly on the outer object.

```forge
thing Base { id: int }
thing Extended {
    has base: Base
    name: string
}

let e = Extended { base: Base { id: 1 }, name: "test" }
say e.name      // "test" (direct)
say e.id        // 1 (delegated to e.base.id)
```

### Resolution Order

Field access resolves in this order:

1. **Direct field lookup**: Check if the object has a field with the given name.
2. **Embedded field delegation**: If the object has a `__type__`, check each embedded field's sub-object for the field name. Embedded fields are checked in definition order.

If neither step finds the field, a runtime error is produced.

## Chaining

Field access expressions can be chained to traverse nested structures.

```forge
let config = {
    server: {
        host: "localhost",
        port: 8080
    }
}
say config.server.host      // "localhost"
say config.server.port      // 8080
```

## Built-in Field Access on Primitives

Certain built-in fields are available on primitive types:

### Strings

| Field    | Type     | Description             |
| -------- | -------- | ----------------------- |
| `.len`   | `int`    | Number of bytes         |
| `.upper` | `string` | Uppercase copy          |
| `.lower` | `string` | Lowercase copy          |
| `.trim`  | `string` | Whitespace-trimmed copy |

```forge
let s = "  Hello  "
say s.len       // 9
say s.upper     // "  HELLO  "
say s.trim      // "Hello"
```

### Arrays

| Field  | Type  | Description        |
| ------ | ----- | ------------------ |
| `.len` | `int` | Number of elements |

```forge
let items = [1, 2, 3]
say items.len   // 3
```

## Index-Based Access

For dynamic key access, use bracket notation instead of dot notation:

```forge
let obj = { name: "Alice" }
let key = "name"
say obj[key]        // "Alice"
```

Bracket notation works on both objects (with string keys) and arrays (with integer indices).
