# 멀티 스테이지: Go 바이너리만 경량 런타임에 복사합니다.
FROM golang:1.24-alpine AS builder
WORKDIR /src
COPY go.mod ./
COPY *.go ./
RUN CGO_ENABLED=0 go build -trimpath -ldflags='-s -w' -o /out/memonest-api .

# 비-root read-only 런타임. 운영 전 베이스 이미지 고정 digest/보안 스캔 필요.
FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=builder /out/memonest-api /memonest-api
ENV API_BIND_ADDRESS=0.0.0.0:8080
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/memonest-api"]
