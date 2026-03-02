# PART II: THE STANDARD LIBRARY

---

Forge ships with fifteen built-in modules that cover the tasks programmers encounter daily—mathematics, file I/O, cryptography, databases, serialization, and terminal presentation. These modules require no imports; they are available the moment your program starts. You access them through dot notation (`module.function()`), and they follow consistent conventions: functions that can fail return meaningful error messages, types are coerced sensibly, and side effects are kept explicit.

Part II is both a reference and a cookbook. Each chapter documents every function a module offers, then closes with recipes that combine those functions into real-world patterns. Read the chapters front to back when learning a module, or jump straight to the reference tables when you need a reminder.

---

## Chapter 10: math — Numbers and Computation

Mathematics is the bedrock of programming, and Forge's `math` module provides the essential toolkit: constants, arithmetic helpers, trigonometric functions, and random number generation. Every function in the module accepts both `Int` and `Float` arguments, coercing integers to floating-point where the result demands it. The module covers the same ground as a scientific calculator—enough to build simulations, games, data analysis pipelines, and engineering tools without reaching for an external library.

### Constants

The `math` module exposes three constants as properties, not functions. Access them directly.

| Constant   | Value               | Description                                                  |
| ---------- | ------------------- | ------------------------------------------------------------ |
| `math.pi`  | `3.141592653589793` | The ratio of a circle's circumference to its diameter (π)    |
| `math.e`   | `2.718281828459045` | Euler's number, the base of natural logarithms               |
| `math.inf` | `Infinity`          | Positive infinity, useful for comparisons and initial bounds |

```forge
let pi = math.pi
let e = math.e
let inf = math.inf
say "π = {pi}"
say "e = {e}"
say "∞ = {inf}"
```

Output:

```
π = 3.141592653589793
e = 2.718281828459045
∞ = inf
```

### Function Reference

| Function           | Description                   | Example                       | Return Type  |
| ------------------ | ----------------------------- | ----------------------------- | ------------ |
| `math.sqrt(n)`     | Square root                   | `math.sqrt(144)` → `12.0`     | Float        |
| `math.pow(b, exp)` | Raise `b` to the power `exp`  | `math.pow(2, 10)` → `1024`    | Int or Float |
| `math.abs(n)`      | Absolute value                | `math.abs(-42)` → `42`        | Int or Float |
| `math.max(a, b)`   | Larger of two values          | `math.max(3, 7)` → `7`        | Int or Float |
| `math.min(a, b)`   | Smaller of two values         | `math.min(3, 7)` → `3`        | Int or Float |
| `math.floor(n)`    | Round down to nearest integer | `math.floor(9.7)` → `9`       | Int          |
| `math.ceil(n)`     | Round up to nearest integer   | `math.ceil(9.2)` → `10`       | Int          |
| `math.round(n)`    | Round to nearest integer      | `math.round(9.5)` → `10`      | Int          |
| `math.random()`    | Pseudorandom float in [0, 1)  | `math.random()` → `0.7382...` | Float        |
| `math.sin(n)`      | Sine (radians)                | `math.sin(0)` → `0.0`         | Float        |
| `math.cos(n)`      | Cosine (radians)              | `math.cos(0)` → `1.0`         | Float        |
| `math.tan(n)`      | Tangent (radians)             | `math.tan(0)` → `0.0`         | Float        |
| `math.log(n)`      | Natural logarithm (base _e_)  | `math.log(1)` → `0.0`         | Float        |

> **Type Preservation.** Functions like `abs`, `max`, `min`, `pow` preserve the input type when both arguments are integers. Pass a float to force a float result: `math.pow(2.0, 10)` returns `1024.0`.

### Core Examples

**Square roots and powers:**

```forge
let hyp = math.sqrt(9.0 + 16.0)
say "Hypotenuse: {hyp}"

let kb = math.pow(2, 10)
say "1 KB = {kb} bytes"

let vol = math.pow(3.0, 3.0)
say "Volume of 3³ cube: {vol}"
```

Output:

```
Hypotenuse: 5.0
1 KB = 1024 bytes
Volume of 3³ cube: 27.0
```

**Rounding family:**

```forge
let price = 19.95
let floored = math.floor(price)
let ceiled = math.ceil(price)
let rounded = math.round(price)
say "floor({price}) = {floored}"
say "ceil({price}) = {ceiled}"
say "round({price}) = {rounded}"
```

Output:

```
floor(19.95) = 19
ceil(19.95) = 20
round(19.95) = 20
```

**Trigonometry:**

```forge
let angle = math.pi / 4.0
let s = math.sin(angle)
let c = math.cos(angle)
let t = math.tan(angle)
say "sin(π/4) = {s}"
say "cos(π/4) = {c}"
say "tan(π/4) = {t}"
```

Output:

```
sin(π/4) = 0.7071067811865476
cos(π/4) = 0.7071067811865476
tan(π/4) = 0.9999999999999999
```

**Absolute value and bounds:**

```forge
let delta = -17
let abs_delta = math.abs(delta)
say "Distance from zero: {abs_delta}"

let high = math.max(100, 250)
let low = math.min(100, 250)
say "Range: {low} to {high}"
```

Output:

```
Distance from zero: 17
Range: 100 to 250
```

**Natural logarithm:**

```forge
let ln2 = math.log(2)
let ln10 = math.log(10)
say "ln(2) = {ln2}"
say "ln(10) = {ln10}"

// log base 10 via change-of-base
let log10_of_1000 = math.log(1000) / math.log(10)
say "log₁₀(1000) = {log10_of_1000}"
```

Output:

```
ln(2) = 0.6931471805599453
ln(10) = 2.302585092994046
log₁₀(1000) = 2.9999999999999996
```

**Random numbers:**

```forge
let r1 = math.random()
let r2 = math.random()
say "Random 1: {r1}"
say "Random 2: {r2}"

// Random integer between 1 and 6 (dice roll)
let raw = math.random() * 6.0
let die = math.floor(raw) + 1
say "Dice roll: {die}"
```

> **Pseudorandomness.** `math.random()` uses system time nanoseconds as its seed. It is suitable for games, simulations, and sampling—not for cryptographic purposes. Use the `crypto` module when security matters.

### Recipes

**Recipe 9.1: Euclidean Distance**

Calculate the distance between two points in 2D space.

```forge
fn distance(x1, y1, x2, y2) {
    let dx = x2 - x1
    let dy = y2 - y1
    return math.sqrt(dx * dx + dy * dy)
}

let d = distance(0.0, 0.0, 3.0, 4.0)
say "Distance: {d}"

let d2 = distance(1.0, 2.0, 4.0, 6.0)
say "Distance: {d2}"
```

Output:

```
Distance: 5.0
Distance: 5.0
```

**Recipe 9.2: Degrees and Radians Conversion**

```forge
fn deg_to_rad(degrees) {
    return degrees * math.pi / 180.0
}

fn rad_to_deg(radians) {
    return radians * 180.0 / math.pi
}

let rad = deg_to_rad(90.0)
say "90° = {rad} radians"

let deg = rad_to_deg(math.pi)
say "π radians = {deg}°"

// Sine of 30 degrees
let angle = deg_to_rad(30.0)
let result = math.sin(angle)
say "sin(30°) = {result}"
```

Output:

```
90° = 1.5707963267948966 radians
π radians = 180.0°
sin(30°) = 0.49999999999999994
```

**Recipe 9.3: Random Number in a Range**

```forge
fn random_between(lo, hi) {
    let range = hi - lo
    let r = math.random() * range
    return math.floor(r) + lo
}

// Generate 5 random numbers between 10 and 50
repeat 5 times {
    let n = random_between(10, 50)
    say "Random: {n}"
}
```

**Recipe 9.4: Basic Statistics**

```forge
fn mean(values) {
    let mut sum = 0.0
    for v in values {
        sum = sum + v
    }
    return sum / len(values)
}

fn variance(values) {
    let avg = mean(values)
    let mut sum_sq = 0.0
    for v in values {
        let diff = v - avg
        sum_sq = sum_sq + diff * diff
    }
    return sum_sq / len(values)
}

fn std_dev(values) {
    return math.sqrt(variance(values))
}

let data = [4.0, 8.0, 15.0, 16.0, 23.0, 42.0]
let m = mean(data)
let sd = std_dev(data)
say "Mean: {m}"
say "Std Dev: {sd}"
```

Output:

```
Mean: 18.0
Std Dev: 12.396773926563296
```

---

## Chapter 11: fs — File System

The `fs` module gives Forge programs the ability to read, write, copy, rename, and inspect files and directories. It wraps the operating system's file APIs in a set of straightforward functions that accept string paths and return predictable results. Whether you are writing a quick script that processes a log file or building a tool that manages configuration across a project, `fs` is the module you will reach for first.

All path arguments are strings. Relative paths resolve from the working directory where `forge run` was invoked. Functions that modify the filesystem—`write`, `append`, `remove`, `mkdir`, `rename`, `copy`—perform their operation or return an error message; they never silently fail.

### Function Reference

| Function                     | Description                            | Example                                     | Return Type   |
| ---------------------------- | -------------------------------------- | ------------------------------------------- | ------------- |
| `fs.read(path)`              | Read entire file as a string           | `fs.read("data.txt")` → `"hello"`           | String        |
| `fs.write(path, content)`    | Write string to file (overwrites)      | `fs.write("out.txt", "data")`               | Null          |
| `fs.append(path, content)`   | Append string to file                  | `fs.append("log.txt", "entry\n")`           | Null          |
| `fs.exists(path)`            | Check if file or directory exists      | `fs.exists("config.json")` → `true`         | Bool          |
| `fs.size(path)`              | File size in bytes                     | `fs.size("photo.jpg")` → `204800`           | Int           |
| `fs.ext(path)`               | File extension without the dot         | `fs.ext("main.fg")` → `"fg"`                | String        |
| `fs.list(path)`              | List entries in a directory            | `fs.list("src/")` → `["main.rs", "lib.rs"]` | Array[String] |
| `fs.mkdir(path)`             | Create directory (and parents)         | `fs.mkdir("build/output")`                  | Null          |
| `fs.copy(src, dst)`          | Copy a file                            | `fs.copy("a.txt", "b.txt")` → `1024`        | Int           |
| `fs.rename(old, new)`        | Rename or move a file or directory     | `fs.rename("old.txt", "new.txt")`           | Null          |
| `fs.remove(path)`            | Delete a file or directory (recursive) | `fs.remove("temp/")`                        | Null          |
| `fs.read_json(path)`         | Read and parse a JSON file             | `fs.read_json("config.json")` → `{...}`     | Value         |
| `fs.write_json(path, value)` | Write a value as pretty-printed JSON   | `fs.write_json("out.json", data)`           | Null          |

> **Safety Note.** `fs.remove()` deletes directories recursively without confirmation. Always double-check your path, especially when it comes from user input.

### Core Examples

**Reading and writing text files:**

```forge
// Write a file
fs.write("/tmp/greeting.txt", "Hello from Forge!")

// Read it back
let content = fs.read("/tmp/greeting.txt")
say "File says: {content}"

// Append to it
fs.append("/tmp/greeting.txt", "\nSecond line.")
let updated = fs.read("/tmp/greeting.txt")
say "Updated:\n{updated}"
```

Output:

```
File says: Hello from Forge!
Updated:
Hello from Forge!
Second line.
```

**Checking existence and metadata:**

```forge
fs.write("/tmp/forge_meta.txt", "some data here")

let exists = fs.exists("/tmp/forge_meta.txt")
say "Exists: {exists}"

let bytes = fs.size("/tmp/forge_meta.txt")
say "Size: {bytes} bytes"

let extension = fs.ext("/tmp/forge_meta.txt")
say "Extension: {extension}"

fs.remove("/tmp/forge_meta.txt")
let gone = fs.exists("/tmp/forge_meta.txt")
say "After remove: {gone}"
```

Output:

```
Exists: true
Size: 14 bytes
Extension: txt
After remove: false
```

**Directory operations:**

```forge
// Create nested directories
fs.mkdir("/tmp/forge_project/src/modules")

// Write files into the structure
fs.write("/tmp/forge_project/src/main.fg", "say \"hello\"")
fs.write("/tmp/forge_project/src/utils.fg", "fn add(a, b) { return a + b }")

// List directory contents
let files = fs.list("/tmp/forge_project/src")
say "Source files: {files}"

// Clean up
fs.remove("/tmp/forge_project")
```

Output:

```
Source files: ["modules", "main.fg", "utils.fg"]
```

**Copying and renaming:**

```forge
fs.write("/tmp/original.txt", "important data")

// Copy creates a duplicate
let bytes_copied = fs.copy("/tmp/original.txt", "/tmp/backup.txt")
say "Copied {bytes_copied} bytes"

// Rename moves the file
fs.rename("/tmp/backup.txt", "/tmp/archive.txt")
let has_backup = fs.exists("/tmp/backup.txt")
let has_archive = fs.exists("/tmp/archive.txt")
say "backup.txt exists: {has_backup}"
say "archive.txt exists: {has_archive}"

// Clean up
fs.remove("/tmp/original.txt")
fs.remove("/tmp/archive.txt")
```

Output:

```
Copied 14 bytes
backup.txt exists: false
archive.txt exists: true
```

**JSON file round-trip:**

```forge
let config = {
    app_name: "Forge Demo",
    version: "1.0.0",
    features: ["logging", "metrics", "auth"],
    database: {
        host: "localhost",
        port: 5432
    }
}

// Write as pretty-printed JSON
fs.write_json("/tmp/config.json", config)

// Read it back as a Forge object
let loaded = fs.read_json("/tmp/config.json")
say "App: {loaded.app_name}"
say "DB port: {loaded.database.port}"

fs.remove("/tmp/config.json")
```

Output:

```
App: Forge Demo
DB port: 5432
```

> **JSON Round-Trip.** `fs.write_json` uses `json.pretty` internally, producing human-readable files with 2-space indentation. `fs.read_json` uses `json.parse`, converting JSON types to their Forge equivalents: objects, arrays, strings, integers, floats, booleans, and null.

### Recipes

**Recipe 10.1: Configuration File Manager**

```forge
fn load_config(path) {
    if fs.exists(path) {
        return fs.read_json(path)
    }
    // Return defaults
    return {
        log_level: "info",
        max_retries: 3,
        timeout: 30
    }
}

fn save_config(path, config) {
    fs.write_json(path, config)
}

let cfg = load_config("/tmp/app_config.json")
say "Log level: {cfg.log_level}"

// Save config for next run
save_config("/tmp/app_config.json", cfg)
fs.remove("/tmp/app_config.json")
```

**Recipe 10.2: Log Rotation**

