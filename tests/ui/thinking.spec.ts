import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'Message' }).fill('Demonstrate thinking');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
});

test('reasoning streams separately, collapses for the answer, and can be reopened', async ({ page }) => {
  const toggle = page.getByRole('button', { name: /^Thinking/ });
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('.thinking-grid[data-active="true"] > span')).toHaveCount(9);
  await expect(page.locator('.thinking-body')).toContainText('synthetic thinking');
  await expect(page.locator('.assistant .markdown')).toHaveCount(0);
  await expect(page.locator('.assistant .markdown')).toContainText('The answer is 42');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(page.locator('.thinking-grid')).toHaveCount(0);
  await toggle.click();
  await expect(page.locator('.thinking-body')).toBeVisible();
  await expect(page.locator('.assistant .markdown')).not.toContainText('compare the options');
  await toggle.focus();
  await page.keyboard.press('Space');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
});

test('Stop preserves partial thinking and Retry replaces that response', async ({ page }) => {
  await expect(page.locator('.thinking-body')).toContainText('synthetic thinking');
  await page.getByRole('button', { name: 'Stop response' }).click();
  await expect(page.getByText('Response stopped', { exact: true })).toBeVisible();
  await expect(page.locator('.thinking-grid')).toHaveCount(0);
  await page.getByRole('button', { name: /^Thinking/ }).click();
  await expect(page.locator('.thinking-body')).toContainText('synthetic thinking');
  await page.getByRole('button', { name: 'Retry response' }).click();
  await expect(page.locator('.user-message')).toHaveCount(1);
  await expect(page.locator('.assistant .markdown')).toContainText('The answer is 42');
});

test('a manual collapse is respected while thinking continues', async ({ page }) => {
  const toggle = page.getByRole('button', { name: /^Thinking/ });
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(page.locator('.thinking-body')).toContainText('check the result');
  await expect(page.locator('.thinking-body')).toBeHidden();
});

test('reduced motion leaves the nine-square indicator still', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const cells = page.locator('.thinking-grid[data-active="true"] > span');
  await expect(cells).toHaveCount(9);
  await expect(cells.first()).toHaveCSS('animation-name', 'none');
});
