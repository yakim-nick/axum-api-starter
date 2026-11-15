# axum-api-starter

[![CI](https://github.com/yakim-nick/axum-api-starter/actions/workflows/ci.yml/badge.svg)](https://github.com/yakim-nick/axum-api-starter/actions/workflows/ci.yml)

A small REST API in Rust to learn and reuse: **Axum**, **SQLx + PostgreSQL**,
**JWT** authentication, **Argon2id** password hashing, and a per-user *notes*
resource that shows how ownership checks work. It is a starter template and a
learning project, not a production service (see [Known limitations](#known-limitations)).

## API

| Method | Path | Auth | What it does |
|--------|------|------|--------------|
| GET | `/health` | no | `200 {"status":"ok"}`, or `503` when the database is unreachable |
| POST | `/auth/register` | no | `{email, password}` -> `201 {token, user}`; `409` if the email exists |
| POST | `/auth/login` | no | `{email, password}` -> `200 {token, user}`; `401` on any failure |
| GET | `/me` | Bearer | the current user |
| POST | `/notes` | Bearer | `{title, body?}` -> `201` note |
| GET | `/notes?limit=&offset=` | Bearer | your notes, newest first (limit 1-100, default 20) |
| GET / PATCH / DELETE | `/notes/{id}` | Bearer | one note; `404` if it is not yours |

All errors share one shape: `{"error": {"code": "VALIDATION_ERROR", "message": "..."}}`.
Codes: `VALIDATION_ERROR` (422), `BAD_REQUEST` (400), `UNAUTHORIZED` (401),
`NOT_FOUND` (404), `CONFLICT` (409), `PAYLOAD_TOO_LARGE` (413), `INTERNAL_ERROR` (500,
details are logged, never returned).

## Run it

Needs Rust 1.94+ (required by SQLx 0.9) and Docker (for PostgreSQL).

```bash
docker run -d --name pg -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=app -p 5432:5432 postgres:16-alpine
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/app
export JWT_SECRET=$(openssl rand -hex 32)
cargo run                      # migrations run automatically at startup

curl -s -X POST localhost:3000/auth/register -H 'content-type: application/json' \
  -d '{"email":"me@example.com","password":"correct-horse-battery"}'
```

Or everything in containers: `export JWT_SECRET=$(openssl rand -hex 32) && docker compose up --build`.

Configuration (environment variables): `DATABASE_URL` and `JWT_SECRET` are
required (the secret must be at least 32 characters, otherwise the server exits
with a message); optional `PORT` (3000), `JWT_TTL_SECS` (3600), `RUST_LOG` (info).

## Layout and design decisions

```
src/main.rs         startup, graceful shutdown (Ctrl+C / SIGTERM), --healthcheck mode
src/routes/         HTTP handlers: parse input, call validation + repo, shape the response
src/validation.rs   every rule for user input, with unit tests
src/repo.rs         ALL SQL, as plain parameterised queries (no string-built SQL)
src/auth.rs         Argon2id hashing, JWT create/verify, the `AuthUser` extractor
src/extract.rs      wrappers so malformed JSON / bad path ids use the JSON error shape
src/error.rs        one `AppError`; `IntoResponse` maps it to status + JSON
migrations/         SQL schema, applied with `sqlx::migrate!`
tests/api.rs        end-to-end tests against a real PostgreSQL
```

- **Layers are routes -> validation -> repo.** There is no separate "service" layer:
  with this little business logic it would only forward calls. If rules grow, add one.
- **Ownership is enforced in SQL.** Note queries always include `AND user_id = $n`, so
  someone else's note is simply not found: `404`, not `403`, which does not reveal that
  the id exists.
- **Login does not reveal which emails exist.** Same status and body for a wrong
  password and an unknown email, and a dummy hash is verified for unknown emails so
  the response time is similar.
- **JWT is pinned to HS256** with a required `exp`; tokens signed with another secret,
  another algorithm or already expired are rejected. A token for a deleted user is
  rejected by `/me`.
- **Passwords:** Argon2id (library defaults), hashed on a blocking thread so the async
  runtime is not stalled.
- **Dynamic SQL is not used.** SQLx 0.9 refuses non-literal SQL strings anyway; queries
  are runtime-checked (`query_as`), not compile-time checked, so no database is needed to build.

## Tests

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/app   # a user that can create databases
cargo test                      # 12 unit tests + 13 end-to-end tests
cargo fmt --check && cargo clippy --all-targets -- -D warnings
```

`sqlx::test` creates a fresh, migrated database for every test, so tests are isolated
and run in parallel. CI runs formatting, clippy (warnings are errors), the tests
against a PostgreSQL service, and a Docker build.

The container was also checked by hand: it starts healthy, runs as a non-root user
(uid 10001), serves requests, returns `503` on `/health` when the database is stopped,
and shuts down on SIGTERM. The image is about 84 MB.

## Known limitations

- **No rate limiting or login throttling.** Argon2 slows each guess, but a real
  deployment needs limits (for example a reverse proxy, or `tower_governor`).
- **No refresh tokens, logout or token revocation**; a token is valid until it expires.
- **No email verification, password reset or roles.**
- **No CORS configuration**, so browser apps on another origin cannot call it yet.
- **No OpenAPI document**; the table above is the documentation.
- **Not benchmarked.** Nothing here claims performance numbers.
- Queries are not checked against the schema at compile time (see above).
- The Docker build compiles inside the image, so it is slow the first time.

## Roadmap

- Rate limiting, CORS, OpenAPI (`utoipa`).
- Refresh tokens and logout.
- Compare with a TypeScript implementation of the same API.

## License

MIT
