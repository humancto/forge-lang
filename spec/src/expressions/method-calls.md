# Method Calls

A method call uses dot notation to invoke a function on a receiver value. Forge resolves method calls through a multi-step dispatch process.

## Syntax

```
expression.method(arguments)
```

The left operand (the **receiver**) is evaluated first, then the method is resolved, then arguments are evaluated left to right.

## Method Dispatch

When evaluating `obj.method(args)`, the interpreter follows this resolution order:

### 1. Object Field Lookup

If the receiver is an object and has a field named `method` whose value is callable (a function or closure), that field is invoked directly.

```forge
let obj = {
    greet: fn(name) { "hello, {name}" }
}
say obj.greet("world")      // "hello, world"
```

### 2. Static Method Lookup

If the receiver is a struct type reference (not an instance), static methods registered via `give`/`impl` blocks are checked.

```forge
thing Counter { value: int }

give Counter {
    fn new() {
        Counter { value: 0 }
    }
}

let c = Counter.new()   // static method call
```

### 3. Instance Method Lookup (method_tables)

If the receiver is a typed object (has a `__type__` field), the interpreter looks up the type name in the global method table. Methods registered via `give`/`impl` blocks are found here. The receiver is automatically passed as the first argument (`self`).

```forge
thing Circle { radius: float }

give Circle {
    fn area(self) {
        3.14159 * self.radius * self.radius
    }
}

let c = Circle { radius: 5.0 }
say c.area()    // 78.53975
```

### 4. Embedded Field Delegation

If the receiver is a typed object and no method is found in step 3, the interpreter checks embedded fields (declared with `has`). For each embedded field, the interpreter looks up the embedded type's method table. If a match is found, the embedded sub-object is passed as `self` instead of the outer object.

```forge
thing Animal { name: string }
give Animal {
    fn speak(self) { "{self.name} speaks" }
}

thing Pet {
    has animal: Animal
    owner: string
}

let p = Pet { animal: Animal { name: "Rex" }, owner: "Alice" }
say p.speak()   // "Rex speaks" (delegated to Animal.speak)
```

### 5. Built-in String Methods

Strings have a small set of built-in methods that are resolved directly without going through the method table:

| Method     | Return Type | Description                       |
| ---------- | ----------- | --------------------------------- |
| `.upper()` | `string`    | Uppercase copy                    |
| `.lower()` | `string`    | Lowercase copy                    |
| `.trim()`  | `string`    | Trimmed copy                      |
| `.len()`   | `int`       | Byte length                       |
| `.chars()` | `array`     | Array of single-character strings |

```forge
say "hello".upper()     // "HELLO"
say "  hi  ".trim()     // "hi"
say "abc".chars()       // ["a", "b", "c"]
```

### 6. Known Built-in Methods

If none of the above steps match, Forge checks a set of known built-in method names. If the method name matches, the call is rewritten as a function call with the receiver prepended to the argument list: `obj.method(args)` becomes `method(obj, args)`.

This allows calling built-in functions with method syntax:

```forge
let nums = [3, 1, 2]
say nums.sort()             // [1, 2, 3]
say nums.map(fn(x) { x * 2 })  // [6, 2, 4]
say nums.filter(fn(x) { x > 1 })   // [3, 2]

let text = "hello world"
say text.split(" ")        // ["hello", "world"]
say text.starts_with("hello")  // true
```

The full list of known built-in methods includes: `map`, `filter`, `reduce`, `sort`, `reverse`, `push`, `pop`, `len`, `contains`, `keys`, `values`, `enumerate`, `split`, `join`, `replace`, `find`, `flat_map`, `has_key`, `get`, `pick`, `omit`, `merge`, `entries`, `from_entries`, `starts_with`, `ends_with`, `upper`, `lower`, `trim`, `substring`, `index_of`, `last_index_of`, `pad_start`, `pad_end`, `capitalize`, `title`, `repeat_str`, `count`, `sum`, `min_of`, `max_of`, `any`, `all`, `unique`, `zip`, `flatten`, `group_by`, `chunk`, `slice`, `slugify`, `snake_case`, `camel_case`, `sample`, `shuffle`, `partition`, `diff`.

## Self Parameter

Methods defined in `give`/`impl` blocks receive the receiver as their first argument, conventionally named `self`. This parameter must be declared explicitly.

```forge
thing Rect { w: int, h: int }

give Rect {
    fn area(self) {
        self.w * self.h
    }
    fn scale(self, factor) {
        Rect { w: self.w * factor, h: self.h * factor }
    }
}
```

## Method Chaining

Method calls can be chained. Each call in the chain returns a value that becomes the receiver for the next call.

```forge
let result = [5, 2, 8, 1, 9]
    .sort()
    .filter(fn(x) { x > 3 })
    .map(fn(x) { x * 10 })
// result: [50, 80, 90]
```

## Resolution Failure

If method resolution exhausts all steps without finding a match, a runtime error is produced:

```forge
let x = 42
say x.nonexistent()     // runtime error: unknown method 'nonexistent'
```
