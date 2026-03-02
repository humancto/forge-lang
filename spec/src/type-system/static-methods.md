# Static Methods

Static methods are methods attached to a type that do not operate on an instance. They are defined in `give`/`impl` blocks without `it` as the first parameter and are called on the type name itself.

## Syntax

Static methods are defined like instance methods but without `it`:

```forge
give TypeName {
    define method_name(params) {
        // body — no `it` parameter
    }
}
```

They are called on the type name:

```
TypeName.method_name(args)
```

## Defining Static Methods

A method is classified as static when its first parameter is **not** named `it`. Any other parameter name (or no parameters at all) makes it a static method.

```forge
thing Person {
    name: String,
    age: Int
}

give Person {
    // Static method — no `it` parameter
    define species() {
        return "Homo sapiens"
    }

    // Static method — first param is not `it`
    define create(name, age) {
        return Person { name: name, age: age }
    }

    // Instance method — first param IS `it`
    define greet(it) {
        say "Hello, I'm " + it.name
    }
}
```

## Invocation

Static methods are called using the type name with dot notation:

```forge
Person.species()               // "Homo sapiens"
let p = Person.create("Bob", 25)  // Person { name: "Bob", age: 25 }
```

The runtime resolves `Person.method()` by:

1. Evaluating `Person` — this yields the `BuiltIn("struct:Person")` sentinel value registered during struct definition.
2. Extracting the type name `"Person"` from the sentinel tag.
3. Looking up the method name in `static_methods["Person"]`.
4. Calling the function with the provided arguments (no instance prepended).

## Storage

Static methods are stored in the `static_methods` table:

```
static_methods: HashMap<String, IndexMap<String, Value>>
```

The key is the type name. The value is an `IndexMap` mapping method names to function values.

When a `give`/`impl` block is executed, each method is checked for the `it` parameter:

| First Parameter | Stored In                                 | Call Syntax         |
| --------------- | ----------------------------------------- | ------------------- |
| `it`            | `method_tables` only                      | `instance.method()` |
| Anything else   | Both `method_tables` and `static_methods` | `TypeName.method()` |

Static methods are stored in both tables. This means they appear in `method_tables` as well, which allows them to be found during interface satisfaction checks.

## Factory Pattern

A common use of static methods is the factory pattern — creating instances with validation or transformation logic:

```forge
thing Color {
    r: Int,
    g: Int,
    b: Int
}

give Color {
    define from_hex(hex) {
        // Parse hex string to RGB values
        return Color { r: 0, g: 0, b: 0 }
    }

    define red() {
        return Color { r: 255, g: 0, b: 0 }
    }

    define display(it) {
        return "rgb(" + str(it.r) + ", " + str(it.g) + ", " + str(it.b) + ")"
    }
}

let c = Color.red()
say c.display()    // "rgb(255, 0, 0)"
```

## Additive Blocks

Like instance methods, static methods from multiple `give`/`impl` blocks are additive:

```forge
give Math {
    define add(a, b) { return a + b }
}

give Math {
    define sub(a, b) { return a - b }
}

Math.add(1, 2)    // 3
Math.sub(5, 3)    // 2
```

## Instance vs. Static Ambiguity

The classification is based solely on whether the first parameter is named `it`. A method named `it` with a different first parameter name is static:

```forge
give Config {
    // Static: first param is "key", not "it"
    define from_key(key) {
        return Config { value: key }
    }

    // Instance: first param is "it"
    define to_string(it) {
        return str(it.value)
    }
}
```
