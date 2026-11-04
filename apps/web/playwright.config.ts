// === E2E 설정: 실제 빌드 결과(vite preview)를 대상으로 오프라인/저장/내보내기 회귀를 검사 ===
import { defineConfig, devices } from '@playwright/test';

const PREVIEW_URL = 'http://127.0.0.1:4173'; // vite.config.ts의 PREVIEW_PORT와 일치

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: { baseURL: PREVIEW_URL, trace: 'retain-on-failure', locale: 'ko-KR' },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: 'npm run build && npm run preview',
    url: PREVIEW_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
