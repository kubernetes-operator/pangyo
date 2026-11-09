// === E2E 설정 ===
// 1) chromium: 웹 빌드(vite preview)만으로 오프라인/저장/내보내기 회귀 (서버 없음)
// 2) server: Rust 서버가 웹 앱 + 계정 API를 함께 제공, 실제 Postgres 사용 (TEST_DATABASE_URL 있을 때만)
import { defineConfig, devices } from '@playwright/test';

const PREVIEW_URL = 'http://127.0.0.1:4173'; // vite.config.ts의 PREVIEW_PORT와 일치
const SERVER_PORT = 18090; // E2E 전용 서버 포트
const SERVER_URL = `http://127.0.0.1:${SERVER_PORT}`;
const SERVER_WEB_ROOT = '../../target/web-e2e'; // 서버용 웹 빌드 (preview의 dist와 분리)
const DATABASE_URL = process.env.TEST_DATABASE_URL; // 테스트 전용 DB (make db-test-up / CI 서비스)

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: { trace: 'retain-on-failure', locale: 'ko-KR' },
  projects: [
    {
      name: 'chromium',
      testIgnore: /(account|sync)\.spec\.ts/,
      use: { ...devices['Desktop Chrome'], baseURL: PREVIEW_URL },
    },
    ...(DATABASE_URL
      ? [
          {
            name: 'server',
            testMatch: /(account|sync)\.spec\.ts/,
            use: { ...devices['Desktop Chrome'], baseURL: SERVER_URL },
          },
        ]
      : []),
  ],
  webServer: [
    {
      command: 'npm run build && npm run preview',
      url: PREVIEW_URL,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
    ...(DATABASE_URL
      ? [
          {
            command: `npx vite build --outDir ${SERVER_WEB_ROOT} --emptyOutDir && cargo run -q --manifest-path ../../Cargo.toml -p memonest-server`,
            url: `${SERVER_URL}/readyz`,
            reuseExistingServer: false,
            timeout: 600_000, // 처음엔 Rust 빌드 시간 포함
            env: {
              API_BIND_ADDRESS: `127.0.0.1:${SERVER_PORT}`,
              DATABASE_URL,
              WEB_ROOT: SERVER_WEB_ROOT,
              COOKIE_SECURE: 'false', // http://127.0.0.1 테스트 전용
              LEASE_TTL_SECS: '5', // 임대 만료·갱신을 빠르게 확인
              DOWNLOADS_DIR: 'e2e/fixtures/downloads', // 설치 파일 다운로드 확인용 작은 가짜 파일
              RUST_LOG: 'warn',
            },
          },
        ]
      : []),
  ],
});