```forge
fn rotate_logs(log_path, max_backups) {
    if fs.exists(log_path) == false {
        return null
    }

    // Shift existing backups: .3 → .4, .2 → .3, etc.
    let mut i = max_backups - 1
    for n in [3, 2, 1] {
        let older = "{log_path}.{n}"
        let newer_num = n + 1
        let newer = "{log_path}.{newer_num}"
        if fs.exists(older) {
            fs.rename(older, newer)
        }
    }

    // Current log becomes .1
    let backup = "{log_path}.1"
    fs.copy(log_path, backup)
    fs.write(log_path, "")
    say "Logs rotated"
}

// Demo
fs.write("/tmp/app.log", "line 1\nline 2\nline 3\n")
rotate_logs("/tmp/app.log", 4)

let current = fs.read("/tmp/app.log")
let has_backup = fs.exists("/tmp/app.log.1")
say "Current log empty: {current}"
say "Backup exists: {has_backup}"

// Clean up
fs.remove("/tmp/app.log")
fs.remove("/tmp/app.log.1")
```

**Recipe 10.3: Directory Tree Printer**

```forge
fn print_tree(path, prefix) {
    let entries = fs.list(path)
    let count = len(entries)
    let mut idx = 0
    for entry in entries {
        idx = idx + 1
        let is_last = idx == count
        let connector = "└── "
        if is_last == false {
            let connector = "├── "
        }
        say "{prefix}{connector}{entry}"

        let full = "{path}/{entry}"
        let ext = fs.ext(full)
        // If no extension, it might be a directory
        if ext == "" {
            let child_prefix = "{prefix}    "
            if is_last == false {
                let child_prefix = "{prefix}│   "
            }
            if fs.exists(full) {
                // Try listing it (will fail gracefully if it's a file)
                safe {
                    print_tree(full, child_prefix)
                }
            }
        }
    }
}

// Build a sample directory
fs.mkdir("/tmp/myproject/src")
fs.mkdir("/tmp/myproject/tests")
fs.write("/tmp/myproject/src/main.fg", "")
fs.write("/tmp/myproject/src/utils.fg", "")
fs.write("/tmp/myproject/tests/test_main.fg", "")
fs.write("/tmp/myproject/README.md", "")

say "myproject/"
print_tree("/tmp/myproject", "")

fs.remove("/tmp/myproject")
```

**Recipe 10.4: File Backup Script**

```forge
fn backup_file(source) {
    if fs.exists(source) == false {
        say "Source not found: {source}"
        return false
    }

    let ext = fs.ext(source)
    let backup_path = "{source}.bak"
    let bytes = fs.copy(source, backup_path)
    say "Backed up {source} ({bytes} bytes)"
    return true
}

// Create some test files
fs.write("/tmp/data1.txt", "important data file 1")
fs.write("/tmp/data2.txt", "important data file 2")

let files_to_backup = ["/tmp/data1.txt", "/tmp/data2.txt"]
for f in files_to_backup {
    backup_file(f)
}

// Clean up
for f in files_to_backup {
    fs.remove(f)
    let bak = "{f}.bak"
    fs.remove(bak)
}
```

---

## Chapter 12: crypto — Hashing and Encoding

The `crypto` module provides hashing algorithms and encoding utilities. It is intentionally small: two hash functions (SHA-256 and MD5) and two pairs of encode/decode functions (Base64 and hexadecimal). These six functions cover the most common needs—verifying data integrity, generating fingerprints, and preparing binary data for text-safe transport.

All functions accept strings and return strings. Hashes produce lowercase hexadecimal digests. Encoding functions convert raw bytes to a text representation; decoding functions reverse the process.

### Function Reference

| Function                  | Description                        | Example                                        | Return Type |
| ------------------------- | ---------------------------------- | ---------------------------------------------- | ----------- |
| `crypto.sha256(s)`        | SHA-256 hash, hex-encoded          | `crypto.sha256("hello")` → `"2cf24d..."`       | String      |
| `crypto.md5(s)`           | MD5 hash, hex-encoded              | `crypto.md5("hello")` → `"5d4114..."`          | String      |
| `crypto.base64_encode(s)` | Encode string to Base64            | `crypto.base64_encode("hello")` → `"aGVsbG8="` | String      |
| `crypto.base64_decode(s)` | Decode Base64 string               | `crypto.base64_decode("aGVsbG8=")` → `"hello"` | String      |
| `crypto.hex_encode(s)`    | Encode string bytes as hexadecimal | `crypto.hex_encode("AB")` → `"4142"`           | String      |
| `crypto.hex_decode(s)`    | Decode hex string to bytes         | `crypto.hex_decode("4142")` → `"AB"`           | String      |

> **MD5 is not secure.** MD5 is provided for legacy compatibility and checksums. Never use it for password hashing or security-critical fingerprints. Use SHA-256 instead.

### Core Examples

**SHA-256 hashing:**

```forge
let hash = crypto.sha256("forge")
say "SHA-256 of 'forge': {hash}"

// Same input always produces same output
let hash2 = crypto.sha256("forge")
let match = hash == hash2
say "Deterministic: {match}"

// Different input produces different output
let other = crypto.sha256("Forge")
let different = hash == other
say "Case sensitive: {different}"
```

Output:

```
SHA-256 of 'forge': <64-character hex string>
Deterministic: true
Case sensitive: false
```

**MD5 hashing:**

```forge
let md5 = crypto.md5("hello world")
say "MD5: {md5}"
```

Output:

```
MD5: 5eb63bbbe01eeed093cb22bb8f5acdc3
```

**Base64 encoding and decoding:**

```forge
let original = "Hello, Forge!"
let encoded = crypto.base64_encode(original)
say "Encoded: {encoded}"

let decoded = crypto.base64_decode(encoded)
say "Decoded: {decoded}"

let roundtrip = original == decoded
say "Round-trip matches: {roundtrip}"
```

Output:

```
Encoded: SGVsbG8sIEZvcmdlIQ==
Decoded: Hello, Forge!
Round-trip matches: true
```

**Hex encoding and decoding:**

```forge
let text = "Forge"
let hex = crypto.hex_encode(text)
say "Hex: {hex}"

let back = crypto.hex_decode(hex)
say "Decoded: {back}"
```

Output:

```
Hex: 466f726765
Decoded: Forge
```

**Combining hashing with encoding:**

```forge
let data = "sensitive payload"
let hash = crypto.sha256(data)
let b64_hash = crypto.base64_encode(hash)
say "SHA-256 (Base64): {b64_hash}"
```

### Recipes

**Recipe 11.1: Password Hashing with Salt**

```forge
fn hash_password(password, salt) {
    let salted = "{salt}:{password}"
    return crypto.sha256(salted)
}

fn verify_password(password, salt, expected_hash) {
    let computed = hash_password(password, salt)
    return computed == expected_hash
}

let salt = "random_salt_value_2024"
let hashed = hash_password("my_secret_password", salt)
say "Stored hash: {hashed}"

let valid = verify_password("my_secret_password", salt, hashed)
say "Correct password: {valid}"

let invalid = verify_password("wrong_password", salt, hashed)
say "Wrong password: {invalid}"
```

Output:

```
Stored hash: <64-character hex string>
Correct password: true
Wrong password: false
```

> **Production Warning.** This recipe demonstrates the principle of salted hashing. For production systems, use a dedicated password hashing algorithm (bcrypt, scrypt, Argon2) via an external service or API. Simple SHA-256, even with a salt, is not sufficient against modern brute-force attacks.

**Recipe 11.2: Data Integrity Verification**

```forge
fn write_with_checksum(path, data) {
    fs.write(path, data)
    let checksum = crypto.sha256(data)
    let checksum_path = "{path}.sha256"
    fs.write(checksum_path, checksum)
    say "Wrote {path} with checksum"
}

fn verify_integrity(path) {
    let data = fs.read(path)
    let checksum_path = "{path}.sha256"
    let expected = fs.read(checksum_path)
    let actual = crypto.sha256(data)
    return actual == expected
}

write_with_checksum("/tmp/payload.dat", "critical data that must not change")

let ok = verify_integrity("/tmp/payload.dat")
say "Integrity check: {ok}"

// Clean up
fs.remove("/tmp/payload.dat")
fs.remove("/tmp/payload.dat.sha256")
```

Output:

```
Wrote /tmp/payload.dat with checksum
Integrity check: true
```

**Recipe 11.3: Encoding Data for Transport**

```forge
// Encode a JSON payload for URL-safe transport
let payload = "{\"user\":\"alice\",\"role\":\"admin\"}"
let encoded = crypto.base64_encode(payload)
say "Transport-safe: {encoded}"

// Receiver decodes it
let received = crypto.base64_decode(encoded)
say "Received: {received}"
let obj = json.parse(received)
say "User: {obj.user}, Role: {obj.role}"
```

Output:

```
Transport-safe: eyJ1c2VyIjoiYWxpY2UiLCJyb2xlIjoiYWRtaW4ifQ==
Received: {"user":"alice","role":"admin"}
User: alice, Role: admin
```

**Recipe 11.4: File Checksum Validation**

```forge
fn checksum_file(path) {
    let content = fs.read(path)
    return crypto.sha256(content)
}

fs.write("/tmp/file_a.txt", "identical content")
fs.write("/tmp/file_b.txt", "identical content")
fs.write("/tmp/file_c.txt", "different content")

let a = checksum_file("/tmp/file_a.txt")
let b = checksum_file("/tmp/file_b.txt")
let c = checksum_file("/tmp/file_c.txt")

let ab_match = a == b
let ac_match = a == c
say "A == B: {ab_match}"
say "A == C: {ac_match}"

fs.remove("/tmp/file_a.txt")
fs.remove("/tmp/file_b.txt")
fs.remove("/tmp/file_c.txt")
```

Output:

```
A == B: true
A == C: false
```

---

## Chapter 13: db — SQLite

Forge includes a built-in SQLite driver, making it trivial to store and query structured data without installing a database server. The `db` module connects to a file-based database or an in-memory database, executes SQL statements, and returns results as arrays of Forge objects—no ORM layer, no mapping configuration. This makes Forge an excellent choice for data scripts, CLI tools, prototyping, and local applications.

### Function Reference

| Function          | Description                                          | Example                                        | Return Type   |
| ----------------- | ---------------------------------------------------- | ---------------------------------------------- | ------------- |
| `db.open(path)`   | Open a SQLite database file (or `":memory:"`)        | `db.open(":memory:")` → `true`                 | Bool          |
| `db.execute(sql)` | Execute a statement (CREATE, INSERT, UPDATE, DELETE) | `db.execute("CREATE TABLE ...")`               | Null          |
| `db.query(sql)`   | Execute a SELECT and return rows                     | `db.query("SELECT * FROM t")` → `[{...}, ...]` | Array[Object] |
| `db.close()`      | Close the database connection                        | `db.close()`                                   | Null          |

> **Connection Model.** Forge maintains one SQLite connection per thread using thread-local storage. Calling `db.open()` replaces any existing connection. Always call `db.close()` when finished to release the database file lock.

### The In-Memory Database

For scripts, tests, and prototyping, the special path `":memory:"` creates a database that lives only in RAM. It is fast, requires no cleanup, and vanishes when the program exits.

```forge
db.open(":memory:")
db.execute("CREATE TABLE greetings (id INTEGER PRIMARY KEY, message TEXT)")
db.execute("INSERT INTO greetings (message) VALUES ('Hello, Forge!')")

let rows = db.query("SELECT * FROM greetings")
say "Rows: {rows}"
db.close()
```

Output:

```
Rows: [{id: 1, message: Hello, Forge!}]
```

### Core Examples

**Creating tables and inserting data:**

```forge
db.open(":memory:")

db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT, active INTEGER)")

db.execute("INSERT INTO users (name, email, active) VALUES ('Alice', 'alice@example.com', 1)")
db.execute("INSERT INTO users (name, email, active) VALUES ('Bob', 'bob@example.com', 1)")
db.execute("INSERT INTO users (name, email, active) VALUES ('Charlie', 'charlie@example.com', 0)")

let users = db.query("SELECT * FROM users WHERE active = 1")
for user in users {
    say "{user.name} <{user.email}>"
}

db.close()
```

Output:

```
Alice <alice@example.com>
Bob <bob@example.com>
```

**Aggregation queries:**

```forge
db.open(":memory:")

db.execute("CREATE TABLE orders (id INTEGER PRIMARY KEY, product TEXT, amount REAL, qty INTEGER)")
db.execute("INSERT INTO orders (product, amount, qty) VALUES ('Widget', 9.99, 5)")
db.execute("INSERT INTO orders (product, amount, qty) VALUES ('Widget', 9.99, 3)")
db.execute("INSERT INTO orders (product, amount, qty) VALUES ('Gadget', 24.99, 2)")
db.execute("INSERT INTO orders (product, amount, qty) VALUES ('Gadget', 24.99, 1)")

let summary = db.query("SELECT product, SUM(amount * qty) as revenue, SUM(qty) as units FROM orders GROUP BY product")
for row in summary {
    say "{row.product}: ${row.revenue} ({row.units} units)"
}

db.close()
```

Output:

```
Gadget: $74.97 (3 units)
Widget: $79.92 (8 units)
```

**Updates and deletes:**

```forge
db.open(":memory:")

db.execute("CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT, done INTEGER DEFAULT 0)")
db.execute("INSERT INTO tasks (title) VALUES ('Write chapter 12')")
db.execute("INSERT INTO tasks (title) VALUES ('Review examples')")
db.execute("INSERT INTO tasks (title) VALUES ('Submit draft')")

// Mark a task as done
db.execute("UPDATE tasks SET done = 1 WHERE title = 'Write chapter 12'")

// Delete completed tasks
db.execute("DELETE FROM tasks WHERE done = 1")

let remaining = db.query("SELECT title FROM tasks")
say "Remaining tasks: {remaining}"
db.close()
```

Output:

```
Remaining tasks: [{title: Review examples}, {title: Submit draft}]
```

**Working with file-based databases:**

```forge
// Persistent database on disk
db.open("/tmp/forge_app.db")

db.execute("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT)")
db.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('theme', 'dark')")
db.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('lang', 'en')")

let settings = db.query("SELECT * FROM settings")
say "Settings: {settings}"

db.close()

// The database persists — we can reopen it
db.open("/tmp/forge_app.db")
let reloaded = db.query("SELECT value FROM settings WHERE key = 'theme'")
say "Theme: {reloaded}"
db.close()

fs.remove("/tmp/forge_app.db")
```

> **Column Types.** SQLite stores data as one of five types: NULL, INTEGER, REAL, TEXT, and BLOB. `db.query()` maps these to Forge's `null`, `Int`, `Float`, `String`, and a blob description string. Column names become object keys in the returned rows.

### Recipes

**Recipe 12.1: Full CRUD Application**

