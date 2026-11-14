// === 처음 실행 도움말 (2026-11-13 사용 의견: 최초 페이지로 MD 작성 매뉴얼) + 상단 버튼 문구 ===
import { expect, test, type Page } from '@playwright/test';

/** 본문 아래 '더보기' 열기 (형식·다운로드·이동·가져오기·삭제) */
async function openMore(page: Page): Promise<void> {
  const more = page.locator('#more-actions');
  if (!(await more.evaluate((d: HTMLDetailsElement) => d.open))) await more.locator('summary').click();
}

test('처음 실행하면 사용법이 첫 문서로 열리고(한 번만), 도움말 창에서도 볼 수 있음', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#title')).toHaveValue('MemoNest 사용법');
  await expect(page.locator('#editor')).toHaveValue(/## Markdown 쓰는 법/);
  await expect(page.locator('#count')).toHaveText('1 notes');
  // 지워도 다시 만들지 않음
  await openMore(page);
  await page.locator('#delete-note').click();
  await page.locator('#confirm-ok').click();
  await expect(page.locator('#count')).toHaveText('0 notes');
  await page.reload();
  await expect(page.locator('#title')).toBeVisible();
  await expect(page.locator('#count')).toHaveText('0 notes');

  await page.locator('#help-btn').click();
  await expect(page.locator('#help-text')).toContainText('## Markdown 쓰는 법');

  // 도움말 창 → 메모로 만들기
  await page.locator('#help-to-note').click();
  await expect(page.locator('#title')).toHaveValue('MemoNest 사용법');
  await expect(page.locator('#editor')).toHaveValue(/# MemoNest 사용법/);
});

test('상단 버튼은 하는 일을 그대로 보여 줌 (언어 선택, 메모만/목록만 보기, 목록+메모 함께)', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('memonest.helpShown', '1'));
  await page.goto('/');
  await expect(page.locator('#language-btn')).toHaveText('언어 선택 · KO/EN');
  await expect(page.locator('#toggle-btn')).toHaveText('메모만 보기 (F6)');
  await expect(page.locator('#split-btn')).toBeHidden(); // 이미 함께 보는 중
  await page.locator('#toggle-btn').click();
  await expect(page.locator('#toggle-btn')).toHaveText('목록만 보기 (F6)');
  await expect(page.locator('#split-btn')).toHaveText('목록+메모 함께');
  await page.locator('#language-btn').click();
  await expect(page.locator('#toggle-btn')).toHaveText('List only (F6)');
});

test('모바일 세로 화면(사용 의견 5차): 상단은 아이콘, 하단 버튼 한 줄, 맨 위·마지막 작성 위치, 용량 표시', async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 }, locale: 'ko-KR' });
  await context.addInitScript(() => localStorage.setItem('memonest.helpShown', '1'));
  const page = await context.newPage();
  await page.goto('/');
  await expect(page.locator('#account-btn')).toHaveText('👤');
  await expect(page.locator('#account-btn')).toHaveAttribute('aria-label', '계정');
  await expect(page.locator('#language-btn')).toHaveText('🌐');
  // 상단 막대가 한 줄 높이 안에 들어감
  const bar = await page.locator('.topbar').boundingBox();
  expect(bar!.height).toBeLessThanOrEqual(56);
  await expect(page.locator('#storage-use')).toHaveText(/^용량: .+ \(이 기기\)$/);

  await page.locator('#add-note').click();
  await page.locator('#name-input').fill('긴 메모');
  await page.locator('#name-input').press('Enter');
  const long = Array.from({ length: 200 }, (_, i) => `줄 ${i + 1}`).join('\n');
  await page.locator('#editor').fill(long);
  // 하단 버튼이 한 줄 (모바일에서 3줄로 나오던 문제)
  const actions = await page.locator('.doc-actions').boundingBox();
  expect(actions!.height).toBeLessThanOrEqual(60);
  // 가운데쯤 고치면 그 자리가 '마지막으로 쓴 위치'
  await page.locator('#editor').evaluate((el: HTMLTextAreaElement) => {
    const at = el.value.indexOf('줄 100');
    el.setSelectionRange(at, at);
    el.focus();
  });
  await page.keyboard.type('★');
  // 쓰는 동안(모바일)은 키보드 위 막대의 ⤒ ✎
  await page.locator('#key-bar [data-key="top"]').click();
  expect(await page.locator('#editor').evaluate((el: HTMLTextAreaElement) => [el.selectionStart, el.scrollTop])).toEqual([0, 0]);
  await page.locator('#key-bar [data-key="last"]').click();
  const [pos, value, scroll] = await page
    .locator('#editor')
    .evaluate((el: HTMLTextAreaElement) => [el.selectionStart, el.value, el.scrollTop] as const);
  expect((value as string).slice((pos as number) - 1, pos as number)).toBe('★');
  expect(scroll).toBeGreaterThan(0);
  await context.close();
});

