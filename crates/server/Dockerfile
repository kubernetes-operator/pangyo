# syntax=docker/dockerfile:1
# === MemoNest 이미지: 웹 앱(apps/web) + API 서버. 빌드 컨텍스트 = 저장소 루트 (`make image`) ===
# 런타임은 비-root distroless. 운영 전 베이스 이미지 digest 고정과 보안 스캔 필요.
ARG RUST_VERSION=1.99
ARG NODE_VERSION=22

# 1) 웹 앱 빌드 → /src/apps/web/dist
FROM node:${NODE_VERSION}-bookworm-slim AS web
WORKDIR /src
COPY package.json package-lock.json ./
COPY apps/web/package.json apps/web/package.json
COPY apps/native/package.json apps/native/package.json
RUN npm ci -w apps/web --no-audit --no-fund
COPY apps/web apps/web
COPY packages/protocol packages/protocol
RUN npm run -w apps/web build

# 2) API 서버 빌드 (DB 마이그레이션 db/postgres 는 바이너리에 포함)
FROM rust:${RUST_VERSION}-slim-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY apps/bsd ./apps/bsd
COPY apps/native/src-tauri ./apps/native/src-tauri
COPY packages/protocol ./packages/protocol
COPY db ./db
RUN cargo build --release --locked -p memonest-server

# 3) 런타임: glibc 버전을 빌더(bookworm)와 맞춤
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /src/target/release/memonest-server /memonest-server
COPY --from=web /src/apps/web/dist /web
ENV API_BIND_ADDRESS=0.0.0.0:8080 \
    WEB_ROOT=/web
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/memonest-server"]