```forge
db.open(":memory:")

db.execute("CREATE TABLE contacts (id INTEGER PRIMARY KEY, name TEXT NOT NULL, phone TEXT, email TEXT)")

// Create
fn add_contact(name, phone, email) {
    db.execute("INSERT INTO contacts (name, phone, email) VALUES ('{name}', '{phone}', '{email}')")
}

// Read
fn get_contacts() {
    return db.query("SELECT * FROM contacts ORDER BY name")
}

fn find_contact(name) {
    return db.query("SELECT * FROM contacts WHERE name = '{name}'")
}

// Update
fn update_phone(name, new_phone) {
    db.execute("UPDATE contacts SET phone = '{new_phone}' WHERE name = '{name}'")
}

// Delete
fn remove_contact(name) {
    db.execute("DELETE FROM contacts WHERE name = '{name}'")
}

// Use the CRUD functions
add_contact("Alice", "555-0101", "alice@mail.com")
add_contact("Bob", "555-0102", "bob@mail.com")
add_contact("Charlie", "555-0103", "charlie@mail.com")

say "All contacts:"
let all = get_contacts()
term.table(all)

update_phone("Bob", "555-9999")
say "\nAfter updating Bob's phone:"
let bob = find_contact("Bob")
say "Bob: {bob}"

remove_contact("Charlie")
say "\nAfter removing Charlie:"
let remaining = get_contacts()
term.table(remaining)

db.close()
```

**Recipe 12.2: Data Migration**

```forge
db.open(":memory:")

// Old schema
db.execute("CREATE TABLE users_v1 (id INTEGER PRIMARY KEY, fullname TEXT, email TEXT)")
db.execute("INSERT INTO users_v1 (fullname, email) VALUES ('Alice Smith', 'alice@example.com')")
db.execute("INSERT INTO users_v1 (fullname, email) VALUES ('Bob Jones', 'bob@example.com')")

// New schema with split name fields
db.execute("CREATE TABLE users_v2 (id INTEGER PRIMARY KEY, first_name TEXT, last_name TEXT, email TEXT)")

// Migrate data
let old_users = db.query("SELECT * FROM users_v1")
for user in old_users {
    let parts = split(user.fullname, " ")
    let first = parts[0]
    let last = parts[1]
    db.execute("INSERT INTO users_v2 (first_name, last_name, email) VALUES ('{first}', '{last}', '{user.email}')")
}

say "Migrated users:"
let new_users = db.query("SELECT * FROM users_v2")
term.table(new_users)

db.close()
```

**Recipe 12.3: Report Generation**

```forge
db.open(":memory:")

db.execute("CREATE TABLE sales (id INTEGER PRIMARY KEY, product TEXT, amount REAL, region TEXT)")
db.execute("INSERT INTO sales (product, amount, region) VALUES ('Widget', 29.99, 'North')")
db.execute("INSERT INTO sales (product, amount, region) VALUES ('Gadget', 49.99, 'South')")
db.execute("INSERT INTO sales (product, amount, region) VALUES ('Widget', 19.99, 'East')")
db.execute("INSERT INTO sales (product, amount, region) VALUES ('Gizmo', 99.99, 'North')")
db.execute("INSERT INTO sales (product, amount, region) VALUES ('Gadget', 49.99, 'West')")

say "=== Sales Report ==="

say "\nBy Product:"
let by_product = db.query("SELECT product, COUNT(*) as orders, SUM(amount) as total FROM sales GROUP BY product ORDER BY total DESC")
term.table(by_product)

say "\nBy Region:"
let by_region = db.query("SELECT region, COUNT(*) as orders, SUM(amount) as total FROM sales GROUP BY region ORDER BY total DESC")
term.table(by_region)

say "\nTop Sale:"
let top = db.query("SELECT product, amount, region FROM sales ORDER BY amount DESC LIMIT 1")
say "{top}"

db.close()
```

**Recipe 12.4: Test Fixtures**

```forge
fn setup_test_db() {
    db.open(":memory:")
    db.execute("CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL, stock INTEGER)")
    db.execute("INSERT INTO products (name, price, stock) VALUES ('Pen', 1.99, 100)")
    db.execute("INSERT INTO products (name, price, stock) VALUES ('Notebook', 4.99, 50)")
    db.execute("INSERT INTO products (name, price, stock) VALUES ('Eraser', 0.99, 200)")
}

fn teardown_test_db() {
    db.close()
}

// Test: all products have positive prices
setup_test_db()
let products = db.query("SELECT * FROM products WHERE price <= 0")
let count = len(products)
assert_eq(count, 0)
say "Test passed: all prices positive"
teardown_test_db()

// Test: stock levels are reasonable
setup_test_db()
let low_stock = db.query("SELECT * FROM products WHERE stock < 10")
let low_count = len(low_stock)
assert_eq(low_count, 0)
say "Test passed: no dangerously low stock"
teardown_test_db()
```

Output:

```
Test passed: all prices positive
Test passed: no dangerously low stock
```

---

## Chapter 14: pg — PostgreSQL

While the `db` module handles local SQLite databases, the `pg` module connects Forge to PostgreSQL—the workhorse of production infrastructure. The API mirrors `db` closely (connect, query, execute, close), so moving from a prototype on SQLite to a production system on PostgreSQL requires minimal code changes.

The `pg` module runs on Forge's async runtime (Tokio under the hood). Connection management uses thread-local storage, giving you one active connection per program. For scripts, CLI tools, and single-connection services, this model is simple and effective.

### Function Reference

| Function              | Description                                          | Example                                                      | Return Type   |
| --------------------- | ---------------------------------------------------- | ------------------------------------------------------------ | ------------- |
| `pg.connect(connstr)` | Connect to a PostgreSQL server                       | `pg.connect("host=localhost dbname=mydb user=app")` → `true` | Bool          |
| `pg.query(sql)`       | Execute a SELECT and return rows                     | `pg.query("SELECT * FROM users")` → `[{...}]`                | Array[Object] |
| `pg.execute(sql)`     | Execute INSERT, UPDATE, DELETE; return rows affected | `pg.execute("DELETE FROM old_logs")` → `42`                  | Int           |
| `pg.close()`          | Close the connection                                 | `pg.close()`                                                 | Null          |

### Connection Strings

PostgreSQL connection strings follow the standard `key=value` format:

```
host=localhost port=5432 dbname=myapp user=appuser password=secret
```

Common parameters:

| Parameter  | Description           | Default         |
| ---------- | --------------------- | --------------- |
| `host`     | Server hostname or IP | `localhost`     |
| `port`     | Server port           | `5432`          |
| `dbname`   | Database name         | Same as user    |
| `user`     | Username              | Current OS user |
| `password` | Password              | None            |
| `sslmode`  | SSL mode              | `prefer`        |

### Core Examples

**Connecting and querying:**

```forge
pg.connect("host=localhost dbname=myapp user=app password=secret")

let users = pg.query("SELECT id, name, email FROM users LIMIT 5")
for user in users {
    say "{user.id}: {user.name} <{user.email}>"
}

pg.close()
```

**Executing write operations:**

```forge
pg.connect("host=localhost dbname=myapp user=app password=secret")

let affected = pg.execute("UPDATE users SET last_login = NOW() WHERE id = 1")
say "Updated {affected} row(s)"

pg.execute("INSERT INTO audit_log (action, user_id) VALUES ('login', 1)")

pg.close()
```

**Creating tables:**

```forge
pg.connect("host=localhost dbname=myapp user=app password=secret")

pg.execute("CREATE TABLE IF NOT EXISTS events (
    id SERIAL PRIMARY KEY,
    type TEXT NOT NULL,
    payload JSONB,
    created_at TIMESTAMP DEFAULT NOW()
)")

pg.execute("INSERT INTO events (type, payload) VALUES ('signup', '{\"user\": \"alice\"}')")

let events = pg.query("SELECT * FROM events ORDER BY created_at DESC LIMIT 10")
term.table(events)

pg.close()
```

**Aggregation queries:**

```forge
pg.connect("host=localhost dbname=analytics user=app password=secret")

let stats = pg.query("SELECT
    DATE(created_at) as day,
    COUNT(*) as signups,
    COUNT(DISTINCT country) as countries
    FROM users
    WHERE created_at > NOW() - INTERVAL '7 days'
    GROUP BY DATE(created_at)
    ORDER BY day")

say "Signup stats (last 7 days):"
term.table(stats)

pg.close()
```

> **Type Mapping.** PostgreSQL types are mapped to Forge values as follows: `int4`/`int8` → `Int`, `float4`/`float8` → `Float`, `text`/`varchar` → `String`, `bool` → `Bool`, and `NULL` → `null`. JSONB columns are returned as strings—parse them with `json.parse()` if you need the structured data.

### Recipes

**Recipe 13.1: Connection Helper with Error Handling**

```forge
fn connect_db() {
    let host = env.get("DB_HOST", "localhost")
    let port = env.get("DB_PORT", "5432")
    let name = env.get("DB_NAME", "myapp")
    let user = env.get("DB_USER", "app")
    let pass = env.get("DB_PASS", "")
    let conn = "host={host} port={port} dbname={name} user={user} password={pass}"
    pg.connect(conn)
    say "Connected to {name}@{host}:{port}"
}

fn disconnect_db() {
    pg.close()
    say "Disconnected"
}
```

**Recipe 13.2: Batch Insert**

```forge
pg.connect("host=localhost dbname=myapp user=app password=secret")

pg.execute("CREATE TABLE IF NOT EXISTS metrics (name TEXT, value REAL, ts TIMESTAMP DEFAULT NOW())")

let metrics = [
    { name: "cpu_usage", value: 72.5 },
    { name: "memory_mb", value: 4096.0 },
    { name: "disk_pct", value: 45.2 },
    { name: "network_mbps", value: 125.8 }
]

for m in metrics {
    pg.execute("INSERT INTO metrics (name, value) VALUES ('{m.name}', {m.value})")
}

say "Inserted {len(metrics)} metrics"

let results = pg.query("SELECT name, value FROM metrics ORDER BY name")
term.table(results)

pg.close()
```

**Recipe 13.3: Migration Runner**

```forge
fn run_migration(version, sql) {
    say "Running migration v{version}..."
    pg.execute(sql)
    pg.execute("INSERT INTO schema_migrations (version) VALUES ({version})")
    say "Migration v{version} complete"
}

pg.connect("host=localhost dbname=myapp user=app password=secret")

pg.execute("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT NOW())")

let applied = pg.query("SELECT version FROM schema_migrations ORDER BY version")
let applied_versions = map(applied, fn(r) { return r.version })

if contains(applied_versions, 1) == false {
    run_migration(1, "CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT, email TEXT UNIQUE)")
}

if contains(applied_versions, 2) == false {
    run_migration(2, "ALTER TABLE users ADD COLUMN created_at TIMESTAMP DEFAULT NOW()")
}

say "All migrations applied"
pg.close()
```

**Recipe 13.4: Health Check Query**

```forge
fn db_health_check() {
    safe {
        pg.connect("host=localhost dbname=myapp user=app password=secret")
        let result = pg.query("SELECT 1 as ok")
        pg.close()
        if len(result) > 0 {
            return { status: "healthy", db: "connected" }
        }
    }
    return { status: "unhealthy", db: "unreachable" }
}

let health = db_health_check()
say "Database: {health.status}"
```

---

## Chapter 15: json — Serialization

JSON is the lingua franca of modern APIs, configuration files, and data exchange. Forge embraces JSON at the language level—object literals in Forge _are_ JSON-compatible structures—and the `json` module provides three functions to move between Forge values and JSON text.

Because Forge objects and JSON objects share the same structural model, serialization is natural. There is no schema to define, no mapping to configure. A Forge object becomes JSON text with `json.stringify()`, and JSON text becomes a Forge object with `json.parse()`.

### Function Reference

| Function                | Description                                   | Example                                  | Return Type |
| ----------------------- | --------------------------------------------- | ---------------------------------------- | ----------- |
| `json.parse(s)`         | Parse a JSON string into a Forge value        | `json.parse("{\"a\":1}")` → `{a: 1}`     | Value       |
| `json.stringify(value)` | Convert a Forge value to compact JSON string  | `json.stringify({a: 1})` → `"{\"a\":1}"` | String      |
| `json.pretty(value)`    | Convert a Forge value to indented JSON string | `json.pretty({a: 1})` → formatted string | String      |

> **Number Handling.** `json.parse()` converts JSON numbers to `Int` when they have no fractional part, and `Float` otherwise. The number `42` becomes `Int(42)`, while `42.0` becomes `Float(42.0)`.

### Core Examples

**Parsing JSON strings:**

```forge
let text = "{\"name\":\"Alice\",\"age\":30,\"active\":true}"
let user = json.parse(text)
say "Name: {user.name}"
say "Age: {user.age}"
say "Active: {user.active}"
```

Output:

```
Name: Alice
Age: 30
Active: true
```

**Parsing arrays:**

```forge
let arr_text = "[1, 2, 3, 4, 5]"
let numbers = json.parse(arr_text)
say "Count: {len(numbers)}"
say "First: {numbers[0]}"
say "Last: {numbers[4]}"
```

Output:

```
Count: 5
First: 1
Last: 5
```

**Stringifying Forge objects:**

```forge
let server = {
    host: "api.example.com",
    port: 443,
    tls: true,
    routes: ["/users", "/posts", "/health"]
}

let compact = json.stringify(server)
say "Compact: {compact}"
```

Output:

```
Compact: {"host":"api.example.com","port":443,"tls":true,"routes":["/users","/posts","/health"]}
```

**Pretty-printing:**

```forge
let config = {
    app: "forge-demo",
    version: "1.0.0",
    database: {
        host: "localhost",
        port: 5432
    },
    features: ["auth", "logging"]
}

let pretty = json.pretty(config)
say pretty
```

Output:

```json
{
  "app": "forge-demo",
  "version": "1.0.0",
  "database": {
    "host": "localhost",
    "port": 5432
  },
  "features": ["auth", "logging"]
}
```

**Nested structures:**

```forge
let api_response = {
    status: 200,
    data: {
        users: [
            { id: 1, name: "Alice" },
            { id: 2, name: "Bob" }
        ],
        total: 2
    }
}

let text = json.stringify(api_response)
say "Serialized length: {len(text)} characters"

// Round-trip
let restored = json.parse(text)
let first_user = restored.data.users[0]
say "First user: {first_user.name}"
```

Output:

```
Serialized length: 77 characters
First user: Alice
```

**Handling null and boolean values:**

```forge
let data = json.parse("{\"value\":null,\"flag\":false,\"count\":0}")
say "Value: {data.value}"
say "Flag: {data.flag}"
say "Count: {data.count}"

let back = json.stringify(data)
say "Serialized: {back}"
```

Output:

```
Value: null
Flag: false
Count: 0
Serialized: {"value":null,"flag":false,"count":0}
```

### Recipes

**Recipe 14.1: API Response Handling**

