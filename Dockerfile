# syntax=docker/dockerfile:1

# Stage 1: Builder
# Using latest stable Rust
FROM rust:bookworm AS builder

WORKDIR /build

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source
COPY src ./src

# Build for release (native architecture - arm64 on Apple Silicon, amd64 on Intel)
RUN cargo build --release

# Strip binary to reduce size
RUN strip target/release/fspulse || true

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    tini \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user (uid 1000, gid 1000)
RUN groupadd -r -g 1000 fspulse && useradd -r -u 1000 -g fspulse fspulse

# Create application directory
WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/fspulse /app/fspulse

# Copy entrypoint script
COPY docker/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

# Create data and roots directories
RUN mkdir -p /data /roots && chown -R fspulse:fspulse /data /roots

# Switch to non-root user
USER fspulse

# Set environment variables
ENV FSPULSE_DATA_DIR=/data \
    FSPULSE_SERVER_HOST=0.0.0.0 \
    FSPULSE_SERVER_PORT=8080

# Expose web UI port
EXPOSE 8080

# Health check (using HTTP endpoint)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1

# Volume mount points
VOLUME ["/data", "/roots"]

# Use tini as init system
ENTRYPOINT ["/usr/bin/tini", "--", "/app/entrypoint.sh"]

# Default command
CMD ["serve"]
