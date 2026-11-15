import { expect, test } from '@playwright/test';

test('context shows a compact estimate when capacity is unknown', async ({ page }) => {
  await page.goto('/');
  const message = page.getByRole('textbox', { name: 'Message', exact: true });
  await message.fill('A synthetic draft for context inspection');
  await page.getByRole('button', { name: 'Context and tokens' }).press('Enter');
  const dialog = page.getByRole('dialog', { name: 'Context', exact: true });
  await expect(dialog.locator('.context-total')).toHaveText(/≈ [\d,]+ tokens/);
  await expect(dialog.getByText('Details', { exact: true })).toHaveCount(0);
  await expect(dialog.locator('.context-bar')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(message).toHaveValue('A synthetic draft for context inspection');
});
