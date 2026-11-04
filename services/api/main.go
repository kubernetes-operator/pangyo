package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"time"
)

// === 0. 최상단 서버 환경 변수: 실제 인증/데이터 저장 서비스는 아직 구현하지 않음 ===
const defaultBindAddress = "127.0.0.1:8080" // 로컬 개발 기본 주소
const version = "0.1.0"                     // 클라이언트 협상에 사용할 서버 버전

func envOr(key, fallback string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return fallback
}
func jsonReply(w http.ResponseWriter, status int, data any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(data)
}
func mux() *http.ServeMux {
	m := http.NewServeMux()
	// 오케스트레이터 liveness와 readiness: 외부 저장소 사용 전이므로 생존 여부만 검사.
	m.HandleFunc("GET /healthz", func(w http.ResponseWriter, r *http.Request) { jsonReply(w, 200, map[string]string{"status": "ok"}) })
	m.HandleFunc("GET /readyz", func(w http.ResponseWriter, r *http.Request) {
		jsonReply(w, 200, map[string]string{"status": "ok", "note": "sync_not_ready"})
	})
	m.HandleFunc("GET /api/v1/info", func(w http.ResponseWriter, r *http.Request) {
		jsonReply(w, 200, map[string]any{"version": version, "authEnabled": false, "syncEnabled": false, "warning": "prototype only"})
	})
	// 구현되지 않은 쓰기 API는 데이터 손실/인증 우회를 막기 위해 거부합니다.
	m.HandleFunc("/api/v1/", func(w http.ResponseWriter, r *http.Request) {
		jsonReply(w, http.StatusNotImplemented, map[string]string{"error": "not_implemented", "message": "Authentication and sync are not available in v0.1"})
	})
	return m
}
func main() {
	addr := envOr("API_BIND_ADDRESS", defaultBindAddress)
	server := &http.Server{Addr: addr, Handler: mux(), ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 15 * time.Second, WriteTimeout: 15 * time.Second, IdleTimeout: 60 * time.Second}
	log.Printf("MemoNest API %s listening %s (no sync/auth endpoints implemented)", version, addr)
	log.Fatal(server.ListenAndServe())
}
