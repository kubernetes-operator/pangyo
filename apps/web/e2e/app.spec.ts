// === 회귀 테스트: docs/12_TESTPLAN.md T-01 ~ T-06 + v0.1 IndexedDB 데이터 호환 ===
import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';

const HISTORY_LIMIT = 250;

/**
 * 대화상자(prompt/alert)를 순서대로 처리합니다. answer(값)을 호출한 순서대로 다음 대화상자에 응답하고,
 * 그 대화상자의 메시지를 돌려줍니다. 핸들러가 여러 개 동시에 반응하지 않도록 큐 하나로 관리합니다.
 */
const dialogQueues = new WeakMap<Page, Array<{ value?: string; resolve: (m: string) => void }>>();
function answerNextDialog(page: Page, value?: string): Promise<string> {
  let queue = dialogQueues.get(page);
  if (!queue) {
    const q: Array<{ value?: string; resolve: (m: string) => void }> = [];
    dialogQueues.set(page, q);
    page.on('dialog', async (dialog) => {
      const next = q.shift();
      if (!next) return dialog.dismiss();
      next.resolve(dialog.message());
      await dialog.accept(next.value);
    });
    queue = q;
  }
  return new Promise((resolve) => queue.push({ value, resolve }));
}

async function createNote(page: Page, title: string): Promise<void> {
  const dialog = answerNextDialog(page, title);
  await page.locator('#add-note').click();
  await dialog;
  await expect(page.locator('#title')).toHaveValue(title);
}

async function waitSaved(page: Page): Promise<void> {
  await expect(page.locator('#save-status')).toHaveText('LOCAL SAVED');
}

test('T-01 새로고침 후에도 메모 보존 (IndexedDB)', async ({ page }) => {
  await page.goto('/');
  await createNote(page, '보존 테스트');
  await page.locator('#editor').fill('# 제목\n긴 본문 줄');
  await waitSaved(page);
  await page.reload();
  await expect(page.locator('#title')).toHaveValue('보존 테스트');
  await expect(page.locator('#editor')).toHaveValue('# 제목\n긴 본문 줄');
});

test('T-01 오프라인에서 앱 로드와 편집 (서비스워커)', async ({ page, context }) => {
  await page.goto('/');
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready;
    if (!navigator.serviceWorker.controller) {
      await new Promise((r) => navigator.serviceWorker.addEventListener('controllerchange', r, { once: true }));
    }
  });
  await context.setOffline(true);
  await page.reload();
  await expect(page.locator('#connectivity')).toHaveText('LOCAL · OFFLINE');
  await createNote(page, '오프라인');
  await page.locator('#editor').fill('네트워크 없이 작성');
  await waitSaved(page);
  await page.reload();
  await expect(page.locator('#editor')).toHaveValue('네트워크 없이 작성');
});

test('T-02 폴더는 2단계까지만 허용', async ({ page }) => {
  await page.goto('/');
  let dialog = answerNextDialog(page, 'Work/Project');
  await page.locator('#add-folder').click();
  await dialog;
  await expect(page.locator('#folder-path')).toHaveText('/Work/Project');
  await expect(page.locator('#tree')).toContainText('▸ Project');

  dialog = answerNextDialog(page, 'Work/Project/Deep');
  const alert = answerNextDialog(page);
  await page.locator('#add-folder').click();
  await dialog;
  expect(await alert).toContain('최대 2단계');
  await expect(page.locator('#tree')).not.toContainText('Deep');
});

test('T-03 MD/TXT 다운로드 내용이 UTF-8로 동일', async ({ page }) => {
  await page.goto('/');
  await createNote(page, '내보내기');
  const body = '# 한글 제목\n- 목록 ✓\n\n  들여쓴 줄';
  await page.locator('#editor').fill(body);

  for (const format of ['md', 'txt'] as const) {
    await page.locator('#format').selectOption(format);
    const download = page.waitForEvent('download');
    await page.locator('#export-btn').click();
    const file = await download;
    expect(file.suggestedFilename()).toBe(`내보내기.${format}`);
    expect(await readFile((await file.path())!, 'utf8')).toBe(body);
  }
});

test('T-04 Undo 250회 기록이 새로고침 후에도 유지', async ({ page }) => {
  await page.goto('/');
  await createNote(page, 'undo');
  // 260번 편집: 한도(250)를 넘긴 앞쪽 10개 기록은 버려져야 함
  await page.evaluate(() => {
    const box = document.getElementById('editor') as HTMLTextAreaElement;
    for (let i = 1; i <= 260; i++) {
      box.value = `v${i}`;
      box.dispatchEvent(new Event('input', { bubbles: true }));
    }
  });
  await waitSaved(page);
  await page.reload();
  await expect(page.locator('#editor')).toHaveValue('v260');
  await page.evaluate((times) => {
    for (let i = 0; i < times; i++) document.getElementById('undo-btn')!.click();
  }, HISTORY_LIMIT + 5);
  await expect(page.locator('#editor')).toHaveValue('v10');
  await page.locator('#redo-btn').click();
  await expect(page.locator('#editor')).toHaveValue('v11');
});

