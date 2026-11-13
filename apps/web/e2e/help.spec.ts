// === 처음 실행 도움말 (2026-11-13 사용 의견: 최초 페이지로 MD 작성 매뉴얼) + 상단 버튼 문구 ===
import { expect, test } from '@playwright/test';

test('처음 실행하면 사용법이 뜨고, 다시 열면 뜨지 않음. 메모로 만들 수 있음', async ({ page }) => {
  await page.goto('/');
  const help = page.locator('#help-dialog');
  await expect(help).toBeVisible();
  await expect(page.locator('#help-text')).toContainText('## Markdown 쓰는 법');
  await page.locator('#help-close').click();
  await expect(help).toBeHidden();
  await page.reload();
  await expect(page.locator('#title')).toBeVisible();
  await expect(help).toBeHidden();

  // 도움말 버튼 → 메모로 만들기
  await page.locator('#help-btn').click();
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
