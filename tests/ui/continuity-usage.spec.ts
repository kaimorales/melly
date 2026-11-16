import { expect, test } from '@playwright/test';

test('editing creates a new branch and preserves the original conversation', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill('Synthetic original question');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry response' })).toBeVisible();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await expect(page.getByText('Editing in a new branch', { exact: true })).toBeVisible();
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill('Synthetic revised question');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry response' })).toBeVisible();
  await expect(page.locator('.user-message')).toHaveText('Synthetic revised question');
  await page.getByRole('button', { name: 'Synthetic original question', exact: true }).click();
  await expect(page.locator('.user-message')).toHaveText('Synthetic original question');
});

test('context shows a compact estimate when capacity is unknown', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill('A synthetic draft for context inspection');
  await page.getByRole('button', { name: 'Context and tokens' }).press('Enter');
  const dialog = page.getByRole('dialog');
  await expect(dialog.locator('.context-total')).toHaveText(/≈ [\d,]+ tokens/);
  await expect(dialog).not.toContainText('Reported input');
  await expect(dialog.getByText('Details', { exact: true })).toHaveCount(0);
  await expect(dialog.locator('.context-bar')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('textbox', { name: 'Message', exact: true })).toHaveValue('A synthetic draft for context inspection');
});

test('browser preview does not offer native attachment imports', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Attach files', exact: true })).toHaveCount(0);
  await expect(page.locator('.attachment-draft')).toHaveCount(0);
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
