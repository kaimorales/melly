import { expect, test, type Page } from '@playwright/test';

async function nativePreview(page: Page, connected = false) {
  await page.route('**/src/backend.ts*', async (route) => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = true;
      export const backend = {
        async invoke(command, args = {}) {
          if (command === 'frontend_ready') return;
          const result = await mockBackend.invoke(command, args);
          if (command === 'bootstrap' && ${connected}) {
            const provider = result.providers.find((item) => item.id === 'chatgpt');
            provider.configured = true;
            provider.status = 'Plan saved · checked when used';
          }
          return result;
        },
        listen: (event, callback) => mockBackend.listen(event, callback),
      };` });
  });
  await page.goto('/');
}

test('the desktop offers provider setup instead of a demo', async ({ page }) => {
  await nativePreview(page);
  await expect(page.getByRole('button', { name: 'Connect provider' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Send message' })).toBeDisabled();
  await expect(page.getByText('Demo', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Try an example' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Connect provider' }).click();
  await expect(page.getByRole('heading', { name: 'Providers' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Configure Demo' })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Try Melly' })).toHaveCount(0);
});

test('a previously connected provider can be selected without a demo or billing fallback', async ({ page }) => {
  await nativePreview(page, true);
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await expect(page.getByRole('option', { name: /ChatGPT/ })).toBeVisible();
  await expect(page.getByRole('option', { name: /Demo/ })).toHaveCount(0);
  await page.getByRole('option', { name: /ChatGPT/ }).click();
  await expect(page.getByRole('button', { name: 'Choose model' })).toContainText('Plan');
});