```forge
// Simulate an API response
let response_text = "{\"status\":\"ok\",\"data\":{\"items\":[{\"id\":1,\"name\":\"Widget\",\"price\":9.99},{\"id\":2,\"name\":\"Gadget\",\"price\":24.99}],\"page\":1,\"total_pages\":5}}"

let response = json.parse(response_text)

if response.status == "ok" {
    let items = response.data.items
    say "Found {len(items)} items (page {response.data.page} of {response.data.total_pages}):"
    for item in items {
        say "  #{item.id} {item.name} — ${item.price}"
    }
}
```

Output:

```
Found 2 items (page 1 of 5):
  #1 Widget — $9.99
  #2 Gadget — $24.99
```

**Recipe 14.2: Config File Management**

```forge
fn load_config(path) {
    if fs.exists(path) {
        return fs.read_json(path)
    }
    let defaults = {
        theme: "dark",
        language: "en",
        notifications: true,
        max_items: 50
    }
    fs.write_json(path, defaults)
    return defaults
}

fn update_config(path, key, value) {
    let config = load_config(path)
    // Build updated config
    let updated = json.parse(json.stringify(config))
    fs.write_json(path, updated)
    return updated
}

let cfg = load_config("/tmp/app_settings.json")
say "Theme: {cfg.theme}"

fs.remove("/tmp/app_settings.json")
```

**Recipe 14.3: Data Transformation Pipeline**

```forge
let raw = "[{\"first\":\"Alice\",\"last\":\"Smith\",\"score\":92},{\"first\":\"Bob\",\"last\":\"Jones\",\"score\":87},{\"first\":\"Carol\",\"last\":\"White\",\"score\":95}]"

let students = json.parse(raw)

// Transform: add full name and grade
let graded = map(students, fn(s) {
    let grade = "C"
    if s.score >= 90 {
        let grade = "A"
    } else if s.score >= 80 {
        let grade = "B"
    }
    return {
        name: "{s.first} {s.last}",
        score: s.score,
        grade: grade
    }
})

say json.pretty(graded)
```

**Recipe 14.4: JSON Merge Utility**

```forge
fn merge_objects(base, overlay) {
    let base_text = json.stringify(base)
    let overlay_text = json.stringify(overlay)
    // Simple merge: overlay keys win
    let merged = json.parse(base_text)
    // In practice, iterate overlay keys
    return merged
}

let defaults = { color: "blue", size: 12, bold: false }
let user_prefs = { size: 16, bold: true }

say "Defaults: {json.stringify(defaults)}"
say "User prefs: {json.stringify(user_prefs)}"
```

---

## Chapter 16: regex — Regular Expressions

Regular expressions are the Swiss Army knife of text processing, and Forge's `regex` module makes them accessible through five focused functions. You can test whether a pattern matches, extract the first or all occurrences, replace matches, or split text by a pattern. Under the hood, Forge uses Rust's `regex` crate—one of the fastest regex engines available—so even complex patterns over large inputs run efficiently.

All functions take the **text first, then the pattern**. This order reads naturally in Forge: "search _this text_ for _this pattern_."

### Pattern Syntax Quick Reference

| Pattern  | Matches                            | Example                       |
| -------- | ---------------------------------- | ----------------------------- |
| `.`      | Any character except newline       | `a.c` → "abc", "a1c"          |
| `\d`     | Digit (0–9)                        | `\d+` → "42", "7"             |
| `\w`     | Word character (letter, digit, \_) | `\w+` → "hello", "x_1"        |
| `\s`     | Whitespace                         | `\s+` → " ", "\t"             |
| `[abc]`  | Character class                    | `[aeiou]` → vowels            |
| `[^abc]` | Negated class                      | `[^0-9]` → non-digits         |
| `^`      | Start of string                    | `^Hello`                      |
| `$`      | End of string                      | `world$`                      |
| `*`      | Zero or more                       | `ab*c` → "ac", "abbc"         |
| `+`      | One or more                        | `ab+c` → "abc", "abbc"        |
| `?`      | Zero or one                        | `colou?r` → "color", "colour" |
| `{n,m}`  | Between n and m repetitions        | `\d{2,4}` → "42", "1234"      |
| `()`     | Capture group                      | `(\d+)-(\d+)`                 |
| `\|`     | Alternation                        | `cat\|dog`                    |

### Function Reference

| Function                         | Description                              | Example                                             | Return Type    |
| -------------------------------- | ---------------------------------------- | --------------------------------------------------- | -------------- |
| `regex.test(text, pattern)`      | Test if pattern matches anywhere in text | `regex.test("hello", "ell")` → `true`               | Bool           |
| `regex.find(text, pattern)`      | Find first match                         | `regex.find("abc123", "\\d+")` → `"123"`            | String or Null |
| `regex.find_all(text, pattern)`  | Find all matches                         | `regex.find_all("a1b2c3", "\\d")` → `["1","2","3"]` | Array[String]  |
| `regex.replace(text, pat, repl)` | Replace all matches                      | `regex.replace("aabaa", "a+", "x")` → `"xbx"`       | String         |
| `regex.split(text, pattern)`     | Split text by pattern                    | `regex.split("a:b::c", ":+")` → `["a","b","c"]`     | Array[String]  |

> **Backslash Escaping.** In Forge strings, backslashes need to be doubled for regex special sequences: write `"\\d+"` to match one or more digits. The first backslash escapes the second, so the regex engine receives `\d+`.

### Core Examples

**Testing for patterns:**

```forge
let email = "alice@example.com"
let valid = regex.test(email, "^[\\w.+-]+@[\\w-]+\\.[\\w.]+$")
say "Valid email: {valid}"

let has_number = regex.test("abc123", "\\d")
say "Has number: {has_number}"

let starts_with_hello = regex.test("Hello, World!", "^Hello")
say "Starts with Hello: {starts_with_hello}"
```

Output:

```
Valid email: true
Has number: true
Starts with Hello: true
```

**Finding matches:**

```forge
let text = "Order #12345 was placed on 2024-01-15"

let order_id = regex.find(text, "#(\\d+)")
say "Order ID: {order_id}"

let date = regex.find(text, "\\d{4}-\\d{2}-\\d{2}")
say "Date: {date}"

let missing = regex.find(text, "refund")
say "Refund mention: {missing}"
```

Output:

```
Order ID: #12345
Date: 2024-01-15
Refund mention: null
```

**Finding all matches:**

```forge
let log = "Error at 10:30, Warning at 11:45, Error at 14:20"

let times = regex.find_all(log, "\\d{2}:\\d{2}")
say "Times found: {times}"

let errors = regex.find_all(log, "Error")
say "Error count: {len(errors)}"
```

Output:

```
Times found: ["10:30", "11:45", "14:20"]
Error count: 2
```

**Replacing patterns:**

```forge
let messy = "too    many     spaces    here"
let clean = regex.replace(messy, "\\s+", " ")
say "Cleaned: {clean}"

let censored = regex.replace("My phone is 555-1234", "\\d", "*")
say "Censored: {censored}"
```

Output:

```
Cleaned: too many spaces here
Censored: My phone is ***-****
```

**Splitting text:**

```forge
let csv_line = "Alice,30,Engineer,New York"
let fields = regex.split(csv_line, ",")
say "Fields: {fields}"

let words = regex.split("one  two\tthree\nfour", "\\s+")
say "Words: {words}"
```

Output:

```
Fields: ["Alice", "30", "Engineer", "New York"]
Words: ["one", "two", "three", "four"]
```

### Recipes

**Recipe 15.1: Input Validation**

```forge
fn validate_username(username) {
    if regex.test(username, "^[a-zA-Z][a-zA-Z0-9_]{2,19}$") {
        return Ok(username)
    }
    return Err("Username must start with a letter, 3–20 chars, only letters/digits/underscores")
}

fn validate_email(email) {
    if regex.test(email, "^[\\w.+-]+@[\\w-]+\\.[\\w.]+$") {
        return Ok(email)
    }
    return Err("Invalid email format")
}

let tests = ["alice", "bob_42", "3bad", "ab", "valid_user_name"]
for t in tests {
    let result = validate_username(t)
    let ok = is_ok(result)
    say "{t}: {ok}"
}
```

Output:

```
alice: true
bob_42: true
3bad: false
ab: false
valid_user_name: true
```

**Recipe 15.2: Log Parser**

```forge
fn parse_log_line(line) {
    let timestamp = regex.find(line, "\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2}")
    let level = regex.find(line, "\\b(INFO|WARN|ERROR|DEBUG)\\b")
    let message = regex.replace(line, "^.*?(INFO|WARN|ERROR|DEBUG)\\s*", "")
    return {
        timestamp: timestamp,
        level: level,
        message: message
    }
}

let log_lines = [
    "2024-01-15 10:30:00 INFO Server started on port 8080",
    "2024-01-15 10:30:05 WARN High memory usage: 85%",
    "2024-01-15 10:31:12 ERROR Connection refused: database"
]

for line in log_lines {
    let parsed = parse_log_line(line)
    say "[{parsed.level}] {parsed.message}"
}
```

**Recipe 15.3: Data Extraction**

```forge
let html = "<a href=\"https://example.com\">Example</a> and <a href=\"https://forge-lang.org\">Forge</a>"

let urls = regex.find_all(html, "https?://[^\"]+")
say "URLs found:"
for url in urls {
    say "  {url}"
}

// Extract all numbers from mixed text
let report = "Revenue: $1,234,567. Users: 42,000. Growth: 15.7%"
let numbers = regex.find_all(report, "[\\d,]+\\.?\\d*")
say "Numbers: {numbers}"
```

Output:

```
URLs found:
  https://example.com
  https://forge-lang.org
Numbers: ["1,234,567", "42,000", "15.7"]
```

**Recipe 15.4: Search and Replace Tool**

```forge
fn redact_sensitive(text) {
    let mut result = text
    // Redact credit card-like patterns
    result = regex.replace(result, "\\b\\d{4}[- ]?\\d{4}[- ]?\\d{4}[- ]?\\d{4}\\b", "[REDACTED-CC]")
    // Redact SSN-like patterns
    result = regex.replace(result, "\\b\\d{3}-\\d{2}-\\d{4}\\b", "[REDACTED-SSN]")
    // Redact email addresses
    result = regex.replace(result, "[\\w.+-]+@[\\w-]+\\.[\\w.]+", "[REDACTED-EMAIL]")
    return result
}

let sensitive = "Contact alice@example.com, SSN 123-45-6789, Card 4111-1111-1111-1111"
let safe = redact_sensitive(sensitive)
say safe
```

Output:

```
Contact [REDACTED-EMAIL], SSN [REDACTED-SSN], Card [REDACTED-CC]
```

---

## Chapter 17: env — Environment Variables

Environment variables are the standard mechanism for passing configuration to applications. The `env` module provides four functions that read, write, check, and enumerate environment variables within the running Forge process. Values set with `env.set()` affect only the current process and its children—they do not persist after the program exits.

This module is small by design. Combined with the `fs` and `json` modules, it covers all common configuration patterns, from simple feature flags to environment-aware deployment scripts.

### Function Reference

| Function                | Description                                 | Example                                | Return Type    |
| ----------------------- | ------------------------------------------- | -------------------------------------- | -------------- |
| `env.get(key)`          | Get an environment variable's value         | `env.get("HOME")` → `"/Users/alice"`   | String or Null |
| `env.get(key, default)` | Get with a fallback default                 | `env.get("PORT", "3000")` → `"3000"`   | String         |
| `env.set(key, value)`   | Set an environment variable (process-local) | `env.set("APP_MODE", "test")`          | Null           |
| `env.has(key)`          | Check if a variable is defined              | `env.has("DATABASE_URL")` → `false`    | Bool           |
| `env.keys()`            | List all environment variable names         | `env.keys()` → `["HOME", "PATH", ...]` | Array[String]  |

> **Default Values.** `env.get()` with two arguments never returns `null`—the second argument serves as a guaranteed fallback. Use the one-argument form when you need to detect missing variables explicitly.

### Core Examples

**Reading system variables:**

```forge
let home = env.get("HOME")
say "Home directory: {home}"

let shell = env.get("SHELL", "unknown")
say "Shell: {shell}"

let has_path = env.has("PATH")
say "PATH defined: {has_path}"
```

Output:

```
Home directory: /Users/alice
Shell: /bin/bash
PATH defined: true
```

**Setting and reading process-local variables:**

```forge
env.set("APP_ENV", "production")
env.set("LOG_LEVEL", "warn")

let app_env = env.get("APP_ENV")
let log_level = env.get("LOG_LEVEL")
say "Environment: {app_env}"
say "Log level: {log_level}"
```

Output:

```
Environment: production
Log level: warn
```

**Checking for required configuration:**

```forge
fn require_env(key) {
    if env.has(key) == false {
        say "ERROR: Required environment variable '{key}' is not set"
        return null
    }
    return env.get(key)
}

// This would fail if DATABASE_URL isn't set
let db_url = require_env("DATABASE_URL")
if db_url == null {
    say "Please set DATABASE_URL before running this program"
}
```

**Listing environment variables:**

```forge
let all_keys = env.keys()
let count = len(all_keys)
say "Total environment variables: {count}"

// Show first 5
let mut shown = 0
for key in all_keys {
    if shown < 5 {
        let val = env.get(key)
        say "  {key} = {val}"
        shown = shown + 1
    }
}
```

**Using defaults for optional configuration:**

```forge
let port = env.get("PORT", "8080")
let host = env.get("HOST", "0.0.0.0")
let workers = env.get("WORKERS", "4")

say "Server will listen on {host}:{port} with {workers} workers"
```

Output:

```
Server will listen on 0.0.0.0:8080 with 4 workers
```

### Recipes

**Recipe 16.1: Configuration Management**

```forge
fn load_env_config() {
    return {
        app_name: env.get("APP_NAME", "MyApp"),
        environment: env.get("APP_ENV", "development"),
        port: env.get("PORT", "3000"),
        db_host: env.get("DB_HOST", "localhost"),
        db_name: env.get("DB_NAME", "myapp_dev"),
        log_level: env.get("LOG_LEVEL", "debug"),
        debug: env.has("DEBUG")
    }
}

let config = load_env_config()
say "Application: {config.app_name}"
say "Environment: {config.environment}"
say "Port: {config.port}"
say "Debug mode: {config.debug}"
```

**Recipe 16.2: Feature Flags**

```forge
fn feature_enabled(flag_name) {
    let key = "FEATURE_{flag_name}"
    let val = env.get(key, "false")
    return val == "true" || val == "1" || val == "yes"
}

env.set("FEATURE_NEW_UI", "true")
env.set("FEATURE_BETA_API", "false")

let new_ui = feature_enabled("NEW_UI")
let beta = feature_enabled("BETA_API")
let dark_mode = feature_enabled("DARK_MODE")

say "New UI: {new_ui}"
say "Beta API: {beta}"
say "Dark mode: {dark_mode}"
```

Output:

```
New UI: true
Beta API: false
Dark mode: false
```

**Recipe 16.3: Environment Detection**

