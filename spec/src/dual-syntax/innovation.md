# Innovation Keywords

Keywords unique to Forge that have no direct equivalent in other mainstream languages. These are not aliases -- they introduce genuinely new constructs.

## when Guards

Pattern matching with comparison guards. Unlike `match`, `when` tests a single value against comparison operators.

```forge
let score = 85
let grade = when score {
    >= 90 -> "A",
    >= 80 -> "B",
    >= 70 -> "C",
    >= 60 -> "D",
    else -> "F"
}
say grade  // "B"
```

The `else` arm is required and handles any unmatched case.

## must

Unwraps a Result or crashes with a clear error message. Used when failure is unrecoverable.

```forge
let data = must fs.read("config.json")
// If the file doesn't exist, the program crashes with a descriptive error
```

## safe { }

Null-safe execution block. If any expression inside `safe` would error, the block evaluates to `null` instead of crashing. Statement-only (cannot be used as an expression).

```forge
safe {
    let data = fs.read("maybe-missing.txt")
    say data
}
// If file is missing, execution continues silently
```

## check ... is not empty

Declarative validation. Checks a condition and produces a validation error if it fails.

```forge
check name is not empty
check age >= 0
check email contains "@"
```

## retry N times { }

Automatically retries a block up to N times on failure.

```forge
retry 3 times {
    let resp = http.get("https://flaky-api.example.com/data")
    say resp.json
}
```

If all retries fail, the error from the last attempt is raised.

## timeout N seconds { }

Limits execution time for a block. If the block does not complete within the time limit, it is interrupted.

```forge
timeout 5 seconds {
    let result = http.get("https://slow-api.example.com")
    say result.json
}
```

> **Note:** This feature is experimental and may not interrupt all operations cleanly.

## schedule every N units { }

Runs a block repeatedly on a schedule (cron-like).

```forge
schedule every 5 minutes {
    let status = http.get("https://api.example.com/health")
    if status.status != 200 {
        log.error("Health check failed!")
    }
}
```

Supported units: `seconds`, `minutes`, `hours`.

## watch "path" { }

Monitors a file or directory for changes and executes the block when changes are detected.

```forge
watch "src/" {
    say "Files changed, rebuilding..."
    sh("cargo build")
}
```

## ask "prompt"

Sends a prompt to an AI/LLM and returns the response. Requires AI configuration.

```forge
let answer = ask "What is the capital of France?"
say answer  // "Paris"
```

## download "url" to "file"

Downloads a file from a URL and saves it to disk. Syntax sugar for `http.download`.

```forge
download "https://example.com/data.csv" to "data.csv"
```

## crawl "url"

Fetches and parses a web page, returning structured data. Syntax sugar for `http.crawl`.

```forge
let page = crawl "https://example.com"
say page.title
say page.links
```

## repeat N times { }

Executes a block exactly N times. A counted loop without a loop variable.

```forge
repeat 5 times {
    say "Hello!"
}
```

## wait N units

Pauses execution for the specified duration.

```forge
wait 2 seconds
wait 500 milliseconds
```

Supported units: `seconds`, `milliseconds`, `minutes`.

## grab ... from "url"

Natural syntax for HTTP fetch. Assigns the response to a variable.

```forge
grab data from "https://api.example.com/users"
say data
```

## emit value

Yields a value from a generator function. Natural equivalent of `yield`.

```forge
fn fibonacci() {
    let a = 0
    let b = 1
    loop {
        emit a
        let temp = a + b
        a = b
        b = temp
    }
}
```

## hold expr

Awaits an async expression. Natural equivalent of `await`.

```forge
forge fetch_data() {
    let resp = hold http.get("https://api.example.com")
    return resp.json
}
```
