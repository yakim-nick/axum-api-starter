//! REST API starter: Axum + SQLx (Postgres) + JWT auth.
//!
//! Layers: `routes` (HTTP, request/response shapes) -> `validation` (input rules)
//! -> `repo` (all SQL). Cross-cutting: `auth` (hashing, tokens, extractor),
//! `error` (one error type with one JSON shape), `config`.

pub mod auth;
pub mod config;
pub mod error;
pub mod extract;
pub mod repo;
pub mod routes;
pub mod state;
pub mod validation;

pub use routes::router;
pub use state::AppState;
