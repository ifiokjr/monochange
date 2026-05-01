//! Server functions — API endpoints that run on the server.
//!
//! These use `#[server]` macros from leptos, which compile to both
//! client-side fetch calls and server-side implementations.
//!
//! The `#[server]` macro itself handles conditional compilation —
//! modules are always visible to both targets.

pub mod ai;
pub mod auth;
pub mod feedback;
pub mod repos;
pub mod roadmap;
