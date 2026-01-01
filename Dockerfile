# Stage 1: Planner - Generate dependency recipe
FROM rust:1-slim-bookworm AS planner
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Builder - Build dependencies and application
FROM rust:1-slim-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

# Build dependencies (cached layer)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN cargo build --release --bin entsoe-price-fetcher

# Stage 3: Runtime - Debian slim for glibc compatibility
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Install CA certificates and SSL libraries for HTTPS
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/entsoe-price-fetcher /app/entsoe-price-fetcher

# Copy configuration files
COPY --from=builder /app/config /app/config

# Use existing nobody user (UID 65534)
USER nobody

EXPOSE 8080

ENTRYPOINT ["/app/entsoe-price-fetcher"]
