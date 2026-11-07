# syntax=docker/dockerfile:1
# === MemoNest API 이미지 (빌드 컨텍스트 = 저장소 루트: `make image`) ===
# 런타임은 비-root distroless. 운영 전 베이스 이미지 digest 고정과 보안 스캔 필요.
ARG RUST_VERSION=1.99

FROM rust:${RUST_VERSION}-slim-bookworm AS builder
WORKDIR /src
# 워크스페이스 전체 매니페스트가 필요합니다 (members: crates/*, apps/bsd)
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY apps/bsd ./apps/bsd
COPY packages/protocol ./packages/protocol
RUN cargo build --release --locked -p memonest-server

# glibc 버전을 빌더(bookworm)와 맞춘 런타임
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /src/target/release/memonest-server /memonest-server
ENV API_BIND_ADDRESS=0.0.0.0:8080
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/memonest-server"]
