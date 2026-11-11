//! Tiny CockroachDB extractor: one query string in, JSON built by the database out.
//!
//! ```no_run
//! # async fn demo() -> Result<(), crdb_extract::Error> {
//! let db = crdb_extract::Extractor::from_env().await?; // reads DATABASE_URL
//! let users = db.fetch_json("users").await?; // whole table
//! let adults = db.fetch_json("SELECT id, name FROM users WHERE age >= 18").await?;
//! # let _ = (users, adults);
//! # Ok(())
//! # }
//! ```

use backon::{ExponentialBuilder, Retryable};
use serde_json::Value;
use sqlx::PgPool;

pub use sqlx::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Environment variable holding the CockroachDB connection url.
pub const DATABASE_URL_ENV: &str = "DATABASE_URL";

/// SQLSTATE CockroachDB returns when a statement must be retried.
const SERIALIZATION_FAILURE: &str = "40001";

#[derive(Debug, Clone)]
pub struct Extractor {
    pool: PgPool,
}

impl Extractor {
    /// Connects using the url in `DATABASE_URL`.
    pub async fn from_env() -> Result<Self> {
        let url = std::env::var(DATABASE_URL_ENV).map_err(|e| Error::Configuration(Box::new(e)))?;
        Self::connect(&url).await
    }

    /// Connects using an explicit url.
    pub async fn connect(url: &str) -> Result<Self> {
        Ok(Self { pool: PgPool::connect(url).await? })
    }

    /// Runs `query` and returns all rows as one JSON array of objects, built by the database.
    ///
    /// `query` is either a bare table name (`"users"`, `"app.users"`), which returns the
    /// whole table, or a `SELECT` statement. An empty result is `[]`. Result columns must
    /// have unique names. Retried automatically on serialization failures (SQLSTATE 40001).
    ///
    /// The query is trusted SQL: never build it from untrusted input.
    pub async fn fetch_json(&self, query: &str) -> Result<Value> {
        let sql = to_json_sql(query);
        let sql = sql.as_str();
        (move || async move { sqlx::query_scalar::<sqlx::Postgres, Value>(sql).fetch_one(&self.pool).await })
            .retry(ExponentialBuilder::default().with_max_times(3))
            .when(is_retryable)
            .await
    }
}

fn to_json_sql(input: &str) -> String {
    let query = input.trim().trim_end_matches(';').trim_end();
    let query = if query.split_whitespace().count() == 1 {
        format!("SELECT * FROM {query}")
    } else {
        query.to_owned()
    };
    format!("SELECT coalesce(jsonb_agg(row_to_json(t)), '[]'::JSONB) FROM ({query}) AS t")
}

fn is_retryable(err: &Error) -> bool {
    match err {
        Error::Database(db) => db.code().as_deref() == Some(SERIALIZATION_FAILURE),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PREFIX: &str = "SELECT coalesce(jsonb_agg(row_to_json(t)), '[]'::JSONB) FROM (";

    #[test]
    fn bare_table_name_selects_whole_table() {
        assert_eq!(to_json_sql("users"), format!("{PREFIX}SELECT * FROM users) AS t"));
        assert_eq!(to_json_sql(" app.users; "), format!("{PREFIX}SELECT * FROM app.users) AS t"));
    }

    #[test]
    fn query_is_wrapped_and_semicolon_stripped() {
        assert_eq!(
            to_json_sql("SELECT id FROM users WHERE id > 5 ;\n"),
            format!("{PREFIX}SELECT id FROM users WHERE id > 5) AS t")
        );
    }

    async fn live() -> Extractor {
        Extractor::from_env()
            .await
            .expect("DATABASE_URL should be set to a reachable CockroachDB")
    }

    #[tokio::test]
    #[ignore = "needs a live CockroachDB at DATABASE_URL"]
    async fn live_query_returns_json_array() {
        let rows = live()
            .await
            .fetch_json("SELECT 1 AS n, 'a' AS s")
            .await
            .expect("literal SELECT should succeed");
        assert_eq!(rows, json!([{ "n": 1, "s": "a" }]), "unexpected JSON result");
    }

    #[tokio::test]
    #[ignore = "needs a live CockroachDB at DATABASE_URL"]
    async fn live_empty_result_is_empty_array() {
        let rows = live()
            .await
            .fetch_json("SELECT 1 AS n WHERE false")
            .await
            .expect("empty SELECT should succeed");
        assert_eq!(rows, json!([]), "empty result should be an empty array");
    }
}