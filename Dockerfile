# Frontend build stage (only needed when admin panel is used)
FROM node:22-slim AS frontend
WORKDIR /app/rinfra-admin/frontend
COPY rinfra-admin/frontend/package.json rinfra-admin/frontend/package-lock.json ./
RUN npm ci
COPY rinfra-admin/frontend ./
RUN npm run build

# Rust build stage
FROM rust:1.88-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
ARG BINARY=web
COPY Cargo.toml Cargo.lock ./
COPY rinfra-core ./rinfra-core
COPY rinfra-derive ./rinfra-derive
COPY config ./config
COPY rinfra-plugins ./rinfra-plugins
COPY rinfra-examples ./rinfra-examples
COPY rinfra-admin ./rinfra-admin
RUN cargo build --release --bin ${BINARY}

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
ARG BINARY=web
COPY --from=builder /app/target/release/${BINARY} /app/rinfra
COPY --from=frontend /app/rinfra-admin/frontend/dist /app/admin-ui

CMD ["/app/rinfra", "--config", "config/config.yaml"]
