# Loops

Loop statements execute a body repeatedly. Forge provides several loop forms for different iteration patterns.

## For-In Loops

The `for`-`in` loop iterates over elements of a collection.

```forge
let names = ["Alice", "Bob", "Charlie"]
for name in names {
    say "Hello, {name}"
}
```

### For Each Variant

The `for each` form is equivalent to `for`-`in`:

```forge
for each name in names {
    say "Hello, {name}"
}
```

### Iterating Over Arrays

```forge
let nums = [10, 20, 30]
for n in nums {
    say n
}
// Output: 10, 20, 30
```

### Iterating Over Objects

When iterating over an object, the loop variable receives each key:

```forge
let user = { name: "Alice", age: 30 }
for key in user {
    say "{key}: {user[key]}"
}
```

### Iterating with Index

Use `enumerate` or a two-variable for loop to access both index and value:

```forge
let items = ["a", "b", "c"]
for i, item in items {
    say "{i}: {item}"
}
// Output: 0: a, 1: b, 2: c
```

### Iterating Over Ranges

The `range` function generates a sequence of integers:

```forge
for i in range(0, 5) {
    say i
}
// Output: 0, 1, 2, 3, 4
```

`range(start, end)` produces integers from `start` (inclusive) to `end` (exclusive).

## While Loops

The `while` loop executes as long as its condition is truthy.

```forge
let mut count = 0
while count < 5 {
    say count
    count += 1
}
// Output: 0, 1, 2, 3, 4
```

The condition is evaluated before each iteration. If the condition is falsy on the first check, the body never executes.

## Loop (Infinite)

The `loop` keyword creates an infinite loop. Use `break` to exit.

```forge
let mut n = 0
loop {
    if n >= 3 {
        break
    }
    say n
    n += 1
}
// Output: 0, 1, 2
```

## Repeat N Times

The `repeat` loop executes a body a fixed number of times.

```forge
repeat 3 times {
    say "hello"
}
// Output: hello, hello, hello
```

The count expression is evaluated once before the loop begins. The body executes exactly that many times.

## Break

The `break` statement exits the innermost enclosing loop immediately.

```forge
for i in range(0, 100) {
    if i == 5 {
        break
    }
    say i
}
// Output: 0, 1, 2, 3, 4
```

`break` can be used with `for`, `while`, `loop`, and `repeat`.

## Continue

The `continue` statement skips the rest of the current iteration and proceeds to the next one.

```forge
for i in range(0, 5) {
    if i == 2 {
        continue
    }
    say i
}
// Output: 0, 1, 3, 4
```

## Nested Loops

Loops can be nested. `break` and `continue` apply to the innermost loop only.

```forge
for i in range(0, 3) {
    for j in range(0, 3) {
        if j == 1 {
            break       // exits inner loop only
        }
        say "{i},{j}"
    }
}
// Output: 0,0  1,0  2,0
```

## Loop Scope

Variables declared inside a loop body are scoped to each iteration:

```forge
for i in range(0, 3) {
    let msg = "iteration {i}"
    say msg
}
// msg is not accessible here
```

The loop variable itself (`i` in `for i in ...`) is scoped to the loop body.

## Wait in Loops

The `wait` statement can be used inside loops to introduce delays:

```forge
repeat 3 times {
    say "tick"
    wait 1 second
}
```
