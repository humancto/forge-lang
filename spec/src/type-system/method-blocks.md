# Method Blocks

Method blocks attach functions to a struct type after its definition. Forge provides dual syntax: `give` (natural) and `impl` (classic). Methods are stored in the interpreter's method tables and dispatched based on the instance's `__type__` field.

## Syntax

```
MethodBlock      = GiveBlock | ImplBlock
GiveBlock        = "give" TypeName AbilityClause? "{" MethodDef* "}"
ImplBlock        = "impl" (TypeName | AbilityForType) "{" MethodDef* "}"
AbilityClause    = "the" "power" InterfaceName
AbilityForType   = InterfaceName "for" TypeName
MethodDef        = ("fn" | "define") Identifier "(" MethodParams ")" Block
MethodParams     = ["mut"] Receiver ("," Param)* | ParamList
Receiver         = "it" | "self"
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

## Receivers Are Read-Only; `mut it` Updates Them

Forge values are copied, so `it` is the method's own copy of the receiver.
A plain receiver is **immutable**: assigning to `it`, to one of its fields or
elements (`it.n += 1`), or calling a mutating method on a part of it
(`it.items.push(x)`) is the immutable-assignment error (E0006 at run time,
T0017 from the type checker). This way an update can never be silently lost.

A method that updates its receiver declares it `mut it`. When such a method
returns normally (an explicit `return`, its last expression, or a `?` early
return), the final value of `it` is written back to the place the method was
called on, as if by an assignment:

```forge
struct Counter {
    n: Int
}

impl Counter {
    fn inc(mut it) {
        it.n += 1
        return it.n
    }
}

let mut c = Counter { n: 0 }
c.inc()                        // c.n == 1
let mut cs = [Counter { n: 0 }]
cs[0].inc()                    // elements and fields are places too
let mut o = { c: Counter { n: 5 } }
o.c.inc()                      // o.c.n == 6
```

Rules (both engines):

- `mut` is allowed only on a method's receiver: the first parameter of a
  method in a `give`/`impl` block, named `it` (or `self`). `mut` on any other
  parameter is a syntax error.
- The receiver place is a variable or a chain of fields and indexes rooted at
  one. Its index expressions are evaluated once, before the arguments; the
  receiver is read before the arguments too (for `push`/`pop`/`add`/`remove`
  on a plain variable, after them, like the collection methods).
- Writing back is an assignment to the place: on an immutable (`let`) root it
  is E0006 (`cannot reassign immutable variable`), reported statically as
  T0017. It happens after the method returns, so the method's own effects
  (output) have happened.
- A `mut it` method called on a value that is not a place (`make().inc()`,
  `a.b().inc()`, a literal) raises E0006 `cannot update a temporary value`
  (T0017 statically): the update would have nowhere to go.
- If the method raises an error, nothing is written back.
- Aliases do not observe the update (`let w = c; c.inc()` leaves `w`
  unchanged): only the place the method was called on changes.
- The same holds for methods of algebraic `type`s (`it = Green` replaces the
  receiver), for methods called through an interface-typed value, and for a
  method found on an embedded field (`outer.hit()` updates `outer.inner`).
- A struct's own method named like a collection method (`push`, `pop`,
  `add`, `remove`) is called like any other method: with `mut it` it updates
  the receiver, without it the variable is unchanged.

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
