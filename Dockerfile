# syntax=docker/dockerfile:1

# ── Builder stage ──────────────────────────────────────────────────────────
FROM rust:1.84-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        curl \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Rust wasm32 target
RUN rustup target add wasm32-unknown-unknown

# Install cargo-leptos (pin to a version known to work with leptos 0.8)
RUN cargo install cargo-leptos --version 0.3.6 --locked

# Install Tailwind CSS v4 standalone binary (x86_64 Linux glibc)
RUN curl -sLO https://github.com/tailwindlabs/tailwindcss/releases/download/v4.0.8/tailwindcss-linux-x64 \
    && chmod +x tailwindcss-linux-x64 \
    && mv tailwindcss-linux-x64 /usr/local/bin/tailwindcss \
    && tailwindcss --version

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY apps/monochange_app/ apps/monochange_app/

# Build the Leptos SSR application
WORKDIR /app/apps/monochange_app
RUN cargo leptos build --release

# ── Final stage ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 app

# Copy static assets and binary from builder
COPY --from=builder /app/apps/monochange_app/target/site /app/site
COPY --from=builder /app/apps/monochange_app/target/release/monochange_app /app/monochange_app

# Ensure correct permissions
RUN chown -R app:app /app

USER app
WORKDIR /app

ENV LEPTOS_SITE_ROOT=/app/site
ENV LEPTOS_SITE_PKG_DIR=pkg
ENV RUST_LOG=info
ENV PORT=3000

EXPOSE 3000

CMD ["./monochange_app"]
