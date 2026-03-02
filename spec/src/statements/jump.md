# Return, Break, Continue

Jump statements transfer control to a different point in the program, interrupting the normal top-to-bottom execution flow.

## Return

The `return` statement exits the current function and optionally provides a return value.

### Return with a Value

```forge
fn square(x) {
    return x * x
}
say square(5)   // 25
```

### Return without a Value

A bare `return` returns `null` from the function:

```forge
fn maybe_log(x) {
    if x <= 0 {
        return          // returns null
    }
    say "value: {x}"
}
```

### Implicit Return

The last expression in a function body is automatically used as the return value. Explicit `return` is only needed for early exit.

```forge
fn double(x) {
    x * 2           // implicit return
}
say double(5)   // 10
```

When using `if`-`else` as the last statement, the last expression in the executed branch becomes the return value:

```forge
fn abs(x) {
    if x < 0 {
        -x
    } else {
        x
    }
}
say abs(-3)     // 3
```

### Return in Nested Blocks

`return` exits the enclosing **function**, not just the current block:

```forge
fn find_first_negative(items) {
    for item in items {
        if item < 0 {
            return item     // exits the function, not just the loop
        }
    }
    null
}
say find_first_negative([1, 2, -3, 4])  // -3
```

### Return from Closures

A `return` inside a closure exits the **closure**, not the outer function:

```forge
fn process(items) {
    let results = map(items, fn(x) {
        if x < 0 {
            return 0        // exits the closure, not process()
        }
        x * 2
    })
    results
}
say process([1, -2, 3])    // [2, 0, 6]
```

## Break

The `break` statement exits the innermost enclosing loop immediately. Execution continues with the first statement after the loop.

### Break in For Loops

```forge
for i in range(0, 100) {
    if i >= 5 {
        break
    }
    say i
}
// Output: 0, 1, 2, 3, 4
say "done"
```

### Break in While Loops

```forge
let mut n = 0
while true {
    if n >= 3 {
        break
    }
    say n
    n += 1
}
// Output: 0, 1, 2
```

### Break in Loop

The `break` statement is the only way to exit a `loop` (infinite loop):

```forge
let mut count = 0
loop {
    count += 1
    if count > 5 {
        break
    }
}
say count   // 6
```

### Break in Nested Loops

`break` only exits the innermost loop:

```forge
for i in range(0, 3) {
    for j in range(0, 10) {
        if j >= 2 {
            break       // exits inner loop only
        }
        say "{i},{j}"
    }
    // continues with next i
}
```

### Break Outside a Loop

Using `break` outside of a loop produces a runtime error.

## Continue

The `continue` statement skips the rest of the current loop iteration and proceeds to the next iteration.

### Continue in For Loops

```forge
for i in range(0, 5) {
    if i == 2 {
        continue        // skip i == 2
    }
    say i
}
// Output: 0, 1, 3, 4
```

### Continue in While Loops

```forge
let mut i = 0
while i < 5 {
    i += 1
    if i % 2 == 0 {
        continue        // skip even numbers
    }
    say i
}
// Output: 1, 3, 5
```

### Continue in Nested Loops

Like `break`, `continue` applies to the innermost loop:

```forge
for i in range(0, 3) {
    for j in range(0, 3) {
        if j == 1 {
            continue    // skips j == 1 in inner loop
        }
        say "{i},{j}"
    }
}
// Output: 0,0  0,2  1,0  1,2  2,0  2,2
```

### Continue Outside a Loop

Using `continue` outside of a loop produces a runtime error.

## Summary

| Statement     | Context  | Effect                                   |
| ------------- | -------- | ---------------------------------------- |
| `return expr` | Function | Exit function, return `expr`             |
| `return`      | Function | Exit function, return `null`             |
| `break`       | Loop     | Exit innermost loop                      |
| `continue`    | Loop     | Skip to next iteration of innermost loop |
