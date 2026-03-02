# Structural Satisfaction

The `satisfies` function tests whether a value's type fulfills an interface's requirements at runtime, without requiring an explicit `give...the power` or `impl...for` declaration. This is Forge's implementation of Go-style structural typing: if a type has the right methods, it satisfies the interface.

## Syntax

```
satisfies(value, InterfaceObject) -> Bool
```

The `satisfies` function is a builtin that takes two arguments:

1. **value** — Any value, typically a struct instance (an object with a `__type__` field).
2. **InterfaceObject** — An interface value (an object with `__kind__: "interface"` and a `methods` array).

It returns `true` if the value's type has all methods required by the interface, `false` otherwise.

## Basic Usage

```forge
thing Robot {
    name: String
}

power Speakable {
    fn speak(it) -> String
}

give Robot {
    define speak(it) {
        return "Beep boop, I am " + it.name
    }
}

let r = Robot { name: "R2D2" }
satisfies(r, Speakable)    // true
```

Note that `Robot` never explicitly declared `give Robot the power Speakable`. The `satisfies` check passes because `Robot` has a `speak` method in its method table.

## Resolution Algorithm

The `satisfies` function checks interface satisfaction in two phases:

### Phase 1: Structural Check

First, `satisfies` performs a structural check on the value itself. It examines whether the value (or the object's fields) contains callable values matching each required method name. This handles objects that carry their methods as fields.

### Phase 2: Method Table Check

If the structural check fails and the value is an object with a `__type__` field, `satisfies` looks up the type name in the interpreter's `method_tables`. For each method required by the interface, it checks whether the method table contains an entry with that name.

```forge
thing Printer {}

give Printer {
    define print_line(it, text) {
        say text
    }
}

power Printable {
    fn print_line(it, text: String)
}

let p = Printer {}
satisfies(p, Printable)    // true (found in method_tables)
```

### Satisfaction Criteria

The check verifies method presence by **name only**. It does not verify:

- Parameter count or parameter types
- Return types
- Method body or behavior

A type satisfies an interface if and only if every method name listed in the interface's `methods` array has a corresponding entry in the type's method table.

## Explicit vs. Structural

Forge supports both explicit and structural interface satisfaction:

| Approach   | Syntax                       | When Checked       |
| ---------- | ---------------------------- | ------------------ |
| Explicit   | `give T the power I { ... }` | At definition time |
| Structural | `satisfies(value, I)`        | At call time       |

Explicit implementation triggers immediate validation and produces clear error messages at the definition site. Structural satisfaction is more flexible but defers errors to the point where `satisfies` is called.

Both approaches can coexist. A type that explicitly implements an interface will also pass `satisfies` checks.

## Examples

### Satisfied Without Explicit Declaration

```forge
thing Duck {
    name: String
}

power Quackable {
    fn quack(it) -> String
}

give Duck {
    define quack(it) {
        return it.name + " says quack!"
    }
}

let d = Duck { name: "Donald" }
satisfies(d, Quackable)    // true — Duck has quack()
```

### Not Satisfied

```forge
thing Rock {
    weight: Int
}

satisfies(Rock { weight: 5 }, Quackable)    // false — Rock has no quack()
```

### Multiple Interface Methods

```forge
power Serializable {
    fn to_string(it) -> String
    fn to_json(it) -> String
}

thing Config {
    data: String
}

give Config {
    define to_string(it) { return it.data }
    // Missing to_json
}

let c = Config { data: "test" }
satisfies(c, Serializable)    // false — missing to_json
```

## Return Value

`satisfies` always returns a `Bool` value. It never throws an error for non-matching types; it simply returns `false`. It only raises a runtime error if called with the wrong number of arguments.
