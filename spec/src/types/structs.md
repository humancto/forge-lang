# Struct Types

A struct defines a **named data type** with declared fields, optional type annotations, and optional default values. Structs are the primary mechanism for defining domain-specific types in Forge.

## Definition

Structs are defined with the `struct` keyword (classic syntax) or the `thing` keyword (natural syntax). Both forms are equivalent.

> _StructDef_ → ( `struct` | `thing` ) _Identifier_ `{` _FieldList_ `}`
>
> _FieldList_ → ( _Field_ ( `,` _Field_ )\* `,`? )?
>
> _Field_ → _Identifier_ `:` _TypeAnnotation_ ( `=` _Expression_ )? | `has` _Identifier_ `:` _TypeAnnotation_

```forge
// Classic syntax
struct Point {
    x: Int,
    y: Int
}

// Natural syntax — identical result
thing Point {
    x: Int,
    y: Int
}
```

Field type annotations are part of the struct definition syntax. Each field has a name, a colon, and a type name.

## Default Values

Fields may have default values. If a field with a default is omitted during construction, the default value is used:

```forge
thing Config {
    host: String = "localhost",
    port: Int = 8080,
    debug: Bool = false
}

set cfg to craft Config {}
say cfg.host   // localhost
say cfg.port   // 8080
say cfg.debug  // false
```

Fields without defaults are required — omitting them during construction produces a runtime error.

```forge
set prod to craft Config { host: "api.example.com", port: 443 }
say prod.host   // api.example.com
say prod.debug  // false (default)
```

## Construction

There are two ways to create a struct instance:

### Direct Construction (Classic)

Use the type name followed by a field initializer block:

```forge
let p = Point { x: 3, y: 4 }
```

### Craft Construction (Natural)

Use the `craft` keyword:

```forge
set p to craft Point { x: 3, y: 4 }
```

Both forms produce the same value: an object with the declared fields plus a `__type__` field set to the struct name.

## Field Access

Fields are accessed with dot notation:

```forge
let p = Point { x: 3, y: 4 }
say p.x  // 3
say p.y  // 4
```

## The `__type__` Field

Every struct instance has an internal `__type__` field containing the struct name as a string. This field is set automatically during construction and is used by the runtime for method dispatch, interface satisfaction checking, and `typeof()`:

```forge
let p = Point { x: 3, y: 4 }
say typeof(p)  // Point
```

## Methods

Methods are attached to a struct using `give` (natural) or `impl` (classic) blocks. See [Method Blocks](../type-system/method-blocks.md) for full details.

```forge
give Point {
    define distance(it) {
        return math.sqrt(it.x * it.x + it.y * it.y)
    }
}

let p = Point { x: 3, y: 4 }
say p.distance()  // 5.0
```

The first parameter `it` is the receiver. When `p.distance()` is called, `p` is automatically bound to `it`.

### Static Methods

If a method's first parameter is not `it`, it is a static method called on the type rather than on an instance:

```forge
give Person {
    define infant(name) {
        return craft Person { name: name, age: 0 }
    }
}

set baby to Person.infant("Bob")
```

### Multiple `give` Blocks

Multiple `give` blocks for the same type are permitted. Methods accumulate across all blocks:

```forge
give Person {
    define greet(it) {
        return "Hi, I'm " + it.name
    }
}

give Person {
    define birthday(it) {
        return craft Person { name: it.name, age: it.age + 1 }
    }
}
```

## Composition with `has`

The `has` keyword inside a struct body embeds one type within another, enabling field and method delegation:

```forge
thing Address {
    street: String,
    city: String
}

thing Employee {
    name: String,
    has addr: Address
}
```

The `has` keyword provides two delegation mechanisms:

1. **Field delegation.** Accessing a field that does not exist on the outer type delegates to the embedded type. `emp.city` resolves to `emp.addr.city`.

2. **Method delegation.** Calling a method that does not exist on the outer type delegates to the embedded type. `emp.full()` resolves to `emp.addr.full()`.

```forge
give Address {
    define full(it) {
        return it.street + ", " + it.city
    }
}

set emp to craft Employee {
    name: "Charlie",
    addr: craft Address { street: "123 Main St", city: "Portland" }
}

say emp.city     // Portland (delegated)
say emp.full()   // 123 Main St, Portland (delegated)
```

The explicit path (`emp.addr.city`) also works and produces the same result.

## Struct vs. Object

| Feature                | Object                  | Struct                         |
| ---------------------- | ----------------------- | ------------------------------ |
| Type identity          | None (generic `Object`) | Named (`__type__` field)       |
| Field declarations     | No                      | Yes, with type annotations     |
| Default values         | No                      | Yes                            |
| Methods                | No                      | Yes (via `give`/`impl`)        |
| Interface satisfaction | No                      | Yes (via `give...the power`)   |
| Construction           | `{ key: value }`        | `Name { }` or `craft Name { }` |
