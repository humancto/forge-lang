# pg

PostgreSQL database operations. Requires a running PostgreSQL server. Uses `tokio-postgres` under the hood and requires the async runtime (interpreter mode).

## Functions

### pg.connect(connection_string) -> bool

Connects to a PostgreSQL database. Returns `true` on success. The connection string follows the standard PostgreSQL format.

```forge
pg.connect("host=localhost user=postgres password=secret dbname=myapp")
```

### pg.query(sql) -> array

Executes a SQL SELECT query and returns an array of objects. Each object maps column names to values.

```forge
let users = pg.query("SELECT id, name, email FROM users")
for user in users {
    say user.name
}
```

### pg.execute(sql) -> int

Executes a SQL statement that does not return rows. Returns the number of rows affected.

```forge
let count = pg.execute("UPDATE users SET active = true WHERE last_login > now() - interval '30 days'")
say count  // number of rows updated
```

### pg.close() -> null

Closes the current PostgreSQL connection.

```forge
pg.close()
```

## Example

```forge
pg.connect("host=localhost dbname=shop user=postgres")

pg.execute("CREATE TABLE IF NOT EXISTS products (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    price NUMERIC(10,2)
)")

pg.execute("INSERT INTO products (name, price) VALUES ('Widget', 9.99)")
pg.execute("INSERT INTO products (name, price) VALUES ('Gadget', 24.99)")

let products = pg.query("SELECT * FROM products ORDER BY price")
for p in products {
    say p.name + " - $" + str(p.price)
}

pg.close()
```

## Notes

- PostgreSQL types map to Forge types: `integer`/`bigint` -> `int`, `real`/`double precision`/`numeric` -> `float`, `text`/`varchar` -> `string`, `boolean` -> `bool`, NULL -> `null`.
- The `pg` module requires the interpreter (default) execution mode. It is not available in the VM or JIT tiers.
