//! Configuration, read once from environment variables at startup.

use std::env;

const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_ttl_secs: u64,
    pub port: u16,
}

impl Config {
    /// Read and validate the environment. Fails fast with a readable message.
    pub fn from_env() -> Result<Self, String> {
        let database_url = required("DATABASE_URL")?;
        let jwt_secret = required("JWT_SECRET")?;
        if jwt_secret.len() < MIN_SECRET_LEN {
            return Err(format!(
                "JWT_SECRET must be at least {MIN_SECRET_LEN} characters (try: openssl rand -hex 32)"
            ));
        }
        Ok(Self {
            database_url,
            jwt_secret,
            jwt_ttl_secs: parse_or("JWT_TTL_SECS", 3600)?,
            port: parse_or("PORT", 3000)?,
        })
    }
}

fn required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("missing required environment variable {name}"))
}

fn parse_or<T: std::str::FromStr>(name: &str, default: T) -> Result<T, String> {
    match env::var(name) {
        Ok(raw) => raw
            .parse()
            .map_err(|_| format!("{name} has an invalid value: {raw:?}")),
        Err(_) => Ok(default),
    }
}
