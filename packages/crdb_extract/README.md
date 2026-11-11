# crdb_extract

A small, general-purpose, **read-only** Rust library for extracting data from
[CockroachDB](https://www.cockroachlabs.com/). The connection url is read from the
`DATABASE_URL` environment variable.

CockroachDB speaks the Postgres wire protocol, so this crate is a thin layer over
[`sqlx`](https://crates.io/crates/sqlx). Rows are converted to JSON by the server
(`row_to_json`), so every column type comes back as a `serde_json::Value` with no
custom decoding code.

## Operations

| Group | Operation | Description |
|---|---|---|
| Connect | `Extractor::from_env()` | Connect using `DATABASE_URL` |
| | `Extractor::connect(url)` / `from_pool(pool)` | Explicit url or existing `PgPool` |
| | `ping()` / `server_version()` | Health check / version string |
| Query | `fetch_all(sql, params)` | All rows as `Vec<serde_json::Value>` |
| | `fetch_all_as::<T>(sql, params)` | All rows deserialized into your own struct |
| | `fetch_optional` / `fetch_one` | First row (`Option`, or `Error::NotFound`) |
| Table | `fetch_table(&TableQuery)` | Columns, filter, order, limit, offset with quoted identifiers |
| | `count(&TableQuery)` | Matching row count (limit/offset ignored) |
| Large results | `for_each(sql, params, f)` | Stream rows into a callback |
| | `export_ndjson(sql, params, writer)` | Stream rows as NDJSON to any `AsyncWrite` |
| Introspection | `list_schemas()`, `list_tables(schema)` | Via `information_schema` |
| | `describe_table(schema, table)`, `primary_key(schema, table)` | Columns and primary-key columns |
| CockroachDB | Automatic retry on SQLSTATE `40001` | Exponential backoff; `with_max_retries(n)` (default 3) |

Out of scope: writes, migrations, transactions.

## Install

```toml
[dependencies]
crdb_extract = { path = "../crdb_extract" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Usage

```rust
use crdb_extract::{Extractor, Param, TableQuery};

#[tokio::main]
async fn main() -> Result<(), crdb_extract::Error> {
    // export DATABASE_URL='postgresql://user:pw@host:26257/mydb?sslmode=verify-full'
    let db = Extractor::from_env().await?;

    // Arbitrary SQL with bind parameters
    let rows = db
        .fetch_all("SELECT id, name FROM users WHERE age > $1", &[Param::Int(30)])
        .await?;

    // Table helper
    let query = TableQuery::new("users")
        .columns(&["id", "name"])
        .filter("age > $1", vec![30.into()])
        .order_by("id", false)
        .limit(100);
    let page = db.fetch_table(&query).await?;
    let total = db.count(&query).await?;

    // Stream a big table to a file as NDJSON
    let file = tokio::fs::File::create("users.ndjson").await?;
    let written = db.export_ndjson("SELECT * FROM users", &[], file).await?;

    // Introspection
    let tables = db.list_tables("public").await?;
    let columns = db.describe_table("public", "users").await?;
    println!("{} rows, {total} total, {written} exported, {tables:?}, {columns:?}", page.len());
    Ok(())
}
```

(`tokio::fs` needs tokio's `fs` feature in your own crate.)

## Notes

- **TLS and auth** come from the url (`sslmode`, `sslrootcert`, ...); `sqlx` handles them with rustls.
- **Column names** in a query must be unique, because each row becomes a JSON object. Alias expressions with `AS`.
- **Row order:** queries are wrapped as `SELECT row_to_json(t) FROM (<your sql>) AS t`, so an `ORDER BY` inside your query is expected to carry through. Verify this on your cluster version if strict ordering matters.
- **Parameter types** are inferred by CockroachDB; add casts such as `$1::UUID` when needed.
- **Large integers:** `INT8` values above 2^53 are valid JSON numbers but may lose precision in some JSON consumers.
- **Security:** `TableQuery` quotes identifiers, but `filter(...)` and every raw `sql` argument are trusted SQL. Pass user input only through `Param`s.
- **Retries** apply to the buffered fetch methods; the streaming methods are not retried because rows may already have been handled.

## Tests

```sh
cargo test                       # unit tests run offline; live tests skip themselves
DATABASE_URL=... cargo test      # also runs the live tests against your cluster
```