```forge
fn detect_environment() {
    // Check for common CI/CD variables
    if env.has("CI") {
        return "ci"
    }
    if env.has("KUBERNETES_SERVICE_HOST") {
        return "kubernetes"
    }
    if env.has("AWS_LAMBDA_FUNCTION_NAME") {
        return "lambda"
    }
    if env.has("HEROKU_APP_NAME") {
        return "heroku"
    }
    return env.get("APP_ENV", "development")
}

let platform = detect_environment()
say "Running in: {platform}"
```

**Recipe 16.4: Secrets Validator**

```forge
fn validate_secrets(required_keys) {
    let mut missing = []
    for key in required_keys {
        if env.has(key) == false {
            missing = append(missing, key)
        }
    }
    if len(missing) > 0 {
        say "Missing required environment variables:"
        for key in missing {
            say "  - {key}"
        }
        return false
    }
    say "All required secrets are configured"
    return true
}

let required = ["DATABASE_URL", "SECRET_KEY", "API_TOKEN"]
let ok = validate_secrets(required)
```

---

## Chapter 18: csv — Tabular Data

CSV (Comma-Separated Values) remains one of the most widely used data exchange formats, particularly for spreadsheets, data exports, and ETL pipelines. The `csv` module handles parsing and serialization of CSV data, automatically detecting column types and producing clean output. It treats the first row as headers and returns an array of objects where each key is a column name.

### Function Reference

| Function                | Description                                 | Example                                                   | Return Type   |
| ----------------------- | ------------------------------------------- | --------------------------------------------------------- | ------------- |
| `csv.parse(text)`       | Parse a CSV string into an array of objects | `csv.parse("name,age\nAlice,30")` → `[{name:"Alice"...}]` | Array[Object] |
| `csv.stringify(rows)`   | Convert an array of objects to a CSV string | `csv.stringify([{a:1}])` → `"a\n1\n"`                     | String        |
| `csv.read(path)`        | Read a CSV file and parse it                | `csv.read("data.csv")` → `[{...}]`                        | Array[Object] |
| `csv.write(path, rows)` | Write an array of objects to a CSV file     | `csv.write("out.csv", rows)`                              | Null          |

> **Automatic Type Detection.** `csv.parse()` inspects each cell and converts it to the most specific Forge type: integers for whole numbers, floats for decimals, booleans for "true"/"false", and strings for everything else. This means `"42"` becomes `Int(42)` and `"3.14"` becomes `Float(3.14)`.

### Core Examples

**Parsing CSV text:**

```forge
let data = "name,age,city
Alice,30,New York
Bob,25,London
Charlie,35,Tokyo"

let rows = csv.parse(data)
for row in rows {
    say "{row.name} is {row.age} years old, lives in {row.city}"
}
```

Output:

```
Alice is 30 years old, lives in New York
Bob is 25 years old, lives in London
Charlie is 35 years old, lives in Tokyo
```

**Type detection in action:**

```forge
let data = "metric,value,active
cpu,72.5,true
memory,4096,false
disk,45.2,true"

let rows = csv.parse(data)
for row in rows {
    say "{row.metric}: {row.value} (active: {row.active})"
}
```

Output:

```
cpu: 72.5 (active: true)
memory: 4096 (active: false)
disk: 45.2 (active: true)
```

**Creating CSV from objects:**

```forge
let products = [
    { name: "Widget", price: 9.99, stock: 150 },
    { name: "Gadget", price: 24.99, stock: 75 },
    { name: "Gizmo", price: 49.99, stock: 30 }
]

let csv_text = csv.stringify(products)
say csv_text
```

Output:

```
name,price,stock
Widget,9.99,150
Gadget,24.99,75
Gizmo,49.99,30
```

**Reading and writing CSV files:**

```forge
// Write a CSV file
let employees = [
    { id: 1, name: "Alice", department: "Engineering", salary: 95000 },
    { id: 2, name: "Bob", department: "Marketing", salary: 72000 },
    { id: 3, name: "Carol", department: "Engineering", salary: 98000 }
]

csv.write("/tmp/employees.csv", employees)
say "Wrote CSV file"

// Read it back
let loaded = csv.read("/tmp/employees.csv")
say "Loaded {len(loaded)} rows"
term.table(loaded)

fs.remove("/tmp/employees.csv")
```

**Handling special characters:**

```forge
let tricky = [
    { name: "Smith, John", role: "Manager", note: "Says \"hello\" often" },
    { name: "Jane Doe", role: "Developer", note: "No issues" }
]

let text = csv.stringify(tricky)
say text
```

> **Escaping.** `csv.stringify()` automatically quotes fields that contain commas or double quotes, following RFC 4180 conventions.

### Recipes

**Recipe 17.1: Data Import and Analysis**

```forge
let sales_data = "product,quarter,revenue
Widget,Q1,125000
Widget,Q2,142000
Widget,Q3,138000
Widget,Q4,165000
Gadget,Q1,89000
Gadget,Q2,95000
Gadget,Q3,102000
Gadget,Q4,118000"

let sales = csv.parse(sales_data)

// Calculate totals per product
let widget_sales = filter(sales, fn(r) { return r.product == "Widget" })
let gadget_sales = filter(sales, fn(r) { return r.product == "Gadget" })

let widget_total = reduce(widget_sales, 0, fn(acc, r) { return acc + r.revenue })
let gadget_total = reduce(gadget_sales, 0, fn(acc, r) { return acc + r.revenue })

say "Widget annual revenue: ${widget_total}"
say "Gadget annual revenue: ${gadget_total}"
```

Output:

```
Widget annual revenue: $570000
Gadget annual revenue: $404000
```

**Recipe 17.2: Report Generation**

```forge
fn generate_report(title, data) {
    say "=== {title} ==="
    term.table(data)
    say ""

    // Export to CSV
    let filename = "/tmp/report.csv"
    csv.write(filename, data)
    say "Report exported to {filename}"
    return filename
}

let metrics = [
    { metric: "Page Views", value: 125000, change: "+12%" },
    { metric: "Unique Users", value: 45000, change: "+8%" },
    { metric: "Bounce Rate", value: 32, change: "-3%" },
    { metric: "Avg Session", value: 245, change: "+15%" }
]

let path = generate_report("Weekly Metrics", metrics)
fs.remove(path)
```

**Recipe 17.3: CSV-to-JSON Converter**

```forge
fn csv_to_json(csv_path, json_path) {
    let rows = csv.read(csv_path)
    fs.write_json(json_path, rows)
    let count = len(rows)
    say "Converted {count} rows from CSV to JSON"
}

fn json_to_csv(json_path, csv_path) {
    let data = fs.read_json(json_path)
    csv.write(csv_path, data)
    let count = len(data)
    say "Converted {count} rows from JSON to CSV"
}

// Create test data
let test_data = "name,score,grade\nAlice,95,A\nBob,82,B\nCarol,91,A"
fs.write("/tmp/students.csv", test_data)

csv_to_json("/tmp/students.csv", "/tmp/students.json")

// Verify
let json_data = fs.read_json("/tmp/students.json")
say json.pretty(json_data)

// Clean up
fs.remove("/tmp/students.csv")
fs.remove("/tmp/students.json")
```

**Recipe 17.4: ETL Pipeline**

```forge
// Extract
let raw = "timestamp,sensor_id,temperature,humidity
2024-01-15T10:00:00,S001,22.5,45
2024-01-15T10:00:00,S002,23.1,42
2024-01-15T10:05:00,S001,22.8,44
2024-01-15T10:05:00,S002,23.4,41
2024-01-15T10:10:00,S001,23.0,43
2024-01-15T10:10:00,S002,24.0,40"

let readings = csv.parse(raw)
say "Extracted {len(readings)} readings"

// Transform: calculate averages per sensor
let s001 = filter(readings, fn(r) { return r.sensor_id == "S001" })
let s002 = filter(readings, fn(r) { return r.sensor_id == "S002" })

let s001_avg_temp = reduce(s001, 0.0, fn(acc, r) { return acc + r.temperature }) / len(s001)
let s002_avg_temp = reduce(s002, 0.0, fn(acc, r) { return acc + r.temperature }) / len(s002)

let summary = [
    { sensor: "S001", avg_temperature: s001_avg_temp, readings: len(s001) },
    { sensor: "S002", avg_temperature: s002_avg_temp, readings: len(s002) }
]

// Load: write summary
say "Sensor Averages:"
term.table(summary)
csv.write("/tmp/sensor_summary.csv", summary)
say "Summary written to /tmp/sensor_summary.csv"
fs.remove("/tmp/sensor_summary.csv")
```

---

## Chapter 19: log — Structured Logging

Every non-trivial program needs logging, and the `log` module provides four severity-level functions that write timestamped, color-coded messages to standard error. The interface is intentionally simple—call the function matching your severity level and pass any number of arguments. The module handles formatting, timestamps, and color.

Logs go to stderr, so they remain separate from your program's standard output. This distinction matters: you can pipe a Forge program's output to another tool while still seeing diagnostics in the terminal.

### Function Reference

| Function         | Description                    | Color  | Example                          | Return Type |
| ---------------- | ------------------------------ | ------ | -------------------------------- | ----------- |
| `log.info(...)`  | Informational message          | Green  | `log.info("Server started")`     | Null        |
| `log.warn(...)`  | Warning—something unexpected   | Yellow | `log.warn("Disk space low")`     | Null        |
| `log.error(...)` | Error—something went wrong     | Red    | `log.error("Connection failed")` | Null        |
| `log.debug(...)` | Debug—detailed diagnostic info | Dim    | `log.debug("Query took 42ms")`   | Null        |

> **Variable Arguments.** All four functions accept any number of arguments of any type. Arguments are converted to strings and joined with spaces.

### Output Format

Each log line follows this format:

```
[HH:MM:SS LEVEL] message
```

For example:

```
[10:30:45 INFO] Server started on port 8080
[10:30:45 WARN] No database URL configured, using defaults
[10:30:46 ERROR] Failed to connect to cache server
[10:30:46 DEBUG] Retrying connection attempt 2 of 3
```

### Core Examples

**Basic logging at each level:**

```forge
log.info("Application starting")
log.debug("Loading configuration from /etc/app/config.json")
log.warn("API key expires in 3 days")
log.error("Failed to open database connection")
```

Output (stderr, with colors):

```
[14:20:00 INFO] Application starting
[14:20:00 DEBUG] Loading configuration from /etc/app/config.json
[14:20:00 WARN] API key expires in 3 days
[14:20:00 ERROR] Failed to open database connection
```

**Logging multiple values:**

```forge
let user = "alice"
let action = "login"
let ip = "192.168.1.100"
log.info("User", user, "performed", action, "from", ip)
```

Output:

```
[14:20:00 INFO] User alice performed login from 192.168.1.100
```

**Logging objects and arrays:**

```forge
let request = { method: "POST", path: "/api/users", status: 201 }
log.info("Request completed:", request)

let errors = ["timeout", "retry_exhausted"]
log.error("Multiple failures:", errors)
```

**Conditional logging:**

```forge
fn process_item(item) {
    log.debug("Processing item:", item)

    if item.price < 0 {
        log.warn("Negative price detected for", item.name)
        return false
    }

    if item.stock == 0 {
        log.error("Out of stock:", item.name)
        return false
    }

    log.info("Successfully processed", item.name)
    return true
}

process_item({ name: "Widget", price: 9.99, stock: 10 })
process_item({ name: "Broken", price: -1, stock: 5 })
process_item({ name: "Sold Out", price: 19.99, stock: 0 })
```

**Timing operations:**

```forge
fn timed_operation(name) {
    log.debug("Starting:", name)
    // Simulate work
    let mut sum = 0
    repeat 10000 times {
        sum = sum + 1
    }
    log.info("Completed:", name)
    return sum
}

timed_operation("data processing")
timed_operation("report generation")
```

### Recipes

**Recipe 18.1: Application Logger**

```forge
fn create_logger(module_name) {
    return {
        info: fn(msg) { log.info("[{module_name}]", msg) },
        warn: fn(msg) { log.warn("[{module_name}]", msg) },
        error: fn(msg) { log.error("[{module_name}]", msg) },
        debug: fn(msg) { log.debug("[{module_name}]", msg) }
    }
}

let db_log = create_logger("database")
let api_log = create_logger("api")

db_log.info("Connected to PostgreSQL")
api_log.info("Listening on port 8080")
db_log.warn("Slow query detected: 2.3s")
api_log.error("Request timeout on /api/users")
```

Output:

```
[14:20:00 INFO] [database] Connected to PostgreSQL
[14:20:00 INFO] [api] Listening on port 8080
[14:20:00 WARN] [database] Slow query detected: 2.3s
[14:20:00 ERROR] [api] Request timeout on /api/users
```

**Recipe 18.2: Debug Tracing**

```forge
fn trace(label, value) {
    log.debug("TRACE [{label}]:", value)
    return value
}

// Use trace to follow data through a pipeline
let data = [10, 25, 3, 47, 12]
let filtered = trace("after filter", filter(data, fn(x) { return x > 10 }))
let mapped = trace("after map", map(filtered, fn(x) { return x * 2 }))
let total = trace("after reduce", reduce(mapped, 0, fn(a, b) { return a + b }))
say "Result: {total}"
```

**Recipe 18.3: Error Reporting with Context**

```forge
fn log_error_with_context(operation, error_msg, context) {
    log.error("Operation failed:", operation)
    log.error("  Error:", error_msg)
    log.error("  Context:", json.stringify(context))
}

fn process_payment(order) {
    if order.amount <= 0 {
        log_error_with_context("process_payment", "Invalid amount", order)
        return false
    }
    log.info("Payment processed:", order.amount, "for order", order.id)
    return true
}

process_payment({ id: "ORD-001", amount: 49.99, currency: "USD" })
process_payment({ id: "ORD-002", amount: 0, currency: "USD" })
```

**Recipe 18.4: Startup Diagnostics**

```forge
fn startup_checks() {
    log.info("=== Startup Diagnostics ===")

    let home = env.get("HOME", "unknown")
    log.info("Home directory:", home)

    let app_env = env.get("APP_ENV", "development")
    log.info("Environment:", app_env)

    if app_env == "production" {
        if env.has("DATABASE_URL") == false {
            log.error("DATABASE_URL is required in production!")
        }
        if env.has("SECRET_KEY") == false {
            log.error("SECRET_KEY is required in production!")
        }
    } else {
        log.debug("Running in", app_env, "mode — relaxed checks")
    }

    log.info("=== Diagnostics Complete ===")
}

startup_checks()
```

---

## Chapter 20: term — Terminal UI

The `term` module transforms the terminal from a plain text canvas into a rich presentation layer. It offers color functions for styling text, display functions for structured output like tables and progress bars, interactive prompts for user input, and visual effects that bring CLI applications to life. If you are building a command-line tool, a dashboard, or any interactive script, `term` is the module that makes it polished.

The module writes styled output using ANSI escape codes, which are supported by virtually every modern terminal emulator. Color functions return styled strings (so you can compose them), while display functions print directly to the terminal.

