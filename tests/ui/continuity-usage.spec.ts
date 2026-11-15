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


test('the context limit belongs to its chat', async ({ page }) => {
  await page.goto('/');
  const openContext = async () => {
    await page.getByRole('button', { name: 'Context and tokens' }).press('Enter');
    return page.getByRole('combobox', { name: 'Context limit' });
  };
  let choice = await openContext();
  await choice.selectOption('65536');
  await expect(choice).toHaveValue('65536');
  await page.keyboard.press('Escape');

  const message = page.getByRole('textbox', { name: 'Message', exact: true });
  await message.fill('Context preference fixture');
  await page.getByRole('button', { name: 'Send message' }).press('Enter');
  await expect(page.getByRole('button', { name: 'Retry response' })).toBeVisible();
  choice = await openContext();
  await expect(choice).toHaveValue('65536');
  await page.keyboard.press('Escape');

  await page.getByRole('button', { name: 'New Chat', exact: true }).press('Enter');
  choice = await openContext();
  await expect(choice).toHaveValue('default');
  await page.keyboard.press('Escape');

  await page.getByRole('button', { name: 'Context preference fixture', exact: true }).press('Enter');
  choice = await openContext();
  await expect(choice).toHaveValue('65536');
});
