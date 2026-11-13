// === 회귀 테스트: docs/12_TESTPLAN.md T-01 ~ T-06 + v0.1 IndexedDB 데이터 호환 ===
import { readFile } from 'node:fs/promises';
import { expect, test, type Page } from '@playwright/test';

const HISTORY_LIMIT = 250;

/** 앱 이름 입력 창(+ 메모, + 폴더)에 값을 넣고 Enter로 확인 (2026-11-13: prompt() 대신 앱 자체 창) */
async function enterName(page: Page, button: '#add-note' | '#add-folder', value: string): Promise<void> {
  await page.locator(button).click();
  const input = page.locator('#name-dialog #name-input');
  await expect(input).toBeFocused();
  await input.fill(value);
  await input.press('Enter');
}

async function createNote(page: Page, title: string): Promise<void> {
  await enterName(page, '#add-note', title);
  await expect(page.locator('#name-dialog')).toHaveCount(0);
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
  await enterName(page, '#add-folder', 'Work/Project');
  await expect(page.locator('#folder-path')).toHaveText('/Work/Project');
  await expect(page.locator('#tree')).toContainText('▸ Project');

  // 3단계는 창을 닫지 않고 이유를 보여 줌 (alert 없음)
  await enterName(page, '#add-folder', 'Work/Project/Deep');
  await expect(page.locator('#name-error')).toContainText('최대 2단계');
  await page.locator('#name-cancel').click();
  await expect(page.locator('#name-dialog')).toHaveCount(0);
  await expect(page.locator('#tree')).not.toContainText('Deep');
});

test('사용 의견(2026-11-13): 이름 창 Enter 확인·취소, 제목 Enter → 본문, 연 메모와 그 폴더만 강조, 표시 설명', async ({ page }) => {
  await page.goto('/');
  // 새 폴더: 선택한 폴더 뒤에 이어 쓰도록 커서가 끝에 있음
  await enterName(page, '#add-folder', '업무');
  await page.locator('#add-folder').click();
  await expect(page.locator('#name-input')).toHaveValue('업무/');
  await page.locator('#name-input').press('End');
  await page.locator('#name-input').pressSequentially('회의');
  await page.locator('#name-input').press('Enter');
  await expect(page.locator('#folder-path')).toHaveText('/업무/회의');

  // 새 메모는 선택한 폴더에 만들어지고, 저장 위치가 창에 보임
  await page.locator('#add-note').click();
  await expect(page.locator('#name-dialog .hint')).toContainText('/업무/회의');
  await page.locator('#name-input').fill('회의록');
  await page.locator('#name-input').press('Enter');
  await expect(page.locator('#title')).toHaveValue('회의록');

  // 취소하면 아무것도 만들지 않음
  await page.locator('#add-note').click();
  await page.locator('#name-input').press('Escape');
  await expect(page.locator('#name-dialog')).toHaveCount(0);
  await expect(page.locator('#count')).toHaveText('1 notes');

  // 제목에서 Enter → 줄 바꿈 없이 본문으로 이동
  await page.locator('#title').click();
  await page.locator('#title').press('End');
  await page.locator('#title').press('Enter');
  await expect(page.locator('#editor')).toBeFocused();
  await expect(page.locator('#title')).toHaveValue('회의록');

  // 루트로 가서 다른 메모를 만든 뒤 다시 '회의록'을 열면: 그 메모와 그 폴더만 강조
  await page.locator('#tree button', { hasText: '▸ /' }).click();
  await createNote(page, '루트 메모');
  await page.locator('#tree button', { hasText: '회의록' }).click();
  const selected = page.locator('#tree button[aria-selected="true"]');
  await expect(selected).toHaveCount(2);
  await expect(selected.nth(0)).toHaveText('▸ 회의');
  await expect(selected.nth(1)).toHaveText('· 회의록.md');
  await expect(page.locator('#folder-path')).toHaveText('/업무/회의');

  // 정리 영역 표시 설명
  await expect(page.locator('#depth-hint')).toHaveText('폴더 2단계까지');
  await page.locator('#organizer-help summary').click();
  await expect(page.locator('#organizer-help')).toContainText('한 폴더 안의 폴더 개수 제한은 없습니다');
  await expect(page.locator('#wifi')).toHaveText('네트워크: 온라인');
});

