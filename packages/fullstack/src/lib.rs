mod layout;

pub fn main_fullstack() {
    // 1. Client Side
    #[cfg(not(feature = "server"))]
    // It also uses the exact same `App` imported from the shared crate
    dioxus::launch(layout::App);

    // 2. Server side
    #[cfg(feature = "server")]
    dioxus::serve(|| async move {
        // It uses `App` imported from the shared crate
        let router = dioxus::server::router(layout::App);
        Ok(router)
    });
}
