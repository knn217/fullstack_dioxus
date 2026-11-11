//! Small, general-purpose, read-only data extraction for CockroachDB.
//!
//! CockroachDB speaks the Postgres wire protocol, so this crate is a thin layer
//! over [`sqlx`]. Rows are converted to JSON by the server (`row_to_json`), which
//! means every column type CockroachDB supports comes back as a
//! [`serde_json::Value`] without any custom decoding.
//!
//! ```no_run
//! # async fn demo() -> Result<(), crdb_extract::Error> {
//! use crdb_extract::{Extractor, Param};
//!
//! let db = Extractor::from_env().await?; // reads DATABASE_URL
//! let rows = db
//!     .fetch_all("SELECT id, name FROM users WHERE age > $1", &[Param::Int(30)])
//!     .await?;
//! println!("{} rows", rows.len());
//! # Ok(())
//! # }
//! ```

use std::future::Future;
use std::time::Duration;

use futures_util::stream::BoxStream;
use futures_util::{StreamExt, TryStreamExt};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use sqlx::postgres::{PgArguments, PgPoolOptions, PgRow};
use sqlx::query::Query;
use sqlx::{PgPool, Postgres, Row};
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// Name of the environment variable holding the CockroachDB connection url.
pub const DATABASE_URL_ENV: &str = "DATABASE_URL";

/// SQLSTATE CockroachDB returns when a transaction must be retried.
const SERIALIZATION_FAILURE: &str = "40001";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("environment variable DATABASE_URL is not set or is not valid unicode")]
    MissingDatabaseUrl,
    #[error("database error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("query returned no rows")]
    NotFound,
    #[error("result did not have the expected shape (missing or mistyped field `{0}`)")]
    UnexpectedShape(&'static str),
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
}

/// A bind parameter for `$1`, `$2`, ... placeholders.
///
/// CockroachDB infers placeholder types from context; add a cast
/// (e.g. `$1::INT8`, `$1::UUID`) when the context is ambiguous.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Json(Value),
}

impl From<bool> for Param {
    fn from(v: bool) -> Self {
        Param::Bool(v)
    }
}
impl From<i64> for Param {
    fn from(v: i64) -> Self {
        Param::Int(v)
    }
}
impl From<i32> for Param {
    fn from(v: i32) -> Self {
        Param::Int(i64::from(v))
    }
}
impl From<f64> for Param {
    fn from(v: f64) -> Self {
        Param::Float(v)
    }
}
impl From<&str> for Param {
    fn from(v: &str) -> Self {
        Param::Text(v.to_owned())
    }
}
impl From<String> for Param {
    fn from(v: String) -> Self {
        Param::Text(v)
    }
}
impl From<Value> for Param {
    fn from(v: Value) -> Self {
        Param::Json(v)
    }
}

/// Column metadata returned by [`Extractor::describe_table`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ColumnInfo {
    pub column_name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub column_default: Option<String>,
}

