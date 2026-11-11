//! Runs against a real CockroachDB when `DATABASE_URL` is set; otherwise skips.

use crdb_extract::{Extractor, Param};

#[tokio::test]
async fn select_roundtrip() {
    if std::env::var("DATABASE_URL").is_err() {
        eprintln!("DATABASE_URL not set; skipping live test");
        return;
    }

    let db = Extractor::from_env()
        .await
        .expect("should connect using DATABASE_URL");

    let rows = db
        .fetch_all("SELECT $1::INT8 AS n, 'a' AS s", &[Param::Int(7)])
        .await
        .expect("literal SELECT should succeed");

    assert_eq!(rows.len(), 1, "expected exactly one row");
    assert_eq!(rows[0]["n"], 7, "bound parameter should round-trip");
    assert_eq!(rows[0]["s"], "a", "text literal should round-trip");
}

#[tokio::test]
async fn introspection_lists_schemas() {
    if std::env::var("DATABASE_URL").is_err() {
        eprintln!("DATABASE_URL not set; skipping live test");
        return;
    }

    let db = Extractor::from_env()
        .await
        .expect("should connect using DATABASE_URL");

    let schemas = db
        .list_schemas()
        .await
        .expect("listing schemas should succeed");

    assert!(
        schemas.iter().any(|s| s == "public"),
        "expected the public schema to exist, got {schemas:?}"
    );
}
