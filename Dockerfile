# ---- build stage ----
FROM rust:1.85-slim AS builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev build-essential \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk

WORKDIR /app
COPY . .

# Build backend binary
RUN cargo build -p backend --release

# Build frontend WASM bundle
RUN cd frontend && trunk build --release

# ---- final image ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/backend /app/backend
COPY --from=builder /app/frontend/dist /app/frontend/dist

ENV STATIC_DIR=/app/frontend/dist
ENV DB_PATH=/data/reports.db

EXPOSE 3000
CMD ["/app/backend"]
