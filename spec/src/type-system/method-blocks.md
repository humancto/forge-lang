# Method Blocks

Method blocks attach functions to a struct type after its definition. Forge provides dual syntax: `give` (natural) and `impl` (classic). Methods are stored in the interpreter's method tables and dispatched based on the instance's `__type__` field.

## Syntax

```
MethodBlock      = GiveBlock | ImplBlock
GiveBlock        = "give" TypeName AbilityClause? "{" MethodDef* "}"
ImplBlock        = "impl" (TypeName | AbilityForType) "{" MethodDef* "}"
AbilityClause    = "the" "power" InterfaceName
AbilityForType   = InterfaceName "for" TypeName
MethodDef        = ("fn" | "define") Identifier "(" ParamList ")" Block
TypeName         = Identifier
InterfaceName    = Identifier
```

## Instance Methods

An instance method is a method whose first parameter is named `it`. The `it` parameter receives the struct instance on which the method is called — it is Forge's equivalent of `self` or `this` in other languages.

```forge
thing Person {
    name: String,
    age: Int
}

give Person {
    define greet(it) {
        say "Hello, I'm " + it.name
    }

    define birthday(it) {
        return it.age + 1
    }
}
```

Classic syntax:

```forge
impl Person {
    fn greet(it) {
        say "Hello, I'm " + it.name
    }
}
```

Both `give` and `impl` are semantically identical. The parser produces the same `ImplBlock` AST node.

## Method Invocation

Instance methods are called using dot notation on a struct instance. The runtime automatically passes the instance as the `it` argument:

```forge
let p = Person { name: "Alice", age: 30 }
p.greet()       // prints "Hello, I'm Alice"
p.birthday()    // returns 31
```

When the interpreter encounters `p.greet()`, it:

1. Evaluates `p` to get the receiver object.
2. Reads `p.__type__` to get the type name (`"Person"`).
3. Looks up `"greet"` in `method_tables["Person"]`.
4. Prepends `p` to the argument list as the `it` parameter.
5. Calls the resolved function with the full argument list.

## Static Methods

A method without `it` as its first parameter is a static method. Static methods are called on the type name itself, not on instances. See [Static Methods](static-methods.md) for full details.

```forge
give Person {
    define species() {
        return "Homo sapiens"
    }
}

Person.species()    // "Homo sapiens"
```

## Additive Blocks

Multiple `give`/`impl` blocks for the same type are additive. Each block adds its methods to the existing method table without removing previously defined methods.

```forge
thing Car {
    make: String
}

give Car {
    define brand(it) {
        return it.make
    }
}

give Car {
    define honk(it) {
        say "Beep!"
    }
}

let c = Car { make: "Toyota" }
c.brand()    // "Toyota"
c.honk()     // prints "Beep!"
```

If a later block defines a method with the same name as an existing method, the later definition overwrites the earlier one in the method table.

## Method Table Storage

The interpreter maintains two `HashMap` tables:

| Table            | Key       | Value                     | Lookup                   |
| ---------------- | --------- | ------------------------- | ------------------------ |
| `method_tables`  | Type name | `IndexMap<String, Value>` | Instance method dispatch |
| `static_methods` | Type name | `IndexMap<String, Value>` | Static method dispatch   |

When a `give`/`impl` block is executed, each method is inserted into `method_tables` under the type name. Methods without an `it` parameter are additionally inserted into `static_methods`.

## Method Resolution Order

When resolving `obj.method(args)` on a typed object:

1. **Direct field** — If the object has a field named `method` that is callable, it is invoked.
2. **Method table** — The runtime looks up `method_tables[obj.__type__][method]`.
3. **Embedded delegation** — If not found, the runtime checks each embedded field's type for the method in `method_tables`. See [Composition](composition.md).
4. **Known builtins** — Certain method names (e.g., `map`, `filter`, `push`) are recognized as builtin functions and dispatched accordingly.
5. **Error** — If no match is found, a runtime error is raised: `no method 'method' on TypeName`.

## Mixed Syntax

Natural (`define`) and classic (`fn`) function syntax may be used interchangeably within `give` and `impl` blocks:

```forge
give Greeter {
    define hello(it) {
        say "hi from " + it.name
    }

    fn goodbye(it) {
        say "bye from " + it.name
    }
}
```
