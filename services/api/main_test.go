package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// API v0.1에서 동기화 및 미구현 경로를 차단하는지 회귀 검증합니다.
func TestHealth(t *testing.T) {
	r := httptest.NewRecorder()
	mux().ServeHTTP(r, httptest.NewRequest(http.MethodGet, "/healthz", nil))
	if r.Code != 200 {
		t.Fatalf("health got %d", r.Code)
	}
}
func TestSyncIsNotAvailable(t *testing.T) {
	r := httptest.NewRecorder()
	mux().ServeHTTP(r, httptest.NewRequest(http.MethodPost, "/api/v1/sync/commit", strings.NewReader("{}")))
	if r.Code != http.StatusNotImplemented {
		t.Fatalf("sync status got %d", r.Code)
	}
}
