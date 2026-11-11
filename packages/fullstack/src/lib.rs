mod layout;

pub fn main_fullstack() {
    // 1. Client Side
    #[cfg(not(feature = "server"))]
    dioxus::launch(layout::App);

    // 2. Server side
    #[cfg(feature = "server")]
    {
        use crdb_extract::Extractor;
        dioxus::serve(|| async move {
            // Connect from env var DATABASE_URL
            let db = Extractor::from_env()
                .await
                .expect("Failed to connect to CockroachDB, please check env var DATABASE_URL");
            // Verify DB connection health
            db.ping().await.expect("Database ping failed");
            // Create router
            let router = dioxus::server::router(layout::App);
            Ok(router)
        });
    }
}