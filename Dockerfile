# ── Stage 1: Build ────────────────────────────────────────────────────
FROM rust:1.83-bookworm AS builder

WORKDIR /app

# Copy manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates/common/Cargo.toml   crates/common/Cargo.toml
COPY crates/indexer/Cargo.toml  crates/indexer/Cargo.toml
COPY crates/features/Cargo.toml crates/features/Cargo.toml
COPY crates/scorer/Cargo.toml   crates/scorer/Cargo.toml
COPY crates/alerts/Cargo.toml   crates/alerts/Cargo.toml
COPY crates/api/Cargo.toml      crates/api/Cargo.toml
COPY crates/storage/Cargo.toml  crates/storage/Cargo.toml
COPY crates/replay/Cargo.toml   crates/replay/Cargo.toml

# Create stub sources so cargo can fetch & cache deps
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/bin/backtest.rs && \
    for d in common indexer features scorer alerts api storage replay; do \
      mkdir -p "crates/$d/src" && echo "" > "crates/$d/src/lib.rs"; \
    done

RUN cargo build --release 2>/dev/null || true

# Now copy the real source
COPY src/ src/
COPY crates/ crates/

# Touch source files to invalidate the stub build
RUN find src crates -name "*.rs" -exec touch {} +

RUN cargo build --release --bin fraud-detector --bin backtest

# ── Stage 2: Runtime ──────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries
COPY --from=builder /app/target/release/fraud-detector .
COPY --from=builder /app/target/release/backtest .

# Copy assets
COPY config/   config/
COPY dashboard/ dashboard/
COPY data/     data/

# Create data directory for SQLite
RUN mkdir -p /app/data

ENV RUST_LOG=info

EXPOSE 3000

CMD ["./fraud-detector"]
