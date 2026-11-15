use std::net::SocketAddr;
use std::sync::Arc;

use axum_api_starter::config::Config;
use axum_api_starter::{AppState, router};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // `axum-api-starter --healthcheck`: used by the Docker HEALTHCHECK (the slim image has no curl).
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        std::process::exit(healthcheck());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("configuration error: {message}");
            std::process::exit(2);
        }
    };

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .expect("could not connect to the database");
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("database migrations failed");

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = router(AppState {
        pool,
        config: Arc::new(config),
    });
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("could not bind the port");
    tracing::info!(%addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

/// Resolve on Ctrl+C or SIGTERM so in-flight requests can finish.
async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("ctrl-c handler") };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("shutting down");
}

/// Exit code 0 when `GET /health` on localhost answers 200, 1 otherwise. Plain std, no extra dependencies.
fn healthcheck() -> i32 {
    use std::io::{Read, Write};
    use std::time::Duration;

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    let Ok(mut stream) = std::net::TcpStream::connect(format!("127.0.0.1:{port}")) else {
        return 1;
    };
    let timeout = Some(Duration::from_secs(3));
    let _ = stream.set_read_timeout(timeout);
    let request = "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let mut response = String::new();
    if stream.write_all(request.as_bytes()).is_err()
        || stream.read_to_string(&mut response).is_err()
    {
        return 1;
    }
    i32::from(!response.starts_with("HTTP/1.1 200"))
}
