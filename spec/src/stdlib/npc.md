# npc

Fake data generation for testing, prototyping, and seeding databases. "NPC" stands for Non-Player Character -- these are the background characters in your application.

## Functions

### npc.name() -> string

Returns a random full name (first + last).

```forge
say npc.name()  // e.g. "Luna Nakamura"
```

### npc.first_name() -> string

Returns a random first name from a diverse, gender-neutral pool.

```forge
say npc.first_name()  // e.g. "Phoenix"
```

### npc.last_name() -> string

Returns a random last name from a globally diverse pool.

```forge
say npc.last_name()  // e.g. "Patel"
```

### npc.email() -> string

Returns a random email address.

```forge
say npc.email()  // e.g. "luna.garcia42@proton.me"
```

### npc.username() -> string

Returns a random username in the format `adjective_noun123`.

```forge
say npc.username()  // e.g. "turbo_wizard847"
```

### npc.phone() -> string

Returns a random US-format phone number.

```forge
say npc.phone()  // e.g. "(555) 234-5678"
```

### npc.number(min?, max?) -> int

Returns a random integer. Defaults to range 0-100.

```forge
npc.number()        // 0-100
npc.number(1, 6)    // dice roll
npc.number(1000, 9999)  // 4-digit number
```

### npc.pick(array) -> any

Returns a random element from the given array.

```forge
let color = npc.pick(["red", "green", "blue"])
say color  // e.g. "green"
```

### npc.bool() -> bool

Returns a random boolean.

```forge
say npc.bool()  // true or false
```

### npc.sentence(word_count?) -> string

Returns a random sentence. Default word count is 5-12.

```forge
say npc.sentence()    // e.g. "The quick data flows through every node."
say npc.sentence(5)   // exactly 5 words
```

### npc.word() -> string

Returns a single random word.

```forge
say npc.word()  // e.g. "algorithms"
```

### npc.id() -> string

Returns a random UUID-like identifier (format: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`).

```forge
say npc.id()  // e.g. "a3b2f1c8-d4e5-f6a7-b8c9-d0e1f2a3b4c5"
```

### npc.color() -> string

Returns a random hex color code.

```forge
say npc.color()  // e.g. "#3a7fb2"
```

### npc.ip() -> string

Returns a random IPv4 address.

```forge
say npc.ip()  // e.g. "192.168.45.12"
```

### npc.url() -> string

Returns a random URL.

```forge
say npc.url()  // e.g. "https://techflow.io/dashboard"
```

### npc.company() -> string

Returns a random company name.

```forge
say npc.company()  // e.g. "QuantumLeap"
```

## Example: Seeding a Database

```forge
db.open(":memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")

repeat 100 times {
    db.execute("INSERT INTO users (name, email) VALUES (?, ?)", [
        npc.name(),
        npc.email()
    ])
}

let users = db.query("SELECT * FROM users LIMIT 5")
term.table(users)
```
