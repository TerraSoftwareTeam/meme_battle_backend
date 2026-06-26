FROM lukemathwalker/cargo-chef:latest-rust-1.86-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl-dev \
        pkg-config \
        && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
COPY overwrite /app/overwrite

ENV SWAGGER_UI_OVERWRITE_FOLDER=/app/overwrite

RUN cargo chef cook --release --locked --recipe-path recipe.json

COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        libssl3 \
        && rm -rf /var/lib/apt/lists/* \
    && groupadd --system app \
    && useradd --system --gid app --home-dir /app app

COPY --from=builder /app/target/release/meme_battle_backend /app/meme_battle_backend

ENV RUST_LOG=info \
    SERVICE_HOST=0.0.0.0 \
    SERVICE_PORT=8080

EXPOSE 8080

USER app

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=5 \
    CMD curl -fsS "http://127.0.0.1:${SERVICE_PORT}/health" || exit 1

ENTRYPOINT ["/app/meme_battle_backend"]
