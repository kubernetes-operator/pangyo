// === Vite 설정: 웹 PWA와 Tauri가 같은 빌드 결과(dist/)를 사용 ===
import { defineConfig } from 'vitest/config';
import { offlineServiceWorker } from './build/offline-sw.ts';

const DEV_HOST = '127.0.0.1'; // 기본적으로 외부 공개 불가
const DEV_PORT = 5173; // 개발 서버 포트 (Tauri devUrl과 일치)
const PREVIEW_PORT = 4173; // 빌드 결과 미리보기 포트 (E2E 테스트가 사용)
const API_PORT = 8080; // 로컬 API 서버 포트 (scripts/dev-api.sh)

export default defineConfig({
  base: './', // 상대 경로: 하위 경로 배포와 Tauri 파일 로딩 모두 지원
  // 개발 중 계정 API는 로컬 서버(make dev-api)로 전달
  server: { host: DEV_HOST, port: DEV_PORT, strictPort: true, proxy: { '/api': `http://${DEV_HOST}:${API_PORT}` } },
  preview: { host: DEV_HOST, port: PREVIEW_PORT, strictPort: true },
  build: { target: 'es2022', sourcemap: true },
  plugins: [offlineServiceWorker()],
  test: { include: ['src/**/*.test.ts'], environment: 'node' },
});
