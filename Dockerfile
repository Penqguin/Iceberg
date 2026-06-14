# Stage 1: Build the Rust application (WASM)
FROM rust:1.80-slim AS builder

WORKDIR /usr/src/app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev npm && rm -rf /var/lib/apt/lists/*
RUN npm install -g wrangler

# Add the WASM target
RUN rustup target add wasm32-unknown-unknown

# Copy cargo configuration files
COPY Cargo.toml Cargo.lock ./

# Pre-build dependencies (WASM library)
RUN mkdir src && touch src/lib.rs
RUN cargo build --release --target wasm32-unknown-unknown
RUN rm -f src/lib.rs

# Copy static files and source files
COPY static ./static
COPY src ./src

# Build the real WASM library
RUN cargo build --release --target wasm32-unknown-unknown

# Stage 2: Runtime image (Wrangler dev)
FROM node:20-slim

WORKDIR /app

# Install wrangler
RUN npm install -g wrangler

# Copy the compiled WASM from the builder stage
# wrangler-rs usually expects the WASM at a specific path or used via wrangler.toml
COPY --from=builder /usr/src/app/target/wasm32-unknown-unknown/release/iceberg.wasm ./index.wasm
COPY static ./static
COPY src ./src
COPY Cargo.toml ./

# In a real worker-rs project, wrangler.toml defines how to load the WASM.
# For a quick dockerized test, we'll need a wrangler.toml.

EXPOSE 8787

# This is a simplified Dockerfile. Running Workers in Docker usually means
# running 'wrangler dev --ip 0.0.0.0'.
CMD ["wrangler", "dev", "--ip", "0.0.0.0", "--port", "8787"]
