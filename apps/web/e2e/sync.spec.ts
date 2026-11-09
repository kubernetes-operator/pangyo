// === 동기화 E2E (로드맵 06, ADR-0003 A안): 같은 계정의 두 기기(브라우저 컨텍스트) + 실제 서버/Postgres ===
import { expect, test, type Browser, type Page } from '@playwright/test';

const PASSWORD = 'correct horse battery';

/** 새 브라우저 컨텍스트(= 다른 기기)에서 로그인한 페이지 */
async function deviceFor(browser: Browser, email: string, register: boolean): Promise<Page> {
  const context = await browser.newContext({ locale: 'ko-KR' });
  const page = await context.newPage();
  await page.goto('/');
  const status = await page.evaluate(
    async ([email, password, path]) =>
      (
        await fetch(`/api/v1/auth/${path}`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', 'X-MemoNest-Request': '1' },
          body: JSON.stringify({ email, password }),
        })
      ).status,
    [email, PASSWORD, register ? 'register' : 'login'],
  );
  expect(status).toBeLessThan(300);
  await page.reload(); // 로그인 상태로 시작 → 자동 동기화
  return page;
}

async function createNote(page: Page, title: string, content: string) {
  page.once('dialog', (d) => void d.accept(title));
  await page.locator('#add-note').click();
  await expect(page.locator('#title')).toHaveValue(title);
  await page.locator('#editor').click();
  await page.locator('#editor').pressSequentially(content);
}

async function openNote(page: Page, title: string) {
  await page.locator('#tree button.note', { hasText: title }).first().click();
  await expect(page.locator('#title')).toHaveValue(title);
}

const synced = (page: Page) => expect(page.locator('#sync-status')).toHaveText('동기화됨', { timeout: 15_000 });

test('다른 기기에 전달, 편집 중엔 읽기 전용, 오프라인 초안 충돌은 둘 다 보관', async ({ browser }) => {
  test.setTimeout(120_000);
  const email = `sync-${Date.now()}-${Math.random().toString(36).slice(2, 7)}@example.com`;
  const a = await deviceFor(browser, email, true);
  await createNote(a, '공유', '노트북에서 씀');
  await synced(a);

  // 1) 다른 기기에서 로그인하면 메모가 내려옴
  const b = await deviceFor(browser, email, false);
  await expect(b.locator('#tree')).toContainText('공유.md');
  await openNote(b, '공유');
  await expect(b.locator('#editor')).toHaveValue('노트북에서 씀');

  // 2) A가 편집 중(임대 보유) → B는 읽기 전용 + 누가 편집 중인지 표시
  await a.locator('#editor').click();
  await a.locator('#editor').pressSequentially('!');
  await synced(a);
  await b.locator('#editor').click();
  const banner = b.locator('#sync-banner');
  await expect(banner).toHaveAttribute('data-kind', 'locked');
  await expect(banner).toContainText('편집 중이라 읽기 전용');
  await expect(b.locator('#editor')).toHaveJSProperty('readOnly', true);
  await expect(b.locator('#editor')).toHaveValue('노트북에서 씀!'); // 최신 내용으로 보임

  // A가 다른 메모로 이동 → 임대 반납 → B가 다시 시도하면 편집 가능
  await createNote(a, '다른것', 'x');
  await b.locator('#lease-retry').click();
  await expect(banner).toBeHidden();
  await expect(b.locator('#editor')).toHaveJSProperty('readOnly', false);
  await b.locator('#editor').press('End');
  await b.locator('#editor').pressSequentially(' 휴대폰');
  await synced(b);
  await openNote(a, '공유'); // B가 아직 임대 보유 → A 읽기 전용, 대신 B 내용이 보임
  await a.locator('#editor').click();
  await expect(a.locator('#sync-banner')).toHaveAttribute('data-kind', 'locked');
  await openNote(b, '다른것'); // B 반납

  // 3) B 오프라인 초안 vs A 온라인 수정 → 충돌, 어느 쪽도 덮어쓰지 않음
  await a.locator('#lease-retry').click();
  await expect(a.locator('#sync-banner')).toBeHidden();
  await openNote(b, '공유');
  await b.context().setOffline(true);
  await b.locator('#editor').click();
  await b.locator('#editor').press('End');
  await b.locator('#editor').pressSequentially(' [오프라인 초안]');
  await expect(b.locator('#sync-status')).toContainText('오프라인', { timeout: 15_000 });

  await a.locator('#editor').press('End');
  await a.locator('#editor').pressSequentially(' [노트북 수정]');
  await synced(a);
  await openNote(a, '다른것'); // A 반납

  await b.context().setOffline(false); // 다시 연결 → 자동 동기화
  await expect(b.locator('#sync-banner')).toHaveAttribute('data-kind', 'conflict', { timeout: 15_000 });
  await expect(b.locator('#editor')).toHaveValue(/\[오프라인 초안\]$/); // 로컬 초안 보존
  await expect(b.locator('#conflict-server-content')).toContainText('[노트북 수정]'); // 서버 버전도 보존
  await b.locator('#conflict-both').click();
  await expect(b.locator('#editor')).toHaveValue(/\[노트북 수정\]$/);
  await expect(b.locator('#tree')).toContainText('공유 (내 사본).md');
  await synced(b);

  // A에도 사본이 전달됨
  await a.reload();
  await expect(a.locator('#tree')).toContainText('공유 (내 사본).md');
  await openNote(a, '공유 (내 사본)');
  await expect(a.locator('#editor')).toHaveValue(/\[오프라인 초안\]$/);
});