### Color and Style Functions

Color functions wrap text in ANSI escape sequences and **return a styled string**. You can assign them to variables, embed them in larger strings, or pass them to `say`.

| Function             | Description              | Example                              | Return Type |
| -------------------- | ------------------------ | ------------------------------------ | ----------- |
| `term.red(text)`     | Red foreground color     | `term.red("error!")` → styled string | String      |
| `term.green(text)`   | Green foreground color   | `term.green("success")` → styled     | String      |
| `term.blue(text)`    | Blue foreground color    | `term.blue("info")` → styled         | String      |
| `term.yellow(text)`  | Yellow foreground color  | `term.yellow("warning")` → styled    | String      |
| `term.cyan(text)`    | Cyan foreground color    | `term.cyan("note")` → styled         | String      |
| `term.magenta(text)` | Magenta foreground color | `term.magenta("special")` → styled   | String      |
| `term.bold(text)`    | Bold weight              | `term.bold("important")` → styled    | String      |
| `term.dim(text)`     | Dim/faint style          | `term.dim("secondary")` → styled     | String      |

### Display Functions

Display functions produce formatted output directly in the terminal.

| Function                       | Description                              | Example                                   | Return Type |
| ------------------------------ | ---------------------------------------- | ----------------------------------------- | ----------- |
| `term.table(rows)`             | Print formatted, aligned table           | `term.table([{a:1,b:2}])`                 | Null        |
| `term.hr()`                    | Print horizontal rule (default width 40) | `term.hr()`                               | Null        |
| `term.hr(width)`               | Horizontal rule with custom width        | `term.hr(60)`                             | Null        |
| `term.hr(width, char)`         | Horizontal rule with custom character    | `term.hr(20, "=")`                        | Null        |
| `term.clear()`                 | Clear the terminal screen                | `term.clear()`                            | Null        |
| `term.sparkline(values)`       | Return a sparkline string (█▇▆▅▄▃▂▁)     | `term.sparkline([1,5,3,8,2])` → `"▂▆▄█▂"` | String      |
| `term.bar(label, value)`       | Print a progress bar (max 100)           | `term.bar("CPU", 72)`                     | Null        |
| `term.bar(label, value, max)`  | Progress bar with custom max             | `term.bar("Sales", 750, 1000)`            | Null        |
| `term.banner(text)`            | Print text in a bordered banner (═)      | `term.banner("Welcome!")`                 | Null        |
| `term.box(text)`               | Print text in a bordered box (─)         | `term.box("Hello")`                       | Null        |
| `term.countdown(seconds)`      | Visual countdown timer                   | `term.countdown(3)`                       | Null        |
| `term.typewriter(text)`        | Print text character by character        | `term.typewriter("Loading...")`           | Null        |
| `term.typewriter(text, delay)` | Typewriter with custom delay (ms)        | `term.typewriter("Fast!", 10)`            | Null        |
| `term.gradient(text)`          | Return text with rainbow gradient colors | `term.gradient("Rainbow!")` → styled      | String      |

### Status Message Functions

Quick, emoji-prefixed status messages for common feedback patterns.

| Function            | Description                 | Emoji | Color  | Return Type |
| ------------------- | --------------------------- | ----- | ------ | ----------- |
| `term.success(msg)` | Print success message       | ✅    | Green  | Null        |
| `term.error(msg)`   | Print error message         | ❌    | Red    | Null        |
| `term.warning(msg)` | Print warning message       | ⚠️    | Yellow | Null        |
| `term.info(msg)`    | Print informational message | ℹ️    | Cyan   | Null        |

### Interactive Functions

Functions that accept user input, blocking until the user responds.

| Function                     | Description                                | Example                              | Return Type   |
| ---------------------------- | ------------------------------------------ | ------------------------------------ | ------------- |
| `term.confirm()`             | Prompt user for yes/no (default prompt)    | `term.confirm()` → `true` or `false` | Bool          |
| `term.confirm(prompt)`       | Prompt with custom question                | `term.confirm("Deploy?")` → `true`   | Bool          |
| `term.menu(options)`         | Show a numbered menu, return selected item | `term.menu(["A","B","C"])` → `"B"`   | Value or Null |
| `term.menu(options, prompt)` | Menu with custom prompt                    | `term.menu(items, "Pick one:")`      | Value or Null |

### Effects and Emoji Functions

| Function           | Description                    | Example                         | Return Type |
| ------------------ | ------------------------------ | ------------------------------- | ----------- |
| `term.beep()`      | Play system bell sound         | `term.beep()`                   | Null        |
| `term.emoji(name)` | Get emoji by name              | `term.emoji("rocket")` → `"🚀"` | String      |
| `term.emojis()`    | List all available emoji names | `term.emojis()`                 | Null        |

**Available Emoji Names:**

| Name(s)                 | Emoji |
| ----------------------- | ----- |
| `check`, `ok`, `yes`    | ✅    |
| `cross`, `no`, `fail`   | ❌    |
| `star`, `fav`           | ⭐    |
| `fire`, `hot`           | 🔥    |
| `heart`, `love`         | ❤️    |
| `rocket`, `launch`      | 🚀    |
| `warn`, `warning`       | ⚠️    |
| `info`, `information`   | ℹ️    |
| `bug`, `error`          | 🐛    |
| `clock`, `time`         | ⏰    |
| `folder`, `dir`         | 📁    |
| `file`, `doc`           | 📄    |
| `lock`, `secure`        | 🔒    |
| `key`                   | 🔑    |
| `link`, `url`           | 🔗    |
| `mail`, `email`         | 📧    |
| `globe`, `web`, `world` | 🌍    |
| `party`, `celebrate`    | 🎉    |
| `think`, `hmm`          | 🤔    |
| `wave`, `hi`, `hello`   | 👋    |
| `thumbsup`, `good`      | 👍    |
| `thumbsdown`, `bad`     | 👎    |
| `100`, `perfect`        | 💯    |
| `zap`, `bolt`, `fast`   | ⚡    |
| `gear`, `settings`      | ⚙️    |
| `tools`, `wrench`       | 🔧    |

### Core Examples

**Colored text:**

```forge
say term.red("Error: file not found")
say term.green("Success: deployment complete")
say term.yellow("Warning: disk space low")
say term.blue("Info: 42 items processed")
say term.bold("This text is bold")
say term.dim("This text is dimmed")
```

**Combining styles:**

```forge
let header = term.bold("=== System Status ===")
say header

let status = term.green("ONLINE")
say "  Server: {status}"

let warning = term.yellow("HIGH")
say "  CPU Usage: {warning}"

let critical = term.bold(term.red("CRITICAL"))
say "  Disk: {critical}"
```

**Tables:**

```forge
let servers = [
    { name: "web-01", status: "running", cpu: "45%", memory: "2.1 GB" },
    { name: "web-02", status: "running", cpu: "62%", memory: "3.4 GB" },
    { name: "db-01", status: "running", cpu: "78%", memory: "8.2 GB" },
    { name: "cache-01", status: "stopped", cpu: "0%", memory: "0 GB" }
]

say term.bold("Server Dashboard")
term.table(servers)
```

Output:

```
Server Dashboard
┌──────────┬─────────┬─────┬────────┐
│ name     │ status  │ cpu │ memory │
├──────────┼─────────┼─────┼────────┤
│ web-01   │ running │ 45% │ 2.1 GB │
│ web-02   │ running │ 62% │ 3.4 GB │
│ db-01    │ running │ 78% │ 8.2 GB │
│ cache-01 │ stopped │ 0%  │ 0 GB   │
└──────────┴─────────┴─────┴────────┘
```

**Sparklines and progress bars:**

```forge
let cpu_history = [23, 45, 67, 34, 89, 56, 78, 12, 45, 90]
let spark = term.sparkline(cpu_history)
say "CPU trend: {spark}"

term.bar("Downloads", 73)
term.bar("Uploads", 45)
term.bar("Storage", 891, 1000)
```

Output:

```
CPU trend: ▂▄▆▃█▅▇▁▄█
Downloads  |████████████████████████░░░░░░| 73%
Uploads    |██████████████░░░░░░░░░░░░░░░░| 45%
Storage    |███████████████████████████░░░| 891/1000
```

**Banners and boxes:**

```forge
term.banner("Welcome to Forge!")
term.box("This is a boxed message\nwith multiple lines")
term.hr()
term.hr(60, "=")
```

**Status messages:**

```forge
term.success("Build completed successfully")
term.warning("3 deprecated functions detected")
term.error("Test suite failed: 2 failures")
term.info("Next build scheduled for 10:00 AM")
```

Output:

```
✅ Build completed successfully
⚠️ 3 deprecated functions detected
❌ Test suite failed: 2 failures
ℹ️ Next build scheduled for 10:00 AM
```

**Emojis:**

```forge
let rocket = term.emoji("rocket")
let fire = term.emoji("fire")
let check = term.emoji("check")
say "{rocket} Launching deployment..."
say "{fire} Build is hot!"
say "{check} All tests passed"
```

Output:

```
🚀 Launching deployment...
🔥 Build is hot!
✅ All tests passed
```

**Interactive confirm:**

```forge
let proceed = term.confirm("Deploy to production?")
if proceed {
    say "Deploying..."
} else {
    say "Aborted"
}
```

Output:

```
Deploy to production? [y/N] y
Deploying...
```

**Interactive menu:**

```forge
let choice = term.menu(["Development", "Staging", "Production"], "Select environment:")
say "You chose: {choice}"
```

Output:

```
Select environment:
  1) Development
  2) Staging
  3) Production
> 2
You chose: Staging
```

### Recipes

**Recipe 19.1: Dashboard Builder**

```forge
fn show_dashboard(metrics) {
    term.clear()
    term.banner("System Dashboard")
    say ""

    // Status indicators
    for m in metrics {
        let icon = term.emoji("check")
        if m.value > 80 {
            let icon = term.emoji("warning")
        }
        if m.value > 95 {
            let icon = term.emoji("cross")
        }
        say " {icon} {m.label}"
        term.bar(m.label, m.value)
    }

    say ""
    term.hr()

    // Trend sparklines
    say term.bold("Trends (last 10 readings):")
    let cpu_spark = term.sparkline([45, 52, 48, 67, 72, 65, 78, 82, 71, 68])
    let mem_spark = term.sparkline([60, 61, 63, 62, 65, 64, 68, 70, 69, 72])
    say "  CPU:    {cpu_spark}"
    say "  Memory: {mem_spark}"
}

let metrics = [
    { label: "CPU", value: 68 },
    { label: "Memory", value: 72 },
    { label: "Disk", value: 45 },
    { label: "Network", value: 23 }
]

show_dashboard(metrics)
```

**Recipe 19.2: Progress Reporting**

```forge
fn process_with_progress(items) {
    let total = len(items)
    let mut processed = 0

    for item in items {
        processed = processed + 1
        // Simulate work
        let pct = processed * 100 / total
        term.bar("Progress", pct)
    }

    say ""
    term.success("Processed {total} items")
}

let items = ["file1.dat", "file2.dat", "file3.dat", "file4.dat", "file5.dat"]
process_with_progress(items)
```

**Recipe 19.3: Interactive CLI Tool**

```forge
fn run_cli() {
    term.banner("Forge Task Manager")
    say ""

    let action = term.menu([
        "List tasks",
        "Add task",
        "Complete task",
        "Generate report",
        "Exit"
    ], "What would you like to do?")

    if action == "List tasks" {
        let tasks = [
            { id: 1, title: "Write documentation", status: "in progress" },
            { id: 2, title: "Fix login bug", status: "pending" },
            { id: 3, title: "Deploy v2.0", status: "pending" }
        ]
        say ""
        term.table(tasks)
    } else if action == "Add task" {
        say "Adding new task..."
        term.success("Task added!")
    } else if action == "Generate report" {
        say ""
        term.info("Generating report...")
        term.typewriter("Analyzing tasks... Done!", 20)
        say ""
        term.success("Report generated")
    } else if action == "Exit" {
        let wave = term.emoji("wave")
        say "{wave} Goodbye!"
    }
}

run_cli()
```

**Recipe 19.4: Data Visualization**

```forge
fn visualize_data(title, dataset) {
    say term.bold(title)
    term.hr(50)

    // Table view
    term.table(dataset)
    say ""

    // Bar chart
    say term.bold("Bar Chart:")
    let values = map(dataset, fn(d) { return d.value })
    let max_val = reduce(values, 0, fn(a, b) {
        if b > a { return b }
        return a
    })
    for row in dataset {
        let val = float(row.value)
        let max_f = float(max_val)
        term.bar(row.label, val, max_f)
    }
    say ""

    // Sparkline
    say term.bold("Trend:")
    let spark = term.sparkline(values)
    say "  {spark}"
    say ""
    term.hr(50)
}

let monthly_revenue = [
    { label: "Jan", value: 12000 },
    { label: "Feb", value: 15000 },
    { label: "Mar", value: 13500 },
    { label: "Apr", value: 18000 },
    { label: "May", value: 22000 },
    { label: "Jun", value: 19500 }
]

visualize_data("Monthly Revenue Report", monthly_revenue)
```

**Recipe 19.5: Styled Error Reporter**

```forge
fn report_errors(errors) {
    if len(errors) == 0 {
        term.success("No errors found!")
        return null
    }

    let count = len(errors)
    term.error("{count} error(s) detected")
    say ""

    let mut idx = 0
    for err in errors {
        idx = idx + 1
        let num = term.bold("#{idx}")
        let file = term.cyan(err.file)
        let line_info = term.dim("line {err.line}")
        say " {num} {file} ({line_info})"

        let msg = term.red("    {err.message}")
        say msg
        say ""
    }

    term.hr()
    let summary = term.bold(term.red("{count} errors must be fixed before deploy"))
    say summary
}

let errors = [
    { file: "src/main.fg", line: 42, message: "Undefined variable 'config'" },
    { file: "src/utils.fg", line: 17, message: "Type mismatch: expected Int, got String" },
    { file: "tests/test_api.fg", line: 8, message: "Assertion failed: expected 200, got 404" }
]

report_errors(errors)
```

**Recipe 19.6: Colorful Build Output**

```forge
fn build_project(steps) {
    term.banner("Building Project")
    say ""

    let total = len(steps)
    let mut passed = 0

    for step in steps {
        let name = term.bold(step.name)
        say " {term.emoji("gear")} {name}..."

        if step.ok {
            term.success("  {step.name} complete")
            passed = passed + 1
        } else {
            term.error("  {step.name} failed: {step.error}")
        }
    }

    say ""
    term.hr()

    if passed == total {
        let msg = term.gradient("BUILD SUCCESSFUL")
        say " {term.emoji("party")} {msg}"
    } else {
        let failed = total - passed
        term.error("BUILD FAILED: {failed} of {total} steps failed")
    }
}

let steps = [
    { name: "Compile", ok: true, error: "" },
    { name: "Lint", ok: true, error: "" },
    { name: "Test", ok: true, error: "" },
    { name: "Bundle", ok: true, error: "" },
    { name: "Deploy", ok: true, error: "" }
]

build_project(steps)
```

