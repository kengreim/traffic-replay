FROM lukemathwalker/cargo-chef:latest-rust-1.91 AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.toml
COPY src/ src/
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.toml
COPY src/ src/
COPY migrations/ migrations/
RUN cargo build --release --bin fetcher

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/fetcher /usr/local/bin/fetcher
CMD ["fetcher"]
