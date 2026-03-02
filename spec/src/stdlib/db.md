# db

SQLite database operations. Forge embeds SQLite via the `rusqlite` crate. One connection is maintained per thread.

## Functions

### db.open(path) -> bool

Opens a SQLite database at `path`. Use `":memory:"` for an in-memory database. Returns `true` on success.

```forge
db.open(":memory:")
db.open("app.db")
```

### db.execute(sql, params?) -> null

Executes a SQL statement that does not return rows (CREATE, INSERT, UPDATE, DELETE). An optional second argument provides parameterized values as an array, using `?` placeholders.

```forge
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")

// Parameterized insert (recommended)
db.execute("INSERT INTO users VALUES (?, ?, ?)", [1, "Alice", "alice@example.com"])

// Batch execution (no params)
db.execute("INSERT INTO users VALUES (2, 'Bob', 'bob@example.com')")
```

### db.query(sql, params?) -> array

Executes a SQL SELECT query and returns an array of objects. Each object maps column names to values. An optional second argument provides parameterized values.

```forge
let users = db.query("SELECT * FROM users")
// [{id: 1, name: "Alice", email: "alice@example.com"}, ...]

// Parameterized query
let result = db.query("SELECT * FROM users WHERE id = ?", [1])
say result[0].name  // "Alice"
```

### db.close() -> null

Closes the current database connection.

```forge
db.close()
```

## Full CRUD Example

```forge
// Open database
db.open(":memory:")

// Create table
db.execute("CREATE TABLE tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    done INTEGER DEFAULT 0
)")

// Create
db.execute("INSERT INTO tasks (title) VALUES (?)", ["Buy groceries"])
db.execute("INSERT INTO tasks (title) VALUES (?)", ["Write documentation"])
db.execute("INSERT INTO tasks (title) VALUES (?)", ["Deploy v2"])

// Read
let all_tasks = db.query("SELECT * FROM tasks")
say all_tasks

let pending = db.query("SELECT * FROM tasks WHERE done = ?", [0])
say len(pending)  // 3

// Update
db.execute("UPDATE tasks SET done = ? WHERE id = ?", [1, 1])

// Delete
db.execute("DELETE FROM tasks WHERE id = ?", [3])

// Verify
let remaining = db.query("SELECT * FROM tasks")
say remaining

// Close
db.close()
```

## Notes

- SQLite types map to Forge types: INTEGER -> `int`, REAL -> `float`, TEXT -> `string`, NULL -> `null`, BLOB -> `string` (as `<blob N bytes>`).
- Parameterized queries (using `?` placeholders with an array) are the recommended approach to prevent SQL injection.
