# ============================================================================
# Multi-Stage Dockerfile for LLM Schema Registry
# Optimized for production deployment with minimal image size
# ============================================================================

# ============================================================================
# Stage 1: Build Environment
# ============================================================================
FROM rust:1.82-bookworm AS builder

# Install system dependencies and protoc
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libprotobuf-dev \
    pkg-config \
    libssl-dev \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Verify protoc installation
RUN protoc --version

# Create app directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml rust-toolchain.toml ./
COPY crates/ ./crates/

# Copy proto files for gRPC compilation
COPY proto/ ./proto/

# Remove tests from workspace (not needed for build, excluded in .dockerignore)
RUN sed -i '/"tests",/d' Cargo.toml

# Fetch dependencies first (cached layer)
RUN cargo fetch

# Build only the schema-registry-server (main service binary)
RUN cargo build --release --package schema-registry-server && \
    cp target/release/schema-registry-server /tmp/schema-registry-server

# Verify binary was built
RUN ls -lh /tmp/schema-registry-server

# ============================================================================
# Stage 2: Runtime Image - Server
# ============================================================================
FROM debian:bookworm-slim AS runtime-server

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r schema-registry && \
    useradd -r -g schema-registry -u 1000 schema-registry && \
    mkdir -p /app && \
    chown -R schema-registry:schema-registry /app

# Set working directory
WORKDIR /app

# Copy server binary from builder
COPY --from=builder /tmp/schema-registry-server /app/schema-registry-server

# Switch to non-root user
USER schema-registry

# Expose port 8080 (Cloud Run standard port)
EXPOSE 8080

# Health check - uses liveness endpoint for fast startup
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/healthz || exit 1

# Set environment variables
ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    PORT=8080

# Run the server
ENTRYPOINT ["/app/schema-registry-server"]

# ============================================================================
# Stage 3: Runtime Image - CLI (placeholder for future CLI build)
# ============================================================================
# Note: CLI build temporarily disabled due to compatibility crate issues
# FROM debian:bookworm-slim AS runtime-cli

# ============================================================================
# Default target is runtime-server
# ============================================================================
FROM runtime-server