test('더보기(사용 의견 6차): 이름 붙은 항목, 밖을 누르면 닫힘, 항목을 고르면 닫힘', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('memonest.helpShown', '1'));
  await page.goto('/');
  await page.locator('#add-note').click();
  await page.locator('#name-input').press('Enter');
  const more = page.locator('#more-actions');
  await openMore(page);
  await expect(page.locator('#more-panel .more-row span')).toHaveText(['프로젝트 이동', '폴더 이동', '확장자']);
  await expect(page.locator('#format option')).toHaveText(['.md (Markdown)', '.txt (텍스트)']);
  // 메모 창(메뉴가 덮지 않은 곳)을 누르면 닫힘
  await page.locator('#editor').click({ position: { x: 30, y: 20 } });
  await expect(more).not.toHaveAttribute('open', '');
  // 항목을 고르면 닫힘
  await openMore(page);
  await page.locator('#format').selectOption('txt');
  await expect(more).not.toHaveAttribute('open', '');
});

test('모바일 키보드 막대(사용 의견 6차): 쓰는 동안 ↶ ↷ 커서 ⤒ ✎ ⋯, 더보기는 가려지지 않음', async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 }, locale: 'ko-KR', hasTouch: true });
  await context.addInitScript(() => localStorage.setItem('memonest.helpShown', '1'));
  const page = await context.newPage();
  await page.goto('/');
  await page.locator('#add-note').click();
  await page.locator('#name-input').press('Enter');
  const bar = page.locator('#key-bar');
  await expect(bar).toBeHidden();
  await page.locator('#editor').fill('첫째 줄\n둘째 줄\n셋째');
  await page.locator('#editor').focus();
  await expect(bar).toBeVisible();
  const caret = () => page.locator('#editor').evaluate((el: HTMLTextAreaElement) => el.selectionStart);
  await page.locator('#editor').evaluate((el: HTMLTextAreaElement) => el.setSelectionRange(2, 2)); // 첫째 줄 2번째
  await bar.locator('[data-key="down"]').click();
  expect(await caret()).toBe(5 + 2); // 둘째 줄 같은 칸
  await bar.locator('[data-key="right"]').click();
  expect(await caret()).toBe(8);
  await bar.locator('[data-key="up"]').click();
  expect(await caret()).toBe(3);
  await bar.locator('[data-key="left"]').click();
  expect(await caret()).toBe(2);
  await expect(page.locator('#editor')).toBeFocused(); // 막대를 눌러도 키보드(포커스) 유지
  await page.keyboard.type('X');
  await bar.locator('[data-key="undo"]').click();
  await expect(page.locator('#editor')).toHaveValue('첫째 줄\n둘째 줄\n셋째');
  // ⋯ → 더보기 시트가 화면 안에, 메모 창에 가려지지 않고 맨 위에 보임
  await bar.locator('[data-key="more"]').click();
  const panel = page.locator('#more-panel');
  await expect(panel).toBeVisible();
  const box = (await panel.boundingBox())!;
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.y + box.height).toBeLessThanOrEqual(844);
  const topmost = await page.evaluate(([x, y]) => document.elementFromPoint(x, y)?.closest('#more-panel') !== null, [
    box.x + box.width / 2,
    box.y + 10,
  ]);
  expect(topmost).toBe(true);
  await context.close();
});