/// Builder for a simple single-table `SELECT`.
///
/// Identifiers (schema, table, columns) are quoted. The `filter` clause is raw
/// SQL with `$n` placeholders and must come from trusted code, never user input.
#[derive(Debug, Clone)]
pub struct TableQuery {
    schema: String,
    table: String,
    columns: Vec<String>,
    filter: Option<(String, Vec<Param>)>,
    order_by: Vec<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

impl TableQuery {
    /// Starts a query on `public.<table>`.
    pub fn new(table: &str) -> Self {
        Self {
            schema: "public".to_owned(),
            table: table.to_owned(),
            columns: Vec::new(),
            filter: None,
            order_by: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    pub fn schema(mut self, schema: &str) -> Self {
        self.schema = schema.to_owned();
        self
    }

    /// Restricts the selected columns (default: all).
    pub fn columns(mut self, columns: &[&str]) -> Self {
        self.columns = columns.iter().map(|c| quote_ident(c)).collect();
        self
    }

    /// Raw `WHERE` condition, e.g. `filter("age > $1 AND city = $2", vec![30.into(), "Oslo".into()])`.
    pub fn filter(mut self, clause: &str, params: Vec<Param>) -> Self {
        self.filter = Some((clause.to_owned(), params));
        self
    }

    /// Adds an `ORDER BY` column; call repeatedly for several.
    pub fn order_by(mut self, column: &str, descending: bool) -> Self {
        let dir = if descending { "DESC" } else { "ASC" };
        self.order_by.push(format!("{} {dir}", quote_ident(column)));
        self
    }

    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    fn from_where(&self) -> String {
        let mut sql = format!("FROM {}.{}", quote_ident(&self.schema), quote_ident(&self.table));
        if let Some((clause, _)) = &self.filter {
            sql.push_str(" WHERE ");
            sql.push_str(clause);
        }
        sql
    }

    fn params(&self) -> &[Param] {
        match &self.filter {
            Some((_, params)) => params,
            None => &[],
        }
    }

    /// The generated SQL (useful for logging/debugging).
    pub fn sql(&self) -> String {
        let cols = if self.columns.is_empty() {
            "*".to_owned()
        } else {
            self.columns.join(", ")
        };
        let mut sql = format!("SELECT {cols} {}", self.from_where());
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&self.order_by.join(", "));
        }
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {offset}"));
        }
        sql
    }
}

/// Read-only CockroachDB extractor backed by a connection pool.
#[derive(Debug, Clone)]
pub struct Extractor {
    pool: PgPool,
    max_retries: u32,
}

impl Extractor {
    /// Connects using the url in the `DATABASE_URL` environment variable.
    pub async fn from_env() -> Result<Self> {
        let url = std::env::var(DATABASE_URL_ENV).map_err(|_| Error::MissingDatabaseUrl)?;
        Self::connect(&url).await
    }