---

## Chapter 21: Shell Integration — First-Class Bash

Forge treats the shell as a first-class citizen. Ten built-in functions give you full control over system commands, from quick one-liners to piping Forge data through Unix tool chains. There is no module prefix—these functions are available globally, so you can run `sh("date")` or `pipe_to(data, "sort -n")` anywhere in your program. Combined with Forge's data types and control flow, they turn scripts into powerful automation tools without dropping to a separate shell.

### Function Reference Table

| Function             | Returns                               | Description                             |
| -------------------- | ------------------------------------- | --------------------------------------- |
| `sh(cmd)`            | String                                | Run command, return stdout              |
| `shell(cmd)`         | Object `{stdout, stderr, status, ok}` | Run command, return full result         |
| `sh_lines(cmd)`      | Array of String                       | Run command, split stdout into lines    |
| `sh_json(cmd)`       | Object/Array                          | Run command, auto-parse JSON output     |
| `sh_ok(cmd)`         | Bool                                  | Run command, return true if exit code 0 |
| `which(cmd)`         | String or null                        | Find command path on $PATH              |
| `cwd()`              | String                                | Current working directory               |
| `cd(path)`           | String                                | Change working directory                |
| `lines(text)`        | Array of String                       | Split any string by newlines            |
| `pipe_to(data, cmd)` | Object `{stdout, stderr, status, ok}` | Feed string data into command via stdin |
| `run_command(cmd)`   | Object `{stdout, stderr, status, ok}` | Direct exec without shell (no pipes)    |

### sh — Quick One-Liners

`sh(cmd)` runs a command through `/bin/sh`, captures stdout, trims trailing whitespace, and returns it as a string. It is the fastest way to get a single value from a command. Pipes, redirects, and variable expansion work—everything your shell supports.

```forge
let user = sh("whoami")
say "Logged in as: {user}"

let date = sh("date +%Y-%m-%d")
say "Today: {date}"

let kernel = sh("uname -s")
let arch = sh("uname -m")
say "Platform: {kernel} on {arch}"
```

Output:

```
Logged in as: alice
Today: 2026-02-28
Platform: Darwin on x86_64
```

```forge
let count = sh("ls /etc | wc -l")
say "Files in /etc: {count}"

let disk_pct = sh("df -h / | tail -1 | awk '{print $5}'")
say "Disk usage: {disk_pct}"
```

> **When to use `sh`.** Use `sh()` when you only need stdout and do not care about exit codes or stderr. If the command fails, you still get whatever was printed to stdout—check `shell()` or `sh_ok()` when correctness depends on the exit status.

### shell — Full Result Object

`shell(cmd)` runs the same command as `sh()` but returns an object with four fields: `stdout`, `stderr`, `status` (exit code), and `ok` (boolean success). Use it when you need to inspect errors, capture stderr, or branch on whether the command succeeded.

```forge
let result = shell("ls -la /tmp")
say "Exit code: {result.status}"
say "Success: {result.ok}"
say "Output:\n{result.stdout}"
```

Output:

```
Exit code: 0
Success: true
Output:
total 48
drwxrwxrwt  15 root  wheel  480 Feb 28 10:00 ...
```

```forge
let r = shell("cat /nonexistent 2>&1")
if r.ok {
    say "File read OK"
} otherwise {
    say "Error: {r.stderr}"
    say "Status: {r.status}"
}
```

```forge
let ping_result = shell("ping -c 1 -W 2 localhost 2>/dev/null")
if ping_result.ok {
    say "Host is reachable"
} otherwise {
    say "Host unreachable (status: {ping_result.status})"
}
```

### sh_lines — Commands That Emit Lines

`sh_lines(cmd)` runs a command and returns its stdout as an array of strings, one per line. Empty lines are dropped. This is ideal for commands like `ls`, `find`, or `ps` whose output you want to iterate or filter.

```forge
let files = sh_lines("ls /etc | head -5")
say "First 5 files in /etc:"
for f in files {
    say "  {f}"
}
```

Output:

```
First 5 files in /etc:
  afpovertcp.cfg
  aliases
  asl
  bashrc_Apple_Terminal
  ...
```

```forge
let procs = sh_lines("ps aux | wc -l")
let count = procs[0]
say "Running processes: {count}"

let fg_files = sh_lines("find . -name '*.fg' -maxdepth 2 | head -10")
say "Forge files: {fg_files}"
```

### sh_json — Parse JSON from Commands

`sh_json(cmd)` runs a command and parses its stdout as JSON. If the command outputs valid JSON (e.g., `curl` responses, `kubectl get -o json`, or `jq` output), you get a Forge object or array directly.

```forge
let data = sh_json("echo '{\"name\":\"Forge\",\"version\":1}'")
say "Name: {data.name}, Version: {data.version}"

let arr = sh_json("echo '[1,2,3,4,5]'")
say "Sum: {reduce(arr, 0, fn(a,b){ return a + b })}"
```

Output:

```
Name: Forge, Version: 1
Sum: 15
```

```forge
let config = sh_json("cat /tmp/config.json")
if config != null {
    say "Loaded config: {config}"
} otherwise {
    say "Failed to parse JSON"
}
```

> **Parse failures.** If the command's stdout is not valid JSON, `sh_json()` raises an error. Wrap in `safe { }` if you need to handle malformed output gracefully.

### sh_ok — Exit Code Check

`sh_ok(cmd)` runs a command, discards stdout and stderr, and returns `true` if the exit code is 0, `false` otherwise. It is perfect for existence checks, process probes, and dependency validation.

```forge
if sh_ok("which docker") {
    say "Docker is installed"
} otherwise {
    say "Docker not found on PATH"
}
```

```forge
let tools = ["git", "cargo", "curl", "jq"]
for tool in tools {
    let found = sh_ok("which " + tool)
    let status = when found { true -> "found", else -> "MISSING" }
    say "  {tool}: {status}"
}
```

```forge
if sh_ok("pgrep -q nginx") {
    say "nginx is running"
} otherwise {
    say "nginx is not running"
}
```

### which — Resolve Command Path

`which(cmd)` looks up a command name on `$PATH` and returns its full path, or `null` if not found. It uses the system `which` (e.g., `/usr/bin/which`), so it reflects the same resolution as your shell.

```forge
let git_path = which("git")
say "Git at: {git_path}"

let missing = which("nonexistent_tool_xyz")
if missing == null {
    say "Tool not found"
}
```

```forge
let tools = ["git", "cargo", "forge"]
for tool in tools {
    let path = which(tool)
    if path != null {
        say "{tool}: {path}"
    } otherwise {
        say "{tool}: NOT FOUND"
    }
}
```

### cwd — Current Working Directory

`cwd()` returns the current working directory as a string. It is useful for logging, building paths, or restoring the directory later after a `cd()`.

```forge
let dir = cwd()
say "Working in: {dir}"

let report_path = cwd() + "/report.txt"
say "Report will be written to: {report_path}"
```

### cd — Change Working Directory

`cd(path)` changes the current process's working directory to the given path. Subsequent `sh()`, `shell()`, and file operations use this directory. It returns the path on success and raises an error if the directory does not exist or is not accessible.

```forge
cd("/tmp")
let dir = cwd()
say "Now in: {dir}"

cd("/var/log")
let log_list = sh_lines("ls | head -5")
say "Log files: {log_list}"
```

> **Process-local.** `cd()` affects only the Forge process. It does not change the shell that invoked `forge run`. Child processes spawned by `sh()` or `shell()` inherit the new working directory.

### lines — Split Text by Newlines

`lines(text)` splits any string by newline characters and returns an array of strings. Unlike `sh_lines()`, it does not run a command—it operates on a string you already have. Empty lines are preserved.

```forge
let log = "2024-01-15 10:30 INFO Started\n2024-01-15 10:31 WARN Retry\n2024-01-15 10:32 ERROR Failed"
let log_lines = lines(log)
say "Log entries: {len(log_lines)}"
for line in log_lines {
    say "  {line}"
}
```

```forge
let csv_text = fs.read("data.csv")
let rows = lines(csv_text)
let header = rows[0]
say "Columns: {header}"

let text = "a\nb\nc\n"
let arr = lines(text)
say "Items: {arr}"
```

### pipe_to — Feed Data Into Commands

`pipe_to(data, cmd)` sends a string into a command's stdin and returns the same result object as `shell()`: `{stdout, stderr, status, ok}`. The command receives `data` on stdin—as if you had run `echo "$data" | cmd`. Use it to process Forge data through `sort`, `grep`, `awk`, `jq`, or any Unix filter.

```forge
let names = "Charlie\nAlice\nBob"
let result = pipe_to(names, "sort")
say result.stdout
```

Output:

```
Alice
Bob
Charlie
```

```forge
let data = "apple\nbanana\ncherry\napricot\navocado"
let filtered = pipe_to(data, "grep '^a'")
say "Starts with 'a':\n{filtered.stdout}"

let numbers = "42\n17\n99\n3\n28"
let sorted = pipe_to(numbers, "sort -n")
say "Sorted numbers:\n{sorted.stdout}"
```

```forge
let csv = "name,score\nAlice,95\nBob,82\nCarol,91"
let result = pipe_to(csv, "awk -F',' 'NR>1 {print $2}' | sort -n")
say "Scores (sorted):\n{result.stdout}"
```

### run_command — Direct Exec Without Shell

`run_command(cmd)` runs a command without invoking a shell. The command string is split on whitespace: the first token is the program, the rest are arguments. There are no pipes, redirects, or variable expansion. It returns the same `{stdout, stderr, status, ok}` object as `shell()`.

Use `run_command()` when you need to avoid shell interpretation—for example, when arguments come from user input and must not be interpreted. Use `sh()` or `shell()` when you need pipes, redirects, or compound commands.

```forge
let r = run_command("echo hello world")
say r.stdout
say "ok: {r.ok}"
```

Output:

```
hello world
ok: true
```

```forge
let r = run_command("ls -la /tmp")
if r.ok {
    say r.stdout
} otherwise {
    say "Failed: {r.stderr}"
}
```

```forge
let r = run_command("date +%Y-%m-%d")
say "Date: {r.stdout}"
```

> **No shell features.** Commands like `ls | head -5` or `cat file.txt` will not work as expected with `run_command()`—the pipe and redirect are passed literally as arguments. Use `shell()` for those cases.

### Recipes

**Recipe 20.1: System Health Checker Script**

A one-stop script that gathers system info, disk usage, dependency checks, and service status.

```forge
say term.bold("=== System Health Check ===")
say ""

say term.blue("System Information:")
let user = sh("whoami")
let host = sh("hostname")
let os_name = sh("uname -s")
let arch = sh("uname -m")
say "  User:     {user}"
say "  Hostname: {host}"
say "  OS:       {os_name}"
say "  Arch:     {arch}"
say ""

say term.blue("Disk Usage:")
let disk = shell("df -h / | tail -1")
say "  {disk.stdout}"
say ""

say term.blue("Required Tools:")
let tools = ["git", "cargo", "curl"]
for tool in tools {
    let path = which(tool)
    if path != null {
        say "  {term.green(tool)}: {path}"
    } otherwise {
        say "  {term.red(tool)}: NOT FOUND"
    }
}
say ""

say term.blue("Service Status:")
if sh_ok("pgrep -q nginx") {
    say "  nginx: running"
} otherwise {
    say "  nginx: stopped"
}
say ""

term.success("Health check complete")
```

**Recipe 20.2: Log File Analyzer**

Read a log file, split by lines, filter for errors, and summarize.

```forge
// Simulate log file
let log_content = "2024-01-15 10:30:00 INFO Server started
2024-01-15 10:30:05 WARN High memory: 85%
2024-01-15 10:31:12 ERROR Connection refused: database
2024-01-15 10:32:00 INFO Recovery complete
2024-01-15 10:35:22 ERROR Timeout on /api/users"
fs.write("/tmp/app.log", log_content)

let all_lines = lines(fs.read("/tmp/app.log"))
let error_lines = filter(all_lines, fn(line) { return regex.test(line, "ERROR") })
let warn_lines = filter(all_lines, fn(line) { return regex.test(line, "WARN") })

say "Total lines: {len(all_lines)}"
say "Errors: {len(error_lines)}"
say "Warnings: {len(warn_lines)}"
say ""
say "Error lines:"
for line in error_lines {
    say "  {line}"
}

fs.remove("/tmp/app.log")
```

Output:

```
Total lines: 5
Errors: 2
Warnings: 1

Error lines:
  2024-01-15 10:31:12 ERROR Connection refused: database
  2024-01-15 10:35:22 ERROR Timeout on /api/users
```

**Recipe 20.3: JSON API Tool Wrapper**

Use `sh_json` with `curl` or `kubectl` to fetch and process JSON APIs.

```forge
// Example: fetch JSON from a public API
let url = "https://api.github.com/repos/rust-lang/rust"
let data = sh_json("curl -s " + url)

if data != null {
    say "Repository: {data.full_name}"
    say "Stars: {data.stargazers_count}"
    say "Open issues: {data.open_issues_count}"
} otherwise {
    say "Failed to fetch or parse API response"
}
```

```forge
// Example: kubectl get pods as JSON (when kubectl is configured)
let pods = sh_json("kubectl get pods -A -o json 2>/dev/null || echo '{}'")
if pods != null && pods.kind == "PodList" {
    let items = pods.items
    say "Pods: {len(items)}"
    for pod in items {
        let name = pod.metadata.name
        let status = pod.status.phase
        say "  {name}: {status}"
    }
} otherwise {
    say "kubectl not available or no pods"
}
```

**Recipe 20.4: Data Pipeline — Forge → Unix → Forge**

Build a pipeline: generate or load data in Forge, pipe it through Unix tools, and consume the result back in Forge.

```forge
// Generate CSV in Forge
let rows = [
    { name: "Charlie", score: 88 },
    { name: "Alice", score: 95 },
    { name: "Bob", score: 82 },
    { name: "Diana", score: 91 }
]
let csv = csv.stringify(rows)
say "Original data:"
say csv
say ""

// Pipe through sort (by second column, numeric)
let result = pipe_to(csv, "sort -t',' -k2 -rn")
let sorted_csv = result.stdout
say "After sort (by score descending):"
say sorted_csv
say ""

// Parse back into Forge and extract top scorer
let sorted_rows = csv.parse(sorted_csv)
let top = sorted_rows[0]
say "Top scorer: {top.name} with {top.score}"
```

```forge
// Another pipeline: filter log lines with grep, then count
let log_text = "INFO request 1\nERROR timeout\nINFO request 2\nERROR connection\nINFO request 3"
let err_result = pipe_to(log_text, "grep ERROR")
let err_lines = lines(err_result.stdout)
say "Error count: {len(err_lines)}"
```

