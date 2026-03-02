# Interface Contracts

Interfaces define behavioral contracts — a set of method signatures that a type must implement. Forge provides dual syntax: `power` (natural) and `interface` (classic). Implementing an interface is verified at definition time when the `give...the power` or `impl...for` block is executed.

## Syntax

```
InterfaceDef     = ("power" | "interface") Identifier "{" MethodSig* "}"
MethodSig        = ("fn" | "define") Identifier "(" ParamList ")" ("->" TypeAnnotation)?
ImplInterface    = "give" TypeName "the" "power" InterfaceName "{" MethodDef* "}"
                 | "impl" InterfaceName "for" TypeName "{" MethodDef* "}"
```

## Defining an Interface

The `power` keyword (or its classic alias `interface`) introduces a named interface. An interface body contains method signatures — method names with parameter lists and optional return type annotations.

```forge
power Greetable {
    fn greet(it) -> String
}
```

Classic syntax:

```forge
interface Greetable {
    fn greet(it) -> String
}
```

Both forms produce the same `InterfaceDef` AST node.

## Interface Registration

When an `InterfaceDef` statement is executed, the interpreter:

1. Builds an array of method specification objects. Each object contains:
   - `name` — the method name as a `String`.
   - `param_count` — the number of parameters as an `Int`.
   - `return_type` — the return type annotation as a `String`, if present.
2. Creates an interface metadata object with fields `__kind__: "interface"`, `name`, and `methods`.
3. Registers the interface in the environment under both its name and `__interface_Name__`.

The interface object is a regular `Object` value and can be passed to functions like `satisfies`.

## Implementing an Interface

To declare that a type fulfills an interface, use `give...the power` (natural) or `impl...for` (classic):

```forge
thing Cat {
    name: String
}

power Greetable {
    fn greet(it) -> String
}

give Cat the power Greetable {
    define greet(it) {
        return "Meow, I'm " + it.name
    }
}
```

Classic syntax:

```forge
interface Named {
    fn get_name(it) -> String
}

impl Named for Animal {
    fn get_name(it) {
        return it.name
    }
}
```

## Validation at Definition Time

When a `give...the power` or `impl...for` block is executed, the runtime validates that every method required by the interface is present in the type's method table (including methods added by the current block and any previous `give`/`impl` blocks).

The validation checks method presence by name. It does not verify parameter counts, parameter types, or return types.

If a required method is missing, a runtime error is raised:

```
'Cat' does not implement 'greet' required by power 'Greetable'
```

This error occurs at the point where the `give...the power` block is executed, not at a later call site.

## Interface Without Explicit Implementation

A type may satisfy an interface without ever using `give...the power` or `impl...for`. Forge supports Go-style structural typing through the `satisfies` function. See [Structural Satisfaction](structural-satisfaction.md).

The `give...the power` syntax provides two benefits over implicit satisfaction:

1. **Early validation** — Errors are reported at the implementation site rather than at a distant call site.
2. **Documentation** — The code explicitly declares the relationship between a type and an interface.

## Multiple Interfaces

A type may implement multiple interfaces through separate `give...the power` blocks:

```forge
power Speakable {
    fn speak(it) -> String
}

power Trainable {
    fn train(it, command: String)
    fn obey(it, command: String) -> Bool
}

thing Dog {
    name: String
}

give Dog the power Speakable {
    define speak(it) {
        return "Woof!"
    }
}

give Dog the power Trainable {
    define train(it, command) {
        say it.name + " is learning " + command
    }

    define obey(it, command) {
        return true
    }
}
```

Each `give...the power` block independently validates that its interface's requirements are met. Methods from earlier blocks (including plain `give` blocks without an interface) count toward satisfaction.

## Return Type Annotations

Return type annotations in interface method signatures are stored in the interface metadata but are not enforced at runtime. They serve as documentation:

```forge
power Hashable {
    fn hash(it) -> String
}
```

The `-> String` annotation is recorded in the method specification but the runtime does not check that `hash` actually returns a `String`.
