# Composition

Forge supports struct composition through the `has` keyword, which embeds one struct inside another. Embedded fields enable automatic delegation of both field access and method calls to the inner struct, providing a composition-based alternative to inheritance.

## Syntax

```
EmbedField = "has" Identifier ":" TypeAnnotation
```

The `has` keyword appears in a struct field position and marks the field as embedded.

## Defining Embedded Fields

Use `has` inside a struct definition to embed another type:

```forge
thing Address {
    street: String,
    city: String,
    zip: String
}

thing Employee {
    name: String,
    has addr: Address
}
```

The `addr` field is an embedded field of type `Address`. The `has` keyword tells the runtime to register this field for delegation.

## Construction

Embedded fields are initialized like regular fields during construction:

```forge
let emp = Employee {
    name: "Alice",
    addr: Address {
        street: "123 Main St",
        city: "Springfield",
        zip: "62701"
    }
}
```

## Field Delegation

When a field is accessed on a struct instance and the field is not found directly on the object, the runtime checks each embedded sub-object for the field. This enables transparent access to inner fields:

```forge
// Direct access (always works)
emp.addr.city      // "Springfield"

// Delegated access (through embedding)
emp.city           // "Springfield"
emp.street         // "123 Main St"
```

The delegation algorithm for `obj.field`:

1. Check if `obj` has a direct field named `field`. If found, return it.
2. Read `obj.__type__` to get the type name.
3. Look up the type name in `embedded_fields` to get the list of `(field_name, type_name)` pairs.
4. For each embedded field, check if `obj[field_name]` is an object with the requested `field`. If found, return it.
5. If no embedded field contains the requested field, raise a runtime error.

## Method Delegation

Method calls are also delegated to embedded types. If a method is not found in the outer type's method table, the runtime searches each embedded type's method table:

```forge
give Address {
    define full(it) {
        return it.street + ", " + it.city + " " + it.zip
    }
}

// Called on the embedded Address through Employee
emp.full()         // "123 Main St, Springfield 62701"

// Explicit path also works
emp.addr.full()    // "123 Main St, Springfield 62701"
```

The method delegation algorithm for `obj.method(args)`:

1. Look up `method` in `method_tables[obj.__type__]`. If found, call it with `obj` prepended as `it`.
2. Look up `embedded_fields[obj.__type__]` to get the list of embedded fields.
3. For each `(embed_field, embed_type)` pair, look up `method` in `method_tables[embed_type]`.
4. If found, extract `obj[embed_field]` as the receiver and call the method with the sub-object as `it`.
5. If no embedded type has the method, continue to builtin resolution or raise an error.

## Multiple Embeddings

A struct may embed multiple fields:

```forge
thing Engine {
    horsepower: Int
}

thing Chassis {
    material: String
}

thing Car {
    make: String,
    has engine: Engine,
    has chassis: Chassis
}

give Engine {
    define rev(it) {
        say "Vroom! " + str(it.horsepower) + "hp"
    }
}

give Chassis {
    define describe(it) {
        return it.material + " chassis"
    }
}

let c = Car {
    make: "Toyota",
    engine: Engine { horsepower: 200 },
    chassis: Chassis { material: "Steel" }
}

c.rev()          // prints "Vroom! 200hp"
c.describe()     // "Steel chassis"
c.horsepower     // 200
c.material       // "Steel"
```

Embedded fields are searched in declaration order. If two embedded types provide the same field name, the first match wins.

## Embedding and Interfaces

Delegated methods count toward interface satisfaction. If an embedded type's method table contains a method required by an interface, the outer type satisfies that interface through delegation:

```forge
power Describable {
    fn describe(it) -> String
}

// Car satisfies Describable through its embedded Chassis
satisfies(c, Describable)    // true (via chassis.describe)
```

## Storage

The interpreter maintains an `embedded_fields` table:

```
embedded_fields: HashMap<String, Vec<(String, String)>>
```

The key is the outer struct name. The value is a vector of `(field_name, type_name)` pairs, one for each `has` field in the struct definition. This table is populated when the `StructDef` statement is executed.
