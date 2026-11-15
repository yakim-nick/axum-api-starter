mod auth;
mod health;
mod notes;

use axum::Router;
use axum::routing::{get, post};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Maximum request body size (bytes). Larger bodies get 413.
const MAX_BODY_BYTES: usize = 64 * 1024;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/me", get(auth::me))
        .route("/notes", post(notes::create).get(notes::list))
        .route(
            "/notes/{id}",
            get(notes::get_one)
                .patch(notes::update)
                .delete(notes::delete),
        )
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
