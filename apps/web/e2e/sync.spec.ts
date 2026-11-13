// === 동기화 E2E (로드맵 06, ADR-0003 A안): 같은 계정의 두 기기(브라우저 컨텍스트) + 실제 서버/Postgres ===
import { expect, test, type Browser, type Page } from '@playwright/test';

const PASSWORD = 'correct horse battery';

/** 새 브라우저 컨텍스트(= 다른 기기)에서 로그인한 페이지 */
async function deviceFor(browser: Browser, email: string, register: boolean): Promise<Page> {
  const context = await browser.newContext({ locale: 'ko-KR' });
  await context.addInitScript(() => localStorage.setItem('memonest.helpShown', '1')); // 도움말은 help.spec.ts
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
  await page.locator('#add-note').click();
  await page.locator('#name-input').fill(title);
  await page.locator('#name-input').press('Enter');
  await expect(page.locator('#title')).toHaveValue(title);
  await page.locator('#editor').click();
  await page.locator('#editor').pressSequentially(content);
}

async function openNote(page: Page, title: string) {
  const exact = new RegExp(`^· ${title.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\.(md|txt)$`);
  await page.locator('#tree button.note', { hasText: exact }).first().click();
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

test('연관 메모: 비슷한 메모를 서버에서 찾아 보여주고 누르면 이동', async ({ browser }) => {
  const email = `related-${Date.now()}@example.com`;
  const page = await deviceFor(browser, email, true);
  await createNote(page, '프로젝트 계획', '프로젝트 계획 회의 일정과 담당자 정리');
  await createNote(page, '프로젝트 계획 2차', '프로젝트 계획 회의 일정 변경과 담당자');
  await createNote(page, '장보기', '우유 계란 빵');
  await synced(page);
  await openNote(page, '프로젝트 계획');
  const panel = page.locator('#related-panel');
  await expect(panel).toBeVisible();
  await panel.locator('summary').click();
  await expect(page.locator('#related-list')).toContainText('프로젝트 계획 2차');
  await expect(page.locator('#related-list')).not.toContainText('장보기');
  await page.locator('#related-list button', { hasText: '프로젝트 계획 2차' }).click();
  await expect(page.locator('#title')).toHaveValue('프로젝트 계획 2차');
});

test('프로젝트(ADR-0005): 한 기기에서 만든 프로젝트와 그 메모가 다른 기기에 같은 프로젝트로', async ({ browser }) => {
  test.setTimeout(90_000);
  const email = `proj-${Date.now()}-${Math.random().toString(36).slice(2, 7)}@example.com`;
  const a = await deviceFor(browser, email, true);
  await a.locator('#add-project').click();
  await a.locator('#name-input').fill('여행');
  await a.locator('#name-input').press('Enter');
  await createNote(a, '일정', '제주 3일');
  await synced(a);

  const b = await deviceFor(browser, email, false);
  await expect(b.locator('#project-select option')).toHaveText(['기본', '여행'], { timeout: 15_000 });
  await expect(b.locator('#tree')).not.toContainText('일정');
  await b.locator('#project-select').selectOption({ label: '여행' });
  await openNote(b, '일정');
  await expect(b.locator('#editor')).toHaveValue('제주 3일');

  // A에서 이름을 바꾸면 B에도
  await a.locator('#rename-project').click();
  await a.locator('#name-input').fill('제주 여행');
  await a.locator('#name-input').press('Enter');
  await synced(a);
  await b.reload();
  await expect(b.locator('#project-select option')).toHaveText(['기본', '제주 여행'], { timeout: 30_000 });
});

test('사용 의견(2026-11-13 2차): 기본 프로젝트 이름 바꾸기와 메모 삭제가 다른 기기에도', async ({ browser }) => {
  test.setTimeout(90_000);
  const email = `del-${Date.now()}-${Math.random().toString(36).slice(2, 7)}@example.com`;
  const a = await deviceFor(browser, email, true);
  await createNote(a, '지울 메모', '곧 지움');
  await synced(a);
  // 기본 프로젝트 이름 바꾸기 (서버 계정 설정)
  await expect(a.locator('#rename-project')).toBeEnabled();
  await a.locator('#rename-project').click();
  await a.locator('#name-input').fill('일상');
  await a.locator('#name-input').press('Enter');
  await expect(a.locator('#project-select option:checked')).toHaveText('일상');

  const b = await deviceFor(browser, email, false);
  await expect(b.locator('#project-select option:checked')).toHaveText('일상', { timeout: 15_000 });
  await expect(b.locator('#tree')).toContainText('지울 메모');

  await openNote(a, '지울 메모');
  await a.locator('#delete-note').click();
  await a.locator('#confirm-ok').click();
  await expect(a.locator('#tree')).not.toContainText('지울 메모');
  await b.reload();
  await expect(b.locator('#tree')).not.toContainText('지울 메모', { timeout: 30_000 });
});

