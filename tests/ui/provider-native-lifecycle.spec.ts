import { expect, test, type Page } from '@playwright/test';

type Audit = { opens: { generation: number; providerId: string }[]; updates: { generation: number; theme: string }[]; closes: number[] };

async function nativeFieldFixture(page: Page) {
  await page.route('**/src/backend.ts*', async (route) => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = true;
      let active = 0;
      window.__nativeKeyAudit = { opens: [], updates: [], closes: [] };
      export const backend = {
        async invoke(command, args = {}) {
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
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
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
