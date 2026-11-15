//! End-to-end API tests. Each test gets its own freshly migrated Postgres database
//! (`sqlx::test`), so tests are isolated and can run in parallel.
//! Needs `DATABASE_URL` pointing at a Postgres the user may create databases in.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum_api_starter::auth::Claims;
use axum_api_starter::config::Config;
use axum_api_starter::{AppState, router};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "test-secret-test-secret-test-secret-123";

fn app(pool: PgPool) -> Router {
    let config = Config {
        database_url: String::new(),
        jwt_secret: SECRET.into(),
        jwt_ttl_secs: 3600,
        port: 0,
    };
    router(AppState {
        pool,
        config: Arc::new(config),
    })
}

/// Send one request, return the status and the parsed JSON body (Null when empty).
async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn register(app: &Router, email: &str) -> String {
    let (status, body) = call(
        app,
        Method::POST,
        "/auth/register",
        None,
        Some(json!({ "email": email, "password": "correct-horse-battery" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["token"].as_str().unwrap().to_string()
}

async fn create_note(app: &Router, token: &str, title: &str) -> Value {
    let (status, body) = call(
        app,
        Method::POST,
        "/notes",
        Some(token),
        Some(json!({ "title": title, "body": "text" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

#[sqlx::test]
async fn health_reports_ok_when_the_database_is_reachable(pool: PgPool) {
    let (status, body) = call(&app(pool), Method::GET, "/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[sqlx::test]
async fn register_login_and_me(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "Ann@Example.com").await;

    let (status, me) = call(&app, Method::GET, "/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        me["email"], "ann@example.com",
        "email is stored lower-cased"
    );
    assert!(
        me.get("password_hash").is_none(),
        "the hash must never be returned"
    );

    let (status, login) = call(
        &app,
        Method::POST,
        "/auth/login",
        None,
        Some(json!({ "email": "ANN@example.com", "password": "correct-horse-battery" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(login["user"]["email"], "ann@example.com");
    assert!(login["token"].as_str().is_some_and(|t| !t.is_empty()));
}

#[sqlx::test]
async fn duplicate_email_is_a_conflict_regardless_of_case(pool: PgPool) {
    let app = app(pool);
    register(&app, "dup@example.com").await;
    let (status, body) = call(
        &app,
        Method::POST,
        "/auth/register",
        None,
        Some(json!({ "email": "DUP@example.com", "password": "another-password" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "CONFLICT");
}

#[sqlx::test]
async fn invalid_registration_input_is_rejected(pool: PgPool) {
    let app = app(pool);
    let cases = [
        json!({ "email": "not-an-email", "password": "long-enough-pw" }),
        json!({ "email": "a@example.com", "password": "short" }),
        json!({ "email": "a@example.com", "password": "x".repeat(129) }),
    ];
    for payload in cases {
        let (status, body) = call(
            &app,
            Method::POST,
            "/auth/register",
            None,
            Some(payload.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{payload}");
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    }
}

#[sqlx::test]
async fn malformed_requests_use_the_json_error_shape(pool: PgPool) {
    let app = app(pool);
    // Missing field -> axum reports a data error; garbage body -> syntax error.
    let (status, body) = call(
        &app,
        Method::POST,
        "/auth/register",
        None,
        Some(json!({ "email": "a@b.co" })),
    )
    .await;
    assert!(status.is_client_error(), "{status}");
    assert!(
        body["error"]["code"].is_string(),
        "must be our JSON shape: {body}"
    );

    let request = Request::builder()
        .method(Method::POST)
        .uri("/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).expect("error body is JSON");
    assert_eq!(body["error"]["code"], "BAD_REQUEST");
}

#[sqlx::test]
async fn wrong_password_and_unknown_email_look_identical(pool: PgPool) {
    let app = app(pool);
    register(&app, "real@example.com").await;
    let wrong_password = call(
        &app,
        Method::POST,
        "/auth/login",
        None,
        Some(json!({ "email": "real@example.com", "password": "not-the-password" })),
    )
    .await;
    let unknown_email = call(
        &app,
        Method::POST,
        "/auth/login",
        None,
        Some(json!({ "email": "nobody@example.com", "password": "not-the-password" })),
    )
    .await;
    assert_eq!(wrong_password.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong_password, unknown_email,
        "no account enumeration through login"
    );
}

#[sqlx::test]
async fn protected_routes_reject_missing_and_invalid_tokens(pool: PgPool) {
    let app = app(pool);
    let user_id = Uuid::new_v4();
    let sign = |secret: &str, exp: u64| {
        let claims = Claims {
            sub: user_id,
            iat: 1,
            exp,
        };
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()).unwrap(),
        )
        .unwrap()
    };
    let far_future = 4_000_000_000;
    let tokens = [
        None,
        Some("garbage".to_string()),
        Some(sign("a-different-secret-a-different-secret", far_future)),
        Some(sign(SECRET, 100)), // expired long ago
    ];
    for uri in ["/me", "/notes"] {
        for token in &tokens {
            let (status, body) = call(&app, Method::GET, uri, token.as_deref(), None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} with {token:?}");
            assert_eq!(body["error"]["code"], "UNAUTHORIZED");
        }
    }
}

#[sqlx::test]
async fn a_valid_token_for_a_deleted_user_is_rejected(pool: PgPool) {
    let app = app(pool.clone());
    let token = register(&app, "gone@example.com").await;
    sqlx::query("DELETE FROM users")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = call(&app, Method::GET, "/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn notes_crud_flow(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "notes@example.com").await;

    let created = create_note(&app, &token, "  First note  ").await;
    assert_eq!(created["title"], "First note", "title is trimmed");
    let id = created["id"].as_str().unwrap();

    let (status, fetched) = call(
        &app,
        Method::GET,
        &format!("/notes/{id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!((status, &fetched["body"]), (StatusCode::OK, &json!("text")));

    let (status, patched) = call(
        &app,
        Method::PATCH,
        &format!("/notes/{id}"),
        Some(&token),
        Some(json!({ "title": "Renamed" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["title"], "Renamed");
    assert_eq!(patched["body"], "text", "fields not sent stay unchanged");

    let (status, _) = call(
        &app,
        Method::DELETE,
        &format!("/notes/{id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &app,
        Method::GET,
        &format!("/notes/{id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn listing_is_newest_first_and_paginated(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "pages@example.com").await;
    for title in ["one", "two", "three"] {
        create_note(&app, &token, title).await;
    }

    let titles = |body: &Value| -> Vec<String> {
        body.as_array()
            .unwrap()
            .iter()
            .map(|n| n["title"].as_str().unwrap().to_string())
            .collect()
    };
    let (_, all) = call(&app, Method::GET, "/notes", Some(&token), None).await;
    assert_eq!(titles(&all), ["three", "two", "one"]);
    let (_, page) = call(
        &app,
        Method::GET,
        "/notes?limit=2&offset=1",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(titles(&page), ["two", "one"]);

    for bad in ["/notes?limit=0", "/notes?limit=101", "/notes?offset=-1"] {
        let (status, _) = call(&app, Method::GET, bad, Some(&token), None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    let (status, _) = call(&app, Method::GET, "/notes?limit=abc", Some(&token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn users_cannot_see_or_change_each_others_notes(pool: PgPool) {
    let app = app(pool);
    let alice = register(&app, "alice@example.com").await;
    let bob = register(&app, "bob@example.com").await;
    let note = create_note(&app, &alice, "alice only").await;
    let uri = format!("/notes/{}", note["id"].as_str().unwrap());

    let (status, _) = call(&app, Method::GET, &uri, Some(&bob), None).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "404, not 403: do not reveal the id exists"
    );
    let (status, _) = call(
        &app,
        Method::PATCH,
        &uri,
        Some(&bob),
        Some(json!({ "title": "hacked" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, Method::DELETE, &uri, Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, bobs_list) = call(&app, Method::GET, "/notes", Some(&bob), None).await;
    assert_eq!(bobs_list, json!([]));
    let (status, still_there) = call(&app, Method::GET, &uri, Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        still_there["title"], "alice only",
        "alice's note is untouched"
    );
}

#[sqlx::test]
async fn note_input_is_validated(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "valid@example.com").await;
    let bad_creates = [
        json!({ "title": "   " }),
        json!({ "title": "x".repeat(201) }),
        json!({ "title": "ok", "body": "x".repeat(10_001) }),
    ];
    for payload in bad_creates {
        let (status, _) = call(
            &app,
            Method::POST,
            "/notes",
            Some(&token),
            Some(payload.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{payload}");
    }
    let note = create_note(&app, &token, "fine").await;
    let uri = format!("/notes/{}", note["id"].as_str().unwrap());
    let (status, _) = call(&app, Method::PATCH, &uri, Some(&token), Some(json!({}))).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "an empty patch is an error"
    );
    let (status, body) = call(&app, Method::GET, "/notes/not-a-uuid", Some(&token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "BAD_REQUEST");
}

#[sqlx::test]
async fn oversized_bodies_are_refused(pool: PgPool) {
    let app = app(pool);
    let token = register(&app, "big@example.com").await;
    let huge = json!({ "title": "x", "body": "y".repeat(100_000) });

    // Streamed body without a Content-Length header: rejected while reading, with our JSON error.
    let (status, body) = call(
        &app,
        Method::POST,
        "/notes",
        Some(&token),
        Some(huge.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "PAYLOAD_TOO_LARGE");

    // A normal client sends Content-Length: rejected up front, before reading the body.
    let payload = huge.to_string();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/notes")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_LENGTH, payload.len())
        .body(Body::from(payload))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
