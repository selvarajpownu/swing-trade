# syntax=docker/dockerfile:1
# Build:  docker build -t swing-api services/api
# Run:    docker run --env-file .env -p 8080:8080 -v ./.data:/app/.data swing-api

# ---- builder ---------------------------------------------------------------
FROM rust:1-slim AS builder
WORKDIR /src

# reqwest uses rustls, so no OpenSSL headers are needed.
COPY . .

# BuildKit cache mounts keep the crate registry and target dir between builds,
# so only changed crates recompile. The binary is copied out of the cached dir.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release -p api-server \
 && cp target/release/swing-api /usr/local/bin/swing-api

# ---- runtime ---------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates for HTTPS to data providers; curl for the container healthcheck.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

RUN groupadd --system --gid 10001 app \
 && useradd --system --uid 10001 --gid app --home-dir /app --shell /usr/sbin/nologin app

WORKDIR /app
RUN mkdir -p /app/.data && chown -R app:app /app

COPY --from=builder /usr/local/bin/swing-api /usr/local/bin/swing-api

USER app
ENV BIND_ADDR=0.0.0.0:8080 \
    INDMONEY_TOKEN_FILE=/app/.data/indmoney-token.json \
    LOG_FORMAT=json \
    RUST_LOG=info

EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/health || exit 1

CMD ["swing-api", "serve"]
