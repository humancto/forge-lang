# Execution Helpers

Higher-order functions for profiling, error handling, and benchmarking.

## cook(fn) -> any

Executes `fn`, measures its execution time, and prints a performance report to stderr. Returns the function's return value.

```forge
let result = cook(fn() {
    let total = 0
    for i in range(0, 1000000) {
        total = total + i
    }
    return total
})
// stderr: 👨‍🍳 COOKED: done in 42.15ms — it's giving adequate
// result contains the computed total
```

Performance messages vary by duration:

- Under 1ms: "speed demon fr"
- 1-100ms: "no cap that was fast"
- 100ms-1s: "it's giving adequate"
- Over 1s: "bruh that took a minute"

## yolo(fn) -> any | None

Executes `fn` and swallows all errors. Returns the function's result on success, or `None` on failure. Useful for non-critical operations where errors can be safely ignored.

```forge
let data = yolo(fn() {
    return fs.read("maybe-missing.txt")
})
// data is the file contents or None

if is_none(data) {
    say "File not found, using defaults"
}
```

## ghost(fn) -> any

Executes `fn` silently. The function runs normally and its return value is passed through, but intended for cases where you want to suppress side effects.

```forge
let result = ghost(fn() {
    return compute_something()
})
```

## slay(fn, iterations?) -> object

Benchmarks `fn` by running it `iterations` times (default: 100). Prints a summary to stderr and returns a statistics object.

**Returns:**

| Field    | Type    | Description                                |
| -------- | ------- | ------------------------------------------ |
| `avg_ms` | `float` | Average time per iteration in milliseconds |
| `min_ms` | `float` | Minimum time in milliseconds               |
| `max_ms` | `float` | Maximum time in milliseconds               |
| `p99_ms` | `float` | 99th percentile time in milliseconds       |
| `runs`   | `int`   | Number of iterations                       |
| `result` | `any`   | Return value of the last iteration         |

```forge
let stats = slay(fn() {
    return math.pow(2, 20)
}, 1000)

// stderr: 💅 SLAYED: 1000x runs — avg 0.003ms, min 0.001ms, max 0.012ms, p99 0.008ms

say stats.avg_ms   // 0.003
say stats.runs     // 1000
say stats.result   // 1048576
```
