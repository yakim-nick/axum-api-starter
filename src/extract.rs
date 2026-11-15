//! Thin wrappers around axum's `Json`, `Path` and `Query` extractors whose failures
//! are reported with the API's own JSON error shape instead of axum's plain text.

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts};
use axum::http::StatusCode;

use crate::error::AppError;

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct ApiJson<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(AppError))]
pub struct ApiPath<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(AppError))]
pub struct ApiQuery<T>(pub T);

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        // The body-size limit surfaces here when the client sent no Content-Length.
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            return AppError::PayloadTooLarge;
        }
        AppError::BadRequest(format!("invalid JSON body: {}", rejection.body_text()))
    }
}

impl From<PathRejection> for AppError {
    fn from(rejection: PathRejection) -> Self {
        AppError::BadRequest(format!("invalid path parameter: {}", rejection.body_text()))
    }
}

impl From<QueryRejection> for AppError {
    fn from(rejection: QueryRejection) -> Self {
        AppError::BadRequest(format!("invalid query string: {}", rejection.body_text()))
    }
}