    /// Connects using an explicit url (`postgresql://user:pw@host:26257/db?sslmode=verify-full`).
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
        Ok(Self::from_pool(pool))
    }

    /// Wraps an existing pool (use this to control pool size, timeouts, etc.).
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool, max_retries: 3 }
    }

    /// Maximum automatic retries on serialization failures (SQLSTATE 40001). Default 3.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ping(&self) -> Result<()> {
        self.retrying(move || async move { sqlx::query("SELECT 1").execute(&self.pool).await })
            .await?;
        Ok(())
    }

    pub async fn server_version(&self) -> Result<String> {
        let row = self.fetch_one("SELECT version() AS version", &[]).await?;
        row.get("version")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(Error::UnexpectedShape("version"))
    }

    // ---------------------------------------------------------------- queries

    /// Runs a `SELECT` and returns every row as a JSON object.
    ///
    /// Result columns must have unique names (alias expressions with `AS`).
    /// Retried automatically on serialization failures.
    pub async fn fetch_all(&self, sql: &str, params: &[Param]) -> Result<Vec<Value>> {
        let wrapped = wrap_json(sql);
        let wrapped = wrapped.as_str();
        let rows = self
            .retrying(move || async move {
                bind_all(sqlx::query(wrapped), params).fetch_all(&self.pool).await
            })
            .await?;
        rows.iter().map(first_json).collect()
    }

    /// Like [`fetch_all`](Self::fetch_all) but deserializes each row into `T`.
    pub async fn fetch_all_as<T: DeserializeOwned>(&self, sql: &str, params: &[Param]) -> Result<Vec<T>> {
        self.fetch_all(sql, params)
            .await?
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(Error::from))
            .collect()
    }

    /// First row of the result, or `None`.
    pub async fn fetch_optional(&self, sql: &str, params: &[Param]) -> Result<Option<Value>> {
        let wrapped = wrap_json(sql);
        let wrapped = wrapped.as_str();
        let row = self
            .retrying(move || async move {
                bind_all(sqlx::query(wrapped), params).fetch_optional(&self.pool).await
            })
            .await?;
        row.as_ref().map(first_json).transpose()
    }

    /// First row of the result; [`Error::NotFound`] if there is none.
    pub async fn fetch_one(&self, sql: &str, params: &[Param]) -> Result<Value> {
        self.fetch_optional(sql, params).await?.ok_or(Error::NotFound)
    }

    // ------------------------------------------------------------- table api

    pub async fn fetch_table(&self, query: &TableQuery) -> Result<Vec<Value>> {
        self.fetch_all(&query.sql(), query.params()).await
    }

    /// Number of rows matching the query's filter (limit/offset ignored).
    pub async fn count(&self, query: &TableQuery) -> Result<u64> {
        let sql = format!("SELECT count(*) AS count {}", query.from_where());
        let row = self.fetch_one(&sql, query.params()).await?;
        row.get("count")
            .and_then(Value::as_u64)
            .ok_or(Error::UnexpectedShape("count"))
    }

    // -------------------------------------------------------------- streaming

    /// Streams rows into `f` one at a time without buffering the result set.
    /// Returns the number of rows processed. Not retried (rows may already have been handled).
    pub async fn for_each<F>(&self, sql: &str, params: &[Param], mut f: F) -> Result<u64>
    where
        F: FnMut(Value) -> Result<()>,
    {
        let wrapped = wrap_json(sql);
        let mut rows = self.json_rows(&wrapped, params);
        let mut count = 0u64;
        while let Some(row) = rows.try_next().await? {
            f(row)?;
            count += 1;
        }
        Ok(count)
    }

    /// Streams rows as newline-delimited JSON into `writer`. Returns the row count.
    pub async fn export_ndjson<W>(&self, sql: &str, params: &[Param], mut writer: W) -> Result<u64>
    where
        W: AsyncWrite + Unpin,
    {
        let wrapped = wrap_json(sql);
        let mut rows = self.json_rows(&wrapped, params);
        let mut count = 0u64;
        while let Some(row) = rows.try_next().await? {
            let mut line = serde_json::to_vec(&row)?;
            line.push(b'\n');
            writer.write_all(&line).await?;
            count += 1;
        }
        writer.flush().await?;
        Ok(count)
    }

    // ----------------------------------------------------------- introspection

    pub async fn list_schemas(&self) -> Result<Vec<String>> {
        let rows = self
            .fetch_all(
                "SELECT schema_name FROM information_schema.schemata ORDER BY schema_name",
                &[],
            )
            .await?;
        strings(rows, "schema_name")
    }

    pub async fn list_tables(&self, schema: &str) -> Result<Vec<String>> {
        let rows = self
            .fetch_all(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = $1 AND table_type = 'BASE TABLE' ORDER BY table_name",
                &[Param::from(schema)],
            )
            .await?;
        strings(rows, "table_name")
    }

    pub async fn describe_table(&self, schema: &str, table: &str) -> Result<Vec<ColumnInfo>> {
        self.fetch_all_as(
            "SELECT column_name, data_type, (is_nullable = 'YES') AS is_nullable, column_default \
             FROM information_schema.columns \
             WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position",
            &[Param::from(schema), Param::from(table)],
        )
        .await
    }

    /// Primary-key column names in key order (empty if the table has none).
    pub async fn primary_key(&self, schema: &str, table: &str) -> Result<Vec<String>> {
        let rows = self
            .fetch_all(
                "SELECT kcu.column_name AS column_name \
                 FROM information_schema.table_constraints tc \
                 JOIN information_schema.key_column_usage kcu \
                   ON tc.constraint_name = kcu.constraint_name \
                  AND tc.table_schema = kcu.table_schema \
                  AND tc.table_name = kcu.table_name \
                 WHERE tc.constraint_type = 'PRIMARY KEY' \
                   AND tc.table_schema = $1 AND tc.table_name = $2 \
                 ORDER BY kcu.ordinal_position",
                &[Param::from(schema), Param::from(table)],
            )
            .await?;
        strings(rows, "column_name")
    }

    // --------------------------------------------------------------- internals

    fn json_rows<'a>(&'a self, wrapped: &'a str, params: &'a [Param]) -> BoxStream<'a, Result<Value>> {
        bind_all(sqlx::query(wrapped), params)
            .fetch(&self.pool)
            .map_err(Error::from)
            .and_then(|row| futures_util::future::ready(first_json(&row)))
            .boxed()
    }

    /// Re-runs `op` with exponential backoff while CockroachDB asks for a retry.
    async fn retrying<T, F, Fut>(&self, mut op: F) -> std::result::Result<T, sqlx::Error>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = std::result::Result<T, sqlx::Error>>,
    {
        let mut attempt = 0u32;
        loop {
            match op().await {
                Err(err) if attempt < self.max_retries && is_retryable(&err) => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(50u64 << attempt.min(6))).await;
                }
                other => return other,
            }
        }
    }
}