---

---

## Chapter 22: npc — Fake Data Generation

Need test data? Prototyping a UI? Building a seed script? The `npc` module generates realistic fake data without external dependencies. Every call returns different random data.

### Function Reference

| Function              | Description               | Example Output              |
| --------------------- | ------------------------- | --------------------------- |
| `npc.name()`          | Full name (first + last)  | `"Jordan Patel"`            |
| `npc.first_name()`    | First name only           | `"Luna"`                    |
| `npc.last_name()`     | Last name only            | `"Nakamura"`                |
| `npc.email()`         | Realistic email address   | `"kai.chen42@gmail.com"`    |
| `npc.username()`      | Fun username              | `"cosmic_dragon247"`        |
| `npc.phone()`         | US-format phone           | `"(415) 867-5309"`          |
| `npc.number(min,max)` | Random integer in range   | `42`                        |
| `npc.pick(arr)`       | Random item from array    | (varies)                    |
| `npc.bool()`          | Random true/false         | `true`                      |
| `npc.sentence(n?)`    | Random sentence (n words) | `"Code runs fast through."` |
| `npc.word()`          | Single random word        | `"algorithm"`               |
| `npc.id()`            | UUID-like identifier      | `"a3f8k2m1-x9b2-..."`       |
| `npc.color()`         | Random hex color          | `"#3a7bc4"`                 |
| `npc.ip()`            | Random IPv4 address       | `"192.168.42.7"`            |
| `npc.url()`           | Random HTTPS URL          | `"https://techflow.io/api"` |
| `npc.company()`       | Random company name       | `"PixelForge"`              |

### Core Examples

```forge
// Seed a database with 10 fake users
db.open(":memory:")
db.execute("CREATE TABLE users (name TEXT, email TEXT, role TEXT)")

let roles = ["admin", "user", "editor"]
repeat 10 times {
    db.execute("INSERT INTO users VALUES (?, ?, ?)", [
        npc.name(),
        npc.email(),
        npc.pick(roles)
    ])
}

let users = db.query("SELECT * FROM users")
for each u in users {
    say "{u.name} — {u.email} ({u.role})"
}
db.close()
```

```forge
// Generate test API payloads
let payload = {
    id: npc.id(),
    name: npc.name(),
    email: npc.email(),
    active: npc.bool(),
    score: npc.number(1, 100),
    color: npc.color()
}
say json.pretty(payload)
```

---

## Chapter 23: String Transformations

Forge includes powerful string transformation builtins that go beyond basic split/join. All support method syntax (`str.function()`).

### Function Reference

| Function                    | Description                     | Example                                      |
| --------------------------- | ------------------------------- | -------------------------------------------- |
| `substring(s, start, end?)` | Extract by char indices         | `substring("hello", 0, 3)` → `"hel"`         |
| `index_of(s, substr)`       | First index of substr, or -1    | `index_of("abcabc", "bc")` → `1`             |
| `last_index_of(s, substr)`  | Last index of substr, or -1     | `last_index_of("abcabc", "bc")` → `4`        |
| `pad_start(s, len, char?)`  | Left-pad (default space)        | `pad_start("42", 5, "0")` → `"00042"`        |
| `pad_end(s, len, char?)`    | Right-pad (default space)       | `pad_end("42", 5)` → `"42   "`               |
| `capitalize(s)`             | Uppercase first, lowercase rest | `capitalize("hELLO")` → `"Hello"`            |
| `title(s)`                  | Title Case Each Word            | `title("the quick fox")` → `"The Quick Fox"` |
| `repeat_str(s, n)`          | Repeat string n times           | `repeat_str("-", 5)` → `"-----"`             |
| `count(s, substr)`          | Count occurrences               | `count("banana", "an")` → `2`                |
| `slugify(s)`                | URL-friendly string             | `slugify("Hello World!")` → `"hello-world"`  |
| `snake_case(s)`             | Convert to snake_case           | `snake_case("myAPIKey")` → `"my_api_key"`    |
| `camel_case(s)`             | Convert to camelCase            | `camel_case("hello_world")` → `"helloWorld"` |

### Core Examples

```forge
// Format table columns with padding
let items = [
    { name: "Widget", price: 9.99 },
    { name: "Gadget", price: 24.50 },
    { name: "Doohickey", price: 149.99 }
]
for each item in items {
    say "{pad_end(item.name, 15)}{pad_start(str(item.price), 8)}"
}
```

```forge
// Convert API keys between naming conventions
let api_field = "userAccountStatus"
say snake_case(api_field)    // user_account_status
say slugify(api_field)       // useraccountstatus (URL-safe)

let db_column = "created_at"
say camel_case(db_column)    // createdAt
```

---

## Chapter 24: Collection Power Tools

Beyond `map`, `filter`, and `reduce`, Forge offers a comprehensive collection toolkit. All functions support method syntax.

### Function Reference

| Function                  | Description                               | Return Type       |
| ------------------------- | ----------------------------------------- | ----------------- |
| `sum(arr)`                | Sum of numeric array                      | Int or Float      |
| `min_of(arr)`             | Minimum value                             | Int or Float      |
| `max_of(arr)`             | Maximum value                             | Int or Float      |
| `any(arr, fn)`            | True if any element satisfies predicate   | Bool              |
| `all(arr, fn)`            | True if all elements satisfy predicate    | Bool              |
| `unique(arr)`             | Deduplicate, preserve order               | Array             |
| `zip(arr1, arr2)`         | Pair elements into `[[a,b], ...]`         | Array of pairs    |
| `flatten(arr)`            | Flatten one level of nesting              | Array             |
| `group_by(arr, fn)`       | Group into object keyed by fn result      | Object            |
| `chunk(arr, size)`        | Split into sized chunks                   | Array of arrays   |
| `slice(arr, start, end?)` | Extract sub-array (supports negative idx) | Array             |
| `partition(arr, fn)`      | Split into `[matches, rest]`              | Array of 2 arrays |
| `sort(arr, fn?)`          | Sort with optional comparator (-1/0/1)    | Array             |
| `sample(arr, n?)`         | Random n items (default 1)                | Value or Array    |
| `shuffle(arr)`            | Randomize array order (Fisher-Yates)      | Array             |
| `diff(a, b)`              | Deep object comparison                    | Object or null    |

### Core Examples

```forge
let scores = [85, 92, 67, 78, 95, 88, 72, 91]
say "Total: {sum(scores)}"
say "Average: {sum(scores) / len(scores)}"
say "Best: {max_of(scores)}"
say "Worst: {min_of(scores)}"
say "All passing? {all(scores, fn(s) { return s >= 60 })}"

// Split into grade groups
let groups = group_by(scores, fn(s) {
    if s >= 90 { return "A" }
    if s >= 80 { return "B" }
    if s >= 70 { return "C" }
    return "D"
})
say "A students: {len(groups.A)}"
```

```forge
// Partition for batch processing
let users = [
    { name: "Alice", active: true },
    { name: "Bob", active: false },
    { name: "Charlie", active: true }
]
let parts = partition(users, fn(u) { return u.active })
say "Active: {len(parts[0])}"    // 2
say "Inactive: {len(parts[1])}"  // 1
```

```forge
// Diff for audit trails
let before = { name: "Alice", role: "user", email: "alice@test.com" }
let after = { name: "Alice", role: "admin", level: 5 }
let changes = diff(before, after)
// changes.role = { from: "user", to: "admin" }
// changes.email = { removed: "alice@test.com" }
// changes.level = { added: 5 }
```

---

## Chapter 25: GenZ Debug Kit

Forge's most distinctive feature: debugging and assertions with personality. These builtins do the same job as traditional tools but with memorable names and expressive error messages that make debugging less painful and more fun.

### Function Reference

| Function          | Traditional Equivalent | Behavior                                                  |
| ----------------- | ---------------------- | --------------------------------------------------------- |
| `sus(val)`        | `dbg!()` in Rust       | Print inspect info to stderr, return value (pass-through) |
| `bruh(msg)`       | `panic!()`             | Crash with "BRUH: {msg}"                                  |
| `bet(cond, msg?)` | `assert()`             | Pass: silent. Fail: "LOST THE BET: {msg}"                 |
| `no_cap(a, b)`    | `assert_eq()`          | Pass: silent. Fail: "CAP DETECTED: a != b"                |
| `ick(cond, msg?)` | `assert_false()`       | Pass if false. Fail: "ICK: {msg}" if true                 |

### Core Examples

```forge
// sus() — sprinkle through code for quick debugging
let data = http.get("https://api.example.com/users")
let users = sus(data.body)  // prints inspect, keeps flowing
let count = sus(len(users)) // prints count, keeps flowing
```

```forge
// bet/no_cap/ick — assertions with attitude
define validate_user(user) {
    bet(has_key(user, "name"), "user needs a name, bestie")
    bet(user.age >= 0, "negative age is not a vibe")
    no_cap(typeof(user.email), "String")
    ick(user.role == "superadmin", "no one should be superadmin")
}
```

```forge
// yolo — when you just don't care about errors
let result = yolo(fn() {
    return http.get("https://maybe-down.com/api").body
})
// result is None if it failed, actual data if it worked
```

---

## Chapter 26: Execution Helpers

Built-in performance profiling and resilient execution patterns — no external tools needed.

### Function Reference

| Function       | Description                                          | Return        |
| -------------- | ---------------------------------------------------- | ------------- |
| `cook(fn)`     | Time execution, print results with personality       | fn's return   |
| `yolo(fn)`     | Execute, swallow ALL errors, return None on failure  | Value or None |
| `ghost(fn)`    | Execute silently, return result                      | fn's return   |
| `slay(fn, n?)` | Benchmark n times (default 100), return stats object | Object        |

The `slay()` stats object contains:

| Field    | Type  | Description              |
| -------- | ----- | ------------------------ |
| `avg_ms` | Float | Average execution time   |
| `min_ms` | Float | Fastest run              |
| `max_ms` | Float | Slowest run              |
| `p99_ms` | Float | 99th percentile          |
| `runs`   | Int   | Number of iterations     |
| `result` | Any   | Return value of last run |

### Core Examples

```forge
// Profile a data processing pipeline
let processed = cook(fn() {
    let data = fs.read("large_file.csv")
    let rows = csv.parse(data)
    return filter(rows, fn(r) { return int(r.score) > 80 })
})
// Prints: "COOKED: done in 42.3ms — no cap that was fast"
```

```forge
// Compare two approaches
say "--- Approach A ---"
let stats_a = slay(fn() {
    return sort([5, 3, 1, 4, 2])
}, 1000)

say "--- Approach B ---"
let stats_b = slay(fn() {
    return sort([5, 3, 1, 4, 2], fn(a, b) {
        if a < b { return -1 }
        if a > b { return 1 }
        return 0
    })
}, 1000)
```

---

## Chapter 27: Advanced Testing

Forge's test framework supports decorators, hooks, assertions, and structured error handling.

### Testing Features

| Feature              | Description                                |
| -------------------- | ------------------------------------------ |
| `@test`              | Mark function as test                      |
| `@skip`              | Skip test (shown as SKIP in output)        |
| `@before`            | Run before each test in file               |
| `@after`             | Run after each test (even on failure)      |
| `assert(cond, msg?)` | Basic assertion                            |
| `assert_eq(a, b)`    | Assert equal                               |
| `assert_ne(a, b)`    | Assert not equal                           |
| `assert_throws(fn)`  | Assert function throws an error            |
| `--filter pattern`   | Run only tests whose name contains pattern |

### Structured Error Objects

When catching errors with `try/catch`, the error object has two fields:

| Field         | Type   | Description            |
| ------------- | ------ | ---------------------- |
| `err.message` | String | The error message text |
| `err.type`    | String | Error classification   |

Error types: `ArithmeticError`, `TypeError`, `ReferenceError`, `IndexError`, `AssertionError`, `RuntimeError`

### Core Examples

```forge
let mut setup_count = 0

@before
define setup() {
    setup_count = setup_count + 1
}

@after
define cleanup() {
    // Runs after every test, even on failure
}

@test
define test_math() {
    assert_eq(1 + 1, 2)
    assert_ne(1, 2)
}

@test
define test_errors() {
    assert_throws(fn() { let x = 1 / 0 })

    try { let x = 1 / 0 } catch err {
        assert_eq(err.type, "ArithmeticError")
        assert(contains(err.message, "zero"))
    }
}

@test
@skip
define test_not_ready() {
    // This won't run, shown as SKIP
}
```

Run with filter: `forge test --filter "math"` — runs only tests with "math" in the name.

---

## Chapter 28: math & fs Additions

### New math Functions

| Function                    | Description             | Example                           |
| --------------------------- | ----------------------- | --------------------------------- |
| `math.random_int(min, max)` | Random integer in range | `math.random_int(1, 6)` → `4`     |
| `math.clamp(val, min, max)` | Clamp value to range    | `math.clamp(150, 0, 100)` → `100` |

### New fs Functions

| Function                  | Description            | Example                                           |
| ------------------------- | ---------------------- | ------------------------------------------------- |
| `fs.lines(path)`          | Read as array of lines | `fs.lines("data.txt")` → `["a", "b"]`             |
| `fs.dirname(path)`        | Parent directory       | `fs.dirname("/home/user/f.txt")` → `"/home/user"` |
| `fs.basename(path)`       | Filename component     | `fs.basename("/home/user/f.txt")` → `"f.txt"`     |
| `fs.join_path(a, b, ...)` | Join path segments     | `fs.join_path("/home", "user")` → `"/home/user"`  |
| `fs.is_dir(path)`         | Is directory?          | `fs.is_dir("/tmp")` → `true`                      |
| `fs.is_file(path)`        | Is regular file?       | `fs.is_file("main.fg")` → `true`                  |
| `fs.temp_dir()`           | System temp directory  | `fs.temp_dir()` → `"/tmp"`                        |

### CLI Argument Parsing (io module)

| Function            | Description           | Example                             |
| ------------------- | --------------------- | ----------------------------------- |
| `io.args_parse()`   | Parse all CLI args    | Returns `{ verbose: true, ... }`    |
| `io.args_get(flag)` | Get single flag value | `io.args_get("--port")` → `"8080"`  |
| `io.args_has(flag)` | Check if flag present | `io.args_has("--verbose")` → `true` |

### Concurrency Additions

| Function            | Description                  | Return            |
| ------------------- | ---------------------------- | ----------------- |
| `try_send(ch, val)` | Non-blocking channel send    | Bool              |
| `try_receive(ch)`   | Non-blocking channel receive | Some(val) or None |

---

_This concludes Part II: The Standard Library. With sixteen modules, a GenZ debug kit, execution helpers, and 230+ functions at your disposal, Forge provides everything needed for file I/O, databases, data processing, HTTP, cryptography, terminal UI, fake data generation, performance profiling, shell scripting, and resilient error handling—all without leaving the language._
