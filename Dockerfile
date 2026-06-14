# Stage 1: Build the Rust application (WASM)
FROM rustlang/rust:nightly-slim AS builder

WORKDIR /usr/src/app

# Ensure rustup is available for all commands
ENV PATH="/root/.cargo/bin:${PATH}"

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev curl gnupg && \
    curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*
RUN npm install -g wrangler

# Add the WASM target
RUN rustup target add wasm32-unknown-unknown

# Copy project files
COPY . .

# Build the project (assuming wrangler or cargo build works)
RUN cargo build --release --target wasm32-unknown-unknown

# Stage 2: Runtime image
FROM rustlang/rust:nightly-slim

WORKDIR /app

# Ensure rustup is available for all commands
ENV PATH="/root/.cargo/bin:${PATH}"

# Install npm and wrangler
RUN apt-get update && apt-get install -y curl gnupg && \
    curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*
RUN npm install -g wrangler

# Add the WASM target so any on-start builds succeed
RUN rustup target add wasm32-unknown-unknown || true

# Copy only the built artifacts and static assets — avoid copying source
# to prevent `wrangler dev` from attempting a rebuild inside the runtime image.
COPY --from=builder /usr/src/app/target/wasm32-unknown-unknown/release/iceberg.wasm ./
COPY --from=builder /usr/src/app/wrangler.toml ./wrangler.toml
COPY --from=builder /usr/src/app/build ./build
COPY --from=builder /usr/src/app/static ./static

# Replace the wrangler config at runtime to avoid invoking the Rust build
# inside the runtime container. The builder already produced `iceberg.wasm` and
# `build/index.js` which will be served.
RUN printf 'name = "iceberg"\nmain = "build/index.js"\ncompatibility_date = "2024-04-05"\n\n[vars]\nWHITELIST = "penqguin"\n' > wrangler.toml

# Expose port 8080 to match common run commands (docker run -p 8080:8080)
EXPOSE 8080

# Use 'wrangler dev' on port 8080
CMD ["wrangler", "dev", "--ip", "0.0.0.0", "--port", "8080"]
