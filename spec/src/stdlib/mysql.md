# mysql

MySQL/MariaDB database operations with parameterized queries and connection pooling. Uses `mysql_async` with `rustls` TLS under the hood. Supports multiple simultaneous connections via connection IDs.

## Functions

### mysql.connect(url) -> string

### mysql.connect(host, user, pass, db) -> string

Connects to a MySQL database and returns a connection ID string. Supports both URL and multi-argument forms.

```forge
// URL form
let conn = mysql.connect("mysql://root:password@localhost:3306/mydb")

// Multi-argument form
let conn = mysql.connect("localhost", "root", "password", "mydb")
```

The returned connection ID (e.g., `"mysql_1"`) is used in all subsequent operations. Multiple connections can be open simultaneously.

### mysql.query(conn, sql, params?) -> array

Executes a SQL SELECT query and returns an array of objects. Each object maps column names to values. Supports parameterized queries with `?` placeholders.

```forge
// Simple query
let users = mysql.query(conn, "SELECT * FROM users")

// Parameterized query (prevents SQL injection)
let users = mysql.query(conn, "SELECT * FROM users WHERE age > ? AND city = ?", [21, "Portland"])
for user in users {
    say user.name + " (age " + str(user.age) + ")"
}
```

### mysql.execute(conn, sql, params?) -> int

Executes a SQL statement that does not return rows (INSERT, UPDATE, DELETE, CREATE, etc.). Returns the number of affected rows. Supports parameterized queries.

```forge
mysql.execute(conn, "INSERT INTO users (name, age) VALUES (?, ?)", ["Alice", 30])

let affected = mysql.execute(conn, "UPDATE users SET age = ? WHERE name = ?", [31, "Alice"])
say affected  // 1

mysql.execute(conn, "DELETE FROM users WHERE id = ?", [5])
```

### mysql.close(conn) -> bool

Closes the connection pool and removes it. Returns `true` if the connection was found and closed, `false` if the connection ID was not found.

```forge
mysql.close(conn)
```

## Example

```forge
// Connect to MySQL
let conn = mysql.connect("mysql://root:password@localhost/mydb")

// Create table
mysql.execute(conn, "CREATE TABLE IF NOT EXISTS products (
    id INT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    price DECIMAL(10,2),
    in_stock BOOLEAN DEFAULT true
)")

// Insert with parameterized queries
mysql.execute(conn, "INSERT INTO products (name, price) VALUES (?, ?)", ["Widget", 9.99])
mysql.execute(conn, "INSERT INTO products (name, price) VALUES (?, ?)", ["Gadget", 24.99])
mysql.execute(conn, "INSERT INTO products (name, price) VALUES (?, ?)", ["Gizmo", 14.50])

// Query with parameters
let cheap = mysql.query(conn, "SELECT * FROM products WHERE price < ? ORDER BY price", [20.0])
for p in cheap {
    say p.name + " - $" + str(p.price)
}
// Widget - $9.99
// Gizmo - $14.50

// Update
let updated = mysql.execute(conn, "UPDATE products SET price = ? WHERE name = ?", [12.99, "Widget"])
say "Updated " + str(updated) + " row(s)"

// Clean up
mysql.execute(conn, "DROP TABLE products")
mysql.close(conn)
```

## Multiple Connections

```forge
let prod = mysql.connect("mysql://root@prod-server/app")
let analytics = mysql.connect("mysql://root@analytics-server/warehouse")

let users = mysql.query(prod, "SELECT * FROM users")
let events = mysql.query(analytics, "SELECT * FROM events")

mysql.close(prod)
mysql.close(analytics)
```

## Type Mapping

| MySQL Type                | Forge Type |
| ------------------------- | ---------- |
| INT, BIGINT               | `int`      |
| TINYINT(1), BOOLEAN       | `int`      |
| FLOAT, DOUBLE, DECIMAL    | `float`    |
| VARCHAR, TEXT, CHAR       | `string`   |
| DATETIME, TIMESTAMP, DATE | `string`   |
| TIME                      | `string`   |
| BLOB, BINARY              | `string`   |
| NULL                      | `null`     |

## Notes

- All queries support parameterized placeholders (`?`) to prevent SQL injection. Always use parameters for user-provided data.
- Connection pooling is handled automatically — each `mysql.connect` call creates a pool that manages reconnection and connection reuse.
- TLS is supported via `rustls` when connecting to remote servers.
- The `mysql` module requires the interpreter (default) execution mode. It is not available in the VM or JIT tiers.
