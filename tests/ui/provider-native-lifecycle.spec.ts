import { expect, test, type Page } from '@playwright/test';

type Audit = { opens: { generation: number; providerId: string }[]; updates: { generation: number; theme: string }[]; closes: number[] };

async function nativeFieldFixture(page: Page, browse = true) {
  await page.route('**/src/backend.ts*', async (route) => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = true;
      let active = 0;
      let connection;
      let hasKey = false;
      window.__nativeKeyAudit = { opens: [], updates: [], closes: [] };
      export const backend = {
        async invoke(command, args = {}) {
          if (command === 'bootstrap') {
            return { ...await mockBackend.invoke(command, args), compatibleConnections: connection ? [connection] : [] };
          }
          if (command === 'save_compatible_connection') {
            connection = { ...args, id: 'compatible:00000000-0000-4000-8000-000000000001' };
            return connection.id;
          }
          if (command === 'has_compatible_key') return hasKey;
          if (command === 'enter_compatible_key') { hasKey = true; return true; }
          if (command === 'models' && args.providerId === connection?.id) {
            return [{ id: 'synthetic-model-id', name: 'Synthetic model', contextWindow: 32768 }];
          }
          if (command === 'open_provider_key_field') {
            if (active >= args.generation) throw new Error('The API key field has changed');
            active = args.generation;
            window.__nativeKeyAudit.opens.push({ generation: active, providerId: args.providerId });
            return;
          }
          if (command === 'update_provider_key_field') {
            window.__nativeKeyAudit.updates.push({ generation: args.generation, theme: args.theme });
            return;
          }
          if (command === 'close_provider_key_field') {
            window.__nativeKeyAudit.closes.push(args.generation);
            if (active === args.generation) active = 0;
            return;
          }
          if (command === 'focus_provider_key_field') return;
          if (command === 'save_provider_key_field') return false;
          return mockBackend.invoke(command, args);
        },
        listen: (event, callback) => mockBackend.listen(event, callback),
      };` });
  });
  await page.goto('/');
  const showSidebar = page.getByRole('button', { name: 'Show sidebar' });
  if (await showSidebar.isVisible()) await showSidebar.press('Enter');
  await page.getByRole('button', { name: 'Settings', exact: true }).press('Enter');
  if (browse) await page.locator('.provider-disclosure summary').press('Enter');
}

test('a system theme change updates the existing native field without replacing it', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await nativeFieldFixture(page);
  await page.getByRole('button', { name: 'Configure Claude' }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __nativeKeyAudit: Audit }).__nativeKeyAudit.opens.length)).toBe(1);
  await page.emulateMedia({ colorScheme: 'light' });
  await expect.poll(() => page.evaluate(() => (window as unknown as { __nativeKeyAudit: Audit }).__nativeKeyAudit.updates.at(-1)?.theme)).toBe('light');
  const audit = await page.evaluate(() => (window as unknown as { __nativeKeyAudit: Audit }).__nativeKeyAudit);
  expect(audit.opens).toHaveLength(1);
  expect(audit.updates.at(-1)?.generation).toBe(audit.opens[0].generation);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await page.getByRole('button', { name: 'Close provider settings' }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __nativeKeyAudit: Audit }).__nativeKeyAudit.closes.length)).toBe(1);
});

test('custom connections keep the address, actions, and model fields in flow', async ({ page }) => {
  await page.setViewportSize({ width: 500, height: 680 });
  await nativeFieldFixture(page, false);
  await page.getByRole('button', { name: 'Add custom connection' }).press('Enter');
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('textbox', { name: 'Connection name' }).fill('Local test server');
  const address = `http://127.0.0.1:9500/${'long-path'.repeat(35)}/v1`;
  await dialog.getByRole('textbox', { name: 'Base URL' }).fill(address);
  await dialog.getByRole('button', { name: 'Add connection', exact: true }).press('Enter');
  await expect(dialog.getByText(address, { exact: true })).toBeVisible();
  await expect(dialog.getByRole('textbox', { name: 'Model ID' })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Add API key' }).press('Enter');
  await expect(dialog.getByRole('button', { name: 'Replace API key' })).toBeVisible();
  const audit = await page.evaluate(() => (window as unknown as { __nativeKeyAudit: Audit }).__nativeKeyAudit);
  expect(audit.opens).toHaveLength(0);
  await dialog.getByRole('button', { name: 'Test connection' }).press('Enter');
  await dialog.getByRole('combobox', { name: 'Discovered models' }).selectOption('synthetic-model-id');
  await expect(dialog.getByRole('textbox', { name: 'Display name' })).toHaveValue('Synthetic model');
  await expect(dialog.getByRole('spinbutton', { name: 'Context limit (optional)' })).toHaveValue('32768');
  expect(await dialog.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
  await dialog.getByRole('button', { name: 'Save model' }).press('Enter');
  await expect(dialog.getByRole('button', { name: 'Synthetic model' })).toBeVisible();
});

test('switching Qwen routes replaces and closes the old native field', async ({ page }) => {
  await nativeFieldFixture(page);
  await page.getByRole('button', { name: 'Configure QwenCloud' }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __nativeKeyAudit: Audit }).__nativeKeyAudit.opens.length)).toBe(1);
  await page.getByRole('button', { name: 'Token Plan', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __nativeKeyAudit: Audit }).__nativeKeyAudit.opens.length)).toBe(2);
  const audit = await page.evaluate(() => (window as unknown as { __nativeKeyAudit: Audit }).__nativeKeyAudit);
  expect(audit.opens.map((entry) => entry.providerId)).toEqual(['qwen-standard', 'qwen-token-plan']);
  expect(audit.closes).toContain(audit.opens[0].generation);
  expect(audit.opens[1].generation).not.toBe(audit.opens[0].generation);
});