test('프로젝트(ADR-0005): 만들기·전환하면 폴더와 메모가 따로, 메모 옮기기, 이름 바꾸기, 빈 프로젝트만 삭제', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#project-select')).toHaveValue('');
  await expect(page.locator('#project-select option:checked')).toHaveText('기본');
  await expect(page.locator('#delete-project')).toBeDisabled();
  await createNote(page, '기본 메모');

  // 새 프로젝트 → 그 프로젝트로 전환, 트리는 비어 있음
  await page.locator('#add-project').click();
  await page.locator('#name-input').fill('@개인');
  await page.locator('#name-input').press('Enter');
  await expect(page.locator('#name-error')).toContainText('@로 시작할 수 없습니다');
  await page.locator('#name-input').fill('업무');
  await page.locator('#name-input').press('Enter');
  await expect(page.locator('#project-select option:checked')).toHaveText('업무');
  await expect(page.locator('#tree')).not.toContainText('기본 메모');
  await expect(page.locator('#count')).toHaveText('0 notes');

  // 프로젝트 안의 폴더 2단계 + 메모 (기본 프로젝트와 같은 이름도 허용)
  await enterName(page, '#add-folder', '회의/2024');
  await page.locator('#tree button', { hasText: '▸ /' }).click();
  await createNote(page, '기본 메모');
  await expect(page.locator('#tree')).toContainText('▸ 2024');
  await expect(page.locator('#count')).toHaveText('1 notes');

  // 같은 이름 프로젝트는 거절
  await page.locator('#add-project').click();
  await page.locator('#name-input').fill('업무');
  await page.locator('#name-input').press('Enter');
  await expect(page.locator('#name-error')).toContainText('같은 이름');
  await page.locator('#name-cancel').click();

  // 기본으로 돌아가면 그 폴더는 보이지 않음
  await page.locator('#project-select').selectOption({ label: '기본' });
  await expect(page.locator('#tree')).not.toContainText('2024');
  await expect(page.locator('#count')).toHaveText('1 notes');

  // 메모를 기본 → 업무로 옮기면 같은 경로 메모가 있어 거절
  await page.locator('#tree button', { hasText: '기본 메모' }).click();
  await page.locator('#note-project').selectOption({ label: '프로젝트 이동: 업무' });
  await expect(page.locator('#project-status')).toContainText('같은 이름의 메모');

  // 이름 바꾸기
  await page.locator('#project-select').selectOption({ label: '업무' });
  await page.locator('#rename-project').click();
  await expect(page.locator('#name-input')).toHaveValue('업무');
  await page.locator('#name-input').fill('회사');
  await page.locator('#name-input').press('Enter');
  await expect(page.locator('#project-select option:checked')).toHaveText('회사');

  // 메모가 있으면 지울 수 없음 → 메모를 기본이 아닌 새 프로젝트로 옮긴 뒤 지울 수 있음
  await page.locator('#delete-project').click();
  await expect(page.locator('#project-status')).toContainText('메모가 1개 있어');
  await page.locator('#add-project').click();
  await page.locator('#name-input').fill('보관');
  await page.locator('#name-input').press('Enter');
  await page.locator('#project-select').selectOption({ label: '회사' });
  await page.locator('#tree button', { hasText: '기본 메모' }).click();
  await page.locator('#note-project').selectOption({ label: '프로젝트 이동: 보관' });
  await expect(page.locator('#project-select option:checked')).toHaveText('보관');
  await expect(page.locator('#tree')).toContainText('기본 메모');
  await page.locator('#project-select').selectOption({ label: '회사' });
  await page.locator('#delete-project').click();
  await expect(page.locator('#project-select option:checked')).toHaveText('기본');
  await expect(page.locator('#project-select option')).toHaveText(['기본', '보관']);

  // 새로고침해도 프로젝트·현재 프로젝트 유지
  await page.locator('#project-select').selectOption({ label: '보관' });
  await page.reload();
  await expect(page.locator('#project-select option:checked')).toHaveText('보관');
  await expect(page.locator('#tree')).toContainText('기본 메모');
});

test('사용 의견(2026-11-13 2차): 새 메모 기본 제목은 일시 Memo, 폴더 이동, 메모 삭제(확인 창)', async ({ page }) => {
  await page.goto('/');
  await enterName(page, '#add-folder', '보관');
  await page.locator('#tree button', { hasText: '▸ /' }).click();
  await page.locator('#add-note').click();
  await expect(page.locator('#name-input')).toHaveValue(/^\d{4}-\d{2}-\d{2} \d{4} Memo$/);
  await page.locator('#name-input').press('Enter');
  await expect(page.locator('#title')).toHaveValue(/ Memo$/);

  // 폴더 이동
  await createNote(page, '옮길 메모');
  await page.locator('#note-folder').selectOption({ label: '폴더 이동: /보관' });
  await expect(page.locator('#folder-path')).toHaveText('/보관');
  const selected = page.locator('#tree button[aria-selected="true"]');
  await expect(selected).toHaveText(['▸ 보관', '· 옮길 메모.md']);

  // 삭제: 취소하면 그대로, 확인하면 지워짐 (새로고침 후에도)
  await page.locator('#delete-note').click();
  await expect(page.locator('#confirm-cancel')).toBeFocused();
  await page.locator('#confirm-cancel').click();
  await expect(page.locator('#tree')).toContainText('옮길 메모');
  await page.locator('#delete-note').click();
  await page.locator('#confirm-ok').click();
  await expect(page.locator('#tree')).not.toContainText('옮길 메모');
  await expect(page.locator('#count')).toHaveText('1 notes');
  await page.reload();
  await expect(page.locator('#tree')).not.toContainText('옮길 메모');
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

test('계정: 서버 없이 실행하면 "서버에 연결되어 있지 않습니다" (메모는 계속 사용)', async ({ page }) => {
  await page.goto('/');
  await page.locator('#account-btn').click();
  await expect(page.locator('#account-dialog [role=alert]')).toContainText('서버에 연결되어 있지 않습니다');
  await page.locator('#account-close').click();
  await createNote(page, '서버 없이');
  await expect(page.locator('#title')).toHaveValue('서버 없이');
});
