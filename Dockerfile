# --- Stage 1: generate dependency recipe ---
FROM lukemathwalker/cargo-chef:latest-rust-1.94-bookworm AS chef
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- Stage 2: build dependencies + binaries ---
FROM lukemathwalker/cargo-chef:latest-rust-1.94-bookworm AS builder
WORKDIR /app
COPY --from=chef /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release -p kojin-examples

# --- Stage 3: minimal runtime ---
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/kojin-worker /usr/local/bin/
COPY --from=builder /app/target/release/kojin-producer /usr/local/bin/
COPY --from=builder /app/target/release/kojin-worker-amqp /usr/local/bin/
COPY --from=builder /app/target/release/kojin-producer-amqp /usr/local/bin/
COPY --from=builder /app/target/release/kojin-worker-agent /usr/local/bin/
COPY --from=builder /app/target/release/kojin-producer-agent /usr/local/bin/
ENTRYPOINT ["kojin-worker"]
