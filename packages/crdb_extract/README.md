# crdb_extract

A tiny Rust library that takes **one query string** and returns the result from
CockroachDB as **JSON built by the database itself**. The connection url is read
from the `DATABASE_URL` environment variable.

CockroachDB speaks the Postgres wire protocol, so this crate is a thin layer over
[`sqlx`](https://crates.io/crates/sqlx), with [`backon`](https://crates.io/crates/backon)
for retries. There is no per-column decoding: the query is wrapped as

```sql
SELECT coalesce(jsonb_agg(row_to_json(t)), '[]'::JSONB) FROM (<your query>) AS t
```

so every column type CockroachDB supports arrives as ordinary JSON.

## API

| Function | Description |
|---|---|
| `Extractor::from_env()` | Connect using `DATABASE_URL` |
| `Extractor::connect(url)` | Connect using an explicit url |
| `extractor.fetch_json(query)` | Run `query`, return a `serde_json::Value` array of row objects |

`query` is either:

- a **bare table name** (`"users"` or `"app.users"`), which returns the whole table, or
- a **`SELECT` statement**, which returns its rows.

An empty result is `[]`. Serialization failures (SQLSTATE `40001`) are retried automatically with exponential backoff (up to 3 times).

## Usage

```rust
use crdb_extract::Extractor;

#[tokio::main]
async fn main() -> Result<(), crdb_extract::Error> {
    // export DATABASE_URL='postgresql://user:pw@host:26257/mydb?sslmode=verify-full'
    let db = Extractor::from_env().await?;

    let whole_table = db.fetch_json("users").await?;
    let adults = db
        .fetch_json("SELECT id, name FROM users WHERE age >= 18 ORDER BY id")
        .await?;

    println!("{whole_table}\n{adults}");
    Ok(())
}
```

## Notes

- **Trusted SQL only.** The query string is run as-is. Never build it from untrusted input.
- **Unique column names.** Each row becomes a JSON object, so alias expressions with `AS`.
- **Whole result in memory.** The database returns one JSON document, so very large tables will use a lot of memory on both sides. Limit with `LIMIT`/`WHERE` or page through the table.
- **Row order.** An `ORDER BY` inside your query is expected to carry through the wrapper; verify on your cluster version if strict order matters.
- **TLS and auth** come from the url (`sslmode`, `sslrootcert`, ...).
- **Errors** are plain `sqlx::Error` (re-exported as `crdb_extract::Error`); a missing `DATABASE_URL` is reported as `Error::Configuration`.

## Tests

```sh
cargo test                              # unit tests only (offline); live tests show as ignored
DATABASE_URL=... cargo test -- --ignored   # runs the live tests against your cluster
```