/** 창 최소화/탭 전환을 흉내 냅니다 (headless 브라우저는 실제로 숨길 수 없음). */
async function setVisibility(page: Page, state: 'visible' | 'hidden'): Promise<void> {
  await page.evaluate((s) => {
    Object.defineProperty(document, 'visibilityState', { value: s, configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
  }, state);
}

test('T-05 작성 시간은 키 입력 간격만, 열어둔 시간은 창이 보이는 동안 모두 집계', async ({ page }) => {
  await page.clock.install();
  await page.goto('/');
  await createNote(page, 'session');
  const editor = page.locator('#editor');
  await editor.pressSequentially('a');
  await page.clock.runFor(10_000);
  await editor.pressSequentially('b'); // 10초 간격 → 작성 시간 +10초
  await page.clock.runFor(120_000); // 2분 무입력: 작성 시간 제외, 열어둔 시간은 포함
  await editor.pressSequentially('c'); // 한도(60초) 초과 간격 → +0
  await expect(page.locator('#timer-active')).toHaveText('00:00:10');
  await expect(page.locator('#timer-open')).toHaveText(/^00:02:1\d$/);
  await expect(page.locator('#sessions p')).toHaveCount(1); // 무입력만으로는 세션이 끊기지 않음
});

test('T-05b 창을 숨겼다 다시 열면 새 세션(재개), 숨긴 동안은 집계 안 함', async ({ page }) => {
  await page.clock.install();
  await page.goto('/');
  await createNote(page, 'resume');
  await page.clock.runFor(30_000);
  await setVisibility(page, 'hidden');
  await page.clock.runFor(600_000); // 10분 최소화
  await setVisibility(page, 'visible');
  await page.clock.runFor(5_000);
  await expect(page.locator('#sessions p')).toHaveCount(2);
  await expect(page.locator('#sessions p').nth(1)).not.toContainText(' - ...'); // 첫 세션 종료됨
  await expect(page.locator('#sessions p').nth(0)).toContainText(' - ...'); // 재개 세션 진행 중
  await expect(page.locator('#timer-open')).toHaveText(/^00:00:3[4-6]$/); // 30초 + 5초, 숨긴 10분 제외
});

test('T-06 F6과 버튼으로 관리/작성 화면 전환', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('body')).toHaveClass('mode-split');
  await page.keyboard.press('F6');
  await expect(page.locator('body')).toHaveClass('mode-editor');
  await expect(page.locator('#organizer')).toBeHidden();
  await page.keyboard.press('F6');
  await expect(page.locator('body')).toHaveClass('mode-organizer');
  await expect(page.locator('#writer')).toBeHidden();
  await page.locator('#split-btn').click();
  await expect(page.locator('body')).toHaveClass('mode-split');
  await page.locator('#toggle-btn').click();
  await expect(page.locator('body')).toHaveClass('mode-editor');
});

test('호환: v0.1 JS 프로토타입이 저장한 IndexedDB 데이터를 그대로 읽음', async ({ page }) => {
  // 앱 스크립트가 없는 같은 출처 페이지에서 v0.1 store.js와 같은 방식으로 DB를 만든다.
  await page.goto('/manifest.webmanifest');
  await page.evaluate(async () => {
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const req = indexedDB.open('memonest_local_v1', 1);
      req.onupgradeneeded = () => {
        req.result.createObjectStore('notes', { keyPath: 'id' });
        req.result.createObjectStore('folders', { keyPath: 'path' });
        req.result.createObjectStore('meta', { keyPath: 'key' });
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    const tx = db.transaction(['notes', 'folders', 'meta'], 'readwrite');
    tx.objectStore('folders').put({ path: 'Work' });
    // v0.1에서 undo/redo가 아직 생기지 않은 레코드 형태
    tx.objectStore('notes').put({
      id: '11111111-1111-4111-8111-111111111111',
      title: '옛 메모',
      folder: 'Work',
      format: 'txt',
      content: '이전 버전 내용',
      createdAt: '2026-10-29T00:00:00.000Z',
      modifiedAt: '2026-10-29T00:00:00.000Z',
      sessions: [{ id: 's1', startedAt: '2026-10-29T00:00:00.000Z', endedAt: '2026-10-29T00:01:00.000Z', activeMs: 60_000 }],
    });
    tx.objectStore('meta').put({ key: 'locale', value: 'en' });
    await new Promise((r) => (tx.oncomplete = r));
    db.close();
  });

  await page.goto('/');
  await expect(page.locator('#title')).toHaveValue('옛 메모');
  await expect(page.locator('#editor')).toHaveValue('이전 버전 내용');
  await expect(page.locator('#format')).toHaveValue('txt');
  await expect(page.locator('#timer-active')).toHaveText('00:01:00'); // v0.1 activeMs 유지
  await expect(page.locator('#tree')).toContainText('▸ Work');
  await expect(page.locator('#add-note')).toHaveText('+ Note'); // 저장된 언어 설정(en)
  // undo 기록이 없던 레코드도 편집/Undo가 정상 동작
  await page.locator('#editor').fill('수정');
  await page.locator('#undo-btn').click();
  await expect(page.locator('#editor')).toHaveValue('이전 버전 내용');
});
