//! monochange_app — WASM client entrypoint.
//!
//! This is compiled to WebAssembly and hydrates the Leptos app
//! on the client side for interactivity after SSR.

pub mod app;
pub mod color_mode;
pub mod components;
pub mod error;
pub mod pages;
pub mod server_fns;

pub use app::App;

#[cfg(test)]
#[path = "__tests.rs"]
mod tests;

/// Hydrate the Leptos app on the client (WASM only).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    _ = console_log::init_with_level(log::Level::Debug);

    leptos::mount::hydrate_body(app::App);
}
