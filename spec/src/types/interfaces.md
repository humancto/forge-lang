# Interface Types

An interface (or "power" in natural syntax) defines a **contract**: a set of method signatures that a type must provide. Interfaces enable polymorphism in Forge — any type that satisfies an interface's contract can be used wherever that interface is expected.

## Definition

Interfaces are defined with the `interface` keyword (classic) or the `power` keyword (natural).

> _InterfaceDef_ → ( `interface` | `power` ) _Identifier_ `{` _MethodSignature_\* `}`
>
> _MethodSignature_ → `fn` _Identifier_ `(` _ParamList_? `)` ( `->` _Type_ )?

```forge
// Classic syntax
interface Describable {
    fn describe() -> String
}

// Natural syntax — identical result
power Describable {
    fn describe() -> String
}
```

An interface body contains one or more method signatures. Each signature specifies the method name, parameter types, and optional return type. No method body is provided — interfaces declare _what_ methods must exist, not _how_ they work.

## Implementing an Interface

A type satisfies an interface by providing implementations of all required methods. This is done using `give ... the power ...` (natural) or `impl ... for ...` (classic).

```forge
thing Person {
    name: String,
    age: Int
}

// Natural syntax
give Person the power Describable {
    define describe(it) {
        return it.name + " (" + str(it.age) + ")"
    }
}

// Classic syntax — identical result
impl Describable for Person {
    fn describe(it) {
        return it.name + " (" + str(it.age) + ")"
    }
}
```

The implementation block must provide a method for every signature in the interface. Missing methods produce a compile-time (definition-time) error.

## Checking Satisfaction

The `satisfies()` built-in function checks whether a value's type satisfies a given interface at runtime:

```forge
set alice to craft Person { name: "Alice", age: 30 }
say satisfies(alice, Describable)   // true
```

If a type has not implemented the interface, `satisfies()` returns `false`:

```forge
thing Robot {
    id: Int
}

set r to craft Robot { id: 1 }
say satisfies(r, Describable)   // false
```

## Multiple Interfaces

A single type may implement multiple interfaces:

```forge
power Describable {
    fn describe() -> String
}

power Vocal {
    fn speak() -> String
}

thing Animal {
    species: String,
    sound: String
}

give Animal the power Describable {
    define describe(it) {
        return it.species + " that says " + it.sound
    }
}

give Animal the power Vocal {
    define speak(it) {
        return it.sound + "! " + it.sound + "!"
    }
}
```

Each interface implementation is provided in a separate `give...the power` block. A type accumulates all its interface implementations.

## No Default Implementations

In the current version of Forge, interfaces cannot provide default method implementations. Every method in an interface must be explicitly implemented by each type that satisfies it.

## Interface Inheritance

Interface inheritance (one interface extending another) is not supported in the current version. Each interface is independent.

## Structural vs. Nominal

Forge uses a **nominal** approach to interface satisfaction: a type satisfies an interface only if it has been explicitly declared to do so via a `give...the power` or `impl...for` block. Simply having methods with matching names and signatures is not sufficient.

However, the `satisfies()` built-in performs a **structural check** at runtime — it verifies that the required methods actually exist on the value. This means satisfaction is ultimately determined by the presence of the declared methods, but the declaration is required to register the type-interface relationship.
