// === 처음 실행 도움말 (2026-11-13 사용 의견: 최초 페이지로 MD 작성 매뉴얼) + 상단 버튼 문구 ===
import { expect, test } from '@playwright/test';

test('처음 실행하면 사용법이 첫 문서로 열리고(한 번만), 도움말 창에서도 볼 수 있음', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#title')).toHaveValue('MemoNest 사용법');
  await expect(page.locator('#editor')).toHaveValue(/## Markdown 쓰는 법/);
  await expect(page.locator('#count')).toHaveText('1 notes');
  // 지워도 다시 만들지 않음
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