/// Quotes a SQL identifier (`my"col` becomes `"my""col"`).
pub fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

fn wrap_json(sql: &str) -> String {
    let inner = sql.trim().trim_end_matches(';').trim_end();
    format!("SELECT row_to_json(t)::JSONB FROM ({inner}) AS t")
}

fn bind_all<'q>(
    mut query: Query<'q, Postgres, PgArguments>,
    params: &'q [Param],
) -> Query<'q, Postgres, PgArguments> {
    for param in params {
        query = match param {
            Param::Bool(v) => query.bind(*v),
            Param::Int(v) => query.bind(*v),
            Param::Float(v) => query.bind(*v),
            Param::Text(v) => query.bind(v.as_str()),
            Param::Json(v) => query.bind(v.clone()),
        };
    }
    query
}

fn first_json(row: &PgRow) -> Result<Value> {
    Ok(row.try_get::<Value, usize>(0)?)
}

fn is_retryable(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => db.code().as_deref() == Some(SERIALIZATION_FAILURE),
        _ => false,
    }
}

fn strings(rows: Vec<Value>, key: &'static str) -> Result<Vec<String>> {
    rows.into_iter()
        .map(|row| {
            row.get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(Error::UnexpectedShape(key))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_json_strips_trailing_semicolons() {
        assert_eq!(
            wrap_json("  SELECT 1 AS a ; \n"),
            "SELECT row_to_json(t)::JSONB FROM (SELECT 1 AS a) AS t"
        );
    }

    #[test]
    fn quote_ident_escapes_quotes() {
        assert_eq!(quote_ident("plain"), "\"plain\"");
        assert_eq!(quote_ident("we\"ird"), "\"we\"\"ird\"");
    }

    #[test]
    fn table_query_defaults_to_select_star() {
        assert_eq!(TableQuery::new("users").sql(), "SELECT * FROM \"public\".\"users\"");
    }

    #[test]
    fn table_query_builds_full_statement() {
        let query = TableQuery::new("users")
            .schema("app")
            .columns(&["id", "name"])
            .filter("age > $1", vec![Param::Int(30)])
            .order_by("id", true)
            .limit(10)
            .offset(20);
        assert_eq!(
            query.sql(),
            "SELECT \"id\", \"name\" FROM \"app\".\"users\" WHERE age > $1 \
             ORDER BY \"id\" DESC LIMIT 10 OFFSET 20"
        );
        assert_eq!(query.params(), &[Param::Int(30)]);
    }

    #[test]
    fn params_convert_from_common_types() {
        assert_eq!(Param::from(7), Param::Int(7));
        assert_eq!(Param::from("x"), Param::Text("x".to_owned()));
        assert_eq!(Param::from(true), Param::Bool(true));
    }
}
