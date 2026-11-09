// === 계정 E2E (로드맵 05): 실제 Rust 서버 + Postgres, 서버가 웹 앱도 제공하는 배포 구조 그대로 ===
import { expect, test } from '@playwright/test';

const PASSWORD = 'correct horse battery';

test('가입 → 계정·용량 표시 → 장비 등록 코드 → 로그아웃 → 틀린 비밀번호 → 다시 로그인', async ({ page }) => {
  const email = `e2e-${Date.now()}-${Math.random().toString(36).slice(2, 8)}@example.com`;
  await page.goto('/');
  // 서버가 웹 앱을 제공하고, 메모 기능은 그대로 동작
  await expect(page.locator('#title')).toBeVisible();

  await page.locator('#account-btn').click();
  const dialog = page.locator('#account-dialog');
  await expect(dialog.locator('#account-login-form')).toBeVisible();
  await dialog.locator('#account-switch').click();
  const form = dialog.locator('#account-register-form');
  await form.locator('input[name=email]').fill(email);
  await form.locator('input[name=password]').fill(PASSWORD);
  await form.locator('input[name=displayName]').fill('테스터');
  await form.locator('button[type=submit]').click();

  await expect(dialog.locator('#account-email')).toContainText(email);
  await expect(dialog.locator('#account-quota')).toHaveText('0 B / 100 MB (0%)');
  await dialog.locator('#pairing-start').click();
  await expect(dialog.locator('#pairing-code')).toHaveText(/^[A-HJ-NP-Z2-9]{8} · /);

  // 새로고침해도 로그인 유지 (HttpOnly 쿠키)
  await page.reload();
  await page.locator('#account-btn').click();
  await expect(dialog.locator('#account-email')).toContainText(email);

  await dialog.locator('#account-logout').click();
  const login = dialog.locator('#account-login-form');
  await expect(login).toBeVisible();
  await login.locator('input[name=email]').fill(email);
  await login.locator('input[name=password]').fill('wrong password!!');
  await login.locator('button[type=submit]').click();
  await expect(login.locator('[role=alert]')).toContainText('올바르지 않습니다');

  await login.locator('input[name=password]').fill(PASSWORD);
  await login.locator('button[type=submit]').click();
  await expect(dialog.locator('#account-email')).toContainText(email);
});

test('JS에서 세션 쿠키를 읽을 수 없음 (HttpOnly)', async ({ page }) => {
  const email = `e2e-cookie-${Date.now()}@example.com`;
  await page.goto('/');
  await page.evaluate(
    async ([email, password]) => {
      await fetch('/api/v1/auth/register', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-MemoNest-Request': '1' },
        body: JSON.stringify({ email, password }),
      });
    },
    [email, PASSWORD],
  );
  expect(await page.evaluate(() => document.cookie)).not.toContain('memonest_session');
  const me = await page.evaluate(async () => (await fetch('/api/v1/account')).status);
  expect(me).toBe(200);
});

test('자동화 토큰(n8n): 만들기 → 원문 1회 표시 → 그 토큰으로 API 사용 → 해제', async ({ page }) => {
  const email = `e2e-token-${Date.now()}@example.com`;
  await page.goto('/');
  await page.evaluate(
    async ([email, password]) =>
      fetch('/api/v1/auth/register', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-MemoNest-Request': '1' },
        body: JSON.stringify({ email, password }),
      }),
    [email, PASSWORD],
  );
  await page.reload();
  await page.locator('#account-btn').click();
  const dialog = page.locator('#account-dialog');
  await dialog.locator('#token-name').fill('n8n 테스트');
  await dialog.locator('#token-create').click();
  const created = dialog.locator('#token-created');
  await expect(created).toHaveText(/^mn_pat_/);
  const token = (await created.textContent())!;
  await expect(dialog.locator('#token-list')).toContainText('n8n 테스트 (read');

  // n8n처럼 Bearer 토큰으로 호출 (쿠키 없이)
  const status = (t: string) =>
    page.evaluate(
      async (t) => (await fetch('/api/v1/notes?since=0', { credentials: 'omit', headers: { Authorization: `Bearer ${t}` } })).status,
      t,
    );
  expect(await status(token)).toBe(200);
  await dialog.locator('#token-list button').first().click();
  await expect(dialog.locator('#token-list')).toContainText('발급한 토큰이 없습니다');
  expect(await status(token)).toBe(401);
});
