# env

Environment variable access.

## Functions

### env.get(key, default?) -> string | null

Returns the value of the environment variable `key`. Returns `null` if the variable is not set, or `default` if provided.

```forge
let home = env.get("HOME")
say home  // "/Users/alice"

let port = env.get("PORT", "8080")
say port  // "8080" if PORT is not set
```

### env.set(key, value) -> null

Sets the environment variable `key` to `value` for the current process.

```forge
env.set("APP_MODE", "production")
```

### env.has(key) -> bool

Returns `true` if the environment variable `key` is set.

```forge
if env.has("DATABASE_URL") {
    say "Database configured"
}
```

### env.keys() -> array

Returns an array of all environment variable names.

```forge
let all_keys = env.keys()
say len(all_keys)
```
