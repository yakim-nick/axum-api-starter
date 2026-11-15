# ---- build ----
FROM rust:1-slim-bookworm AS build
WORKDIR /app
# Cache dependencies: build a dummy crate first so this layer only reruns when Cargo.* change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs \
    && cargo build --release --locked && rm -rf src target/release/deps/axum_api_starter* target/release/axum-api-starter*
COPY src ./src
COPY migrations ./migrations
# The dummy build above is newer than the copied sources, so cargo would reuse it and ship an
# empty main(). Touch the real files so they are rebuilt.
RUN touch src/main.rs src/lib.rs && cargo build --release --locked

# ---- run ----
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home app
COPY --from=build /app/target/release/axum-api-starter /usr/local/bin/axum-api-starter
USER app
ENV PORT=3000
EXPOSE 3000
HEALTHCHECK --interval=15s --timeout=5s --retries=3 CMD ["axum-api-starter", "--healthcheck"]
CMD ["axum-api-starter"]
