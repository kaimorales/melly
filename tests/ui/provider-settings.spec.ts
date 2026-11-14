import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
});

test('provider browsing starts collapsed and can be toggled with the keyboard', async ({ page }) => {
  const browse = page.getByText('Browse providers', { exact: true });
  const ollama = page.getByRole('button', { name: 'Configure Ollama Cloud' });
  await expect(page.getByRole('button', { name: 'Configure Demo' })).toBeVisible();
  await expect(ollama).toBeHidden();
  await browse.focus();
  await page.keyboard.press('Enter');
  await expect(ollama).toBeVisible();
  await page.keyboard.press('Space');
  await expect(ollama).toBeHidden();
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(ollama).toBeHidden();
});

test('provider settings show an obvious key field and a plainly named model', async ({ page }) => {
  await page.getByText('Browse providers', { exact: true }).click();
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
  await page.getByText('Browse providers', { exact: true }).click();
  for (const theme of ['Light', 'Dark']) {
    await page.getByRole('button', { name: theme, exact: true }).click();
    const action = page.getByRole('button', { name: 'Configure Meta Model API' });
    const color = await action.evaluate((element) => getComputedStyle(element).backgroundColor);
    expect(color).not.toBe('rgba(0, 0, 0, 0)');
  }
});

test('model refresh has a visible button surface on light and dark dialogs', async ({ page }) => {
  await page.getByText('Browse providers', { exact: true }).click();
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
  await page.getByText('Browse providers', { exact: true }).click();
  await page.getByRole('button', { name: 'Configure QwenCloud' }).click();
  await page.getByRole('button', { name: 'Token Plan', exact: true }).click();
  await expect(page.getByText('Token Plan subscription', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'API key', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close provider settings' }).click();
  await page.getByRole('button', { name: 'Configure Custom' }).click();
  await expect(page.getByRole('textbox', { name: 'Base URL' })).toBeEditable();
  await expect(page.getByRole('button', { name: 'API key', exact: true })).toBeVisible();
});

test('Ollama Cloud setup identifies its key and credit billing', async ({ page }) => {
  await page.getByText('Browse providers', { exact: true }).click();
  await page.getByRole('button', { name: 'Configure Ollama Cloud' }).click();
  const dialog = page.getByRole('dialog', { name: 'Ollama Cloud' });
  const logo = dialog.locator('img');
  await expect(logo).toHaveAttribute('src', '/providers/ollama.svg');
  await expect.poll(() => logo.evaluate((image: HTMLImageElement) => image.naturalWidth)).toBe(17);
  await expect(dialog.getByText('API key · plan or usage credits')).toBeVisible();
  await expect(dialog.getByText('Plan credits first; purchased credits can follow.')).toBeVisible();
  await expect(dialog.getByRole('textbox', { name: 'Model' })).toHaveValue('gemma4:31b');
  await expect(dialog.getByRole('button', { name: 'API key' })).toBeVisible();
  await expect(dialog.getByRole('textbox', { name: 'Base URL' })).toHaveCount(0);
});

test('Meta Model API is the only Meta setup and has metered billing', async ({ page }) => {
  await page.getByText('Browse providers', { exact: true }).click();
  await expect(page.getByRole('button', { name: 'Configure Meta Muse Code' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Configure Meta Model API' }).click();
  const api = page.getByRole('dialog', { name: 'Meta Model API' });
  await expect(api.getByText('API key · Metered')).toBeVisible();
  await expect(api.getByRole('button', { name: 'API key', exact: true })).toBeVisible();
  await expect(api.getByRole('textbox', { name: 'Model' })).toHaveValue('muse-spark-1.3');
});
