import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
});

test('provider settings show an obvious key field and a plainly named model', async ({ page }) => {
  await page.getByRole('button', { name: 'Configure Claude' }).click();
  const dialog = page.getByRole('dialog', { name: 'Claude' });
  await expect(dialog.getByRole('textbox', { name: 'Model', exact: true })).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'API key', exact: true })).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Save', exact: true })).toBeVisible();
  await expect(dialog).not.toContainText('Keychain');
  await expect(dialog).not.toContainText('Base URL');
  await dialog.getByRole('button', { name: 'API key', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('desktop app');
  await expect(page.locator('input[type=password]')).toHaveCount(0);
});

test('provider detail actions look like buttons in both themes', async ({ page }) => {
  for (const theme of ['Light', 'Dark']) {
    await page.getByRole('button', { name: theme, exact: true }).click();
    const action = page.getByRole('button', { name: 'Configure Meta Muse Code' });
    const color = await action.evaluate((element) => getComputedStyle(element).backgroundColor);
    expect(color).not.toBe('rgba(0, 0, 0, 0)');
  }
});

test('model refresh has a visible button surface on light and dark dialogs', async ({ page }) => {
  for (const theme of ['Light', 'Dark']) {
    await page.getByRole('button', { name: theme, exact: true }).click();
    await page.getByRole('button', { name: 'Configure Claude' }).click();
    const colors = await page.getByRole('button', { name: 'Refresh models' }).evaluate((button) => ({
      button: getComputedStyle(button).backgroundColor,
      dialog: getComputedStyle(button.closest('.dialog')!).backgroundColor,
    }));
    expect(colors.button).not.toBe(colors.dialog);
    expect(colors.button).not.toBe('rgba(0, 0, 0, 0)');
    await page.getByRole('button', { name: 'Close provider settings' }).click();
  }
});

test('Qwen keeps its routes and Custom keeps its editable endpoint', async ({ page }) => {
  await page.getByRole('button', { name: 'Configure QwenCloud' }).click();
  await page.getByRole('button', { name: 'Token Plan', exact: true }).click();
  await expect(page.getByText('Token Plan subscription', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'API key', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close provider settings' }).click();
  await page.getByRole('button', { name: 'Configure Custom' }).click();
  await expect(page.getByRole('textbox', { name: 'Base URL' })).toBeEditable();
  await expect(page.getByRole('button', { name: 'API key', exact: true })).toBeVisible();
});
