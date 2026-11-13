import { expect, test, type Page } from '@playwright/test';

// These fixtures replace only the browser preview adapter. They contain no account or key.
async function fixture(page: Page, configured: string[] = ['claude', 'qwen-standard', 'qwen-token-plan'], fail = false) {
  await page.route('**/src/mock-backend.ts*', async (route) => {
    const response = await route.fetch();
    const code = await response.text();
    await route.fulfill({ response, body: `${code}\n
      const fixtureConfigured = new Set(${JSON.stringify(configured)});
      const fixtureInvoke = mockBackend.invoke.bind(mockBackend);
      if (fixtureConfigured.has('custom')) await fixtureInvoke('save_connection', {connection:{providerId:'custom', baseUrl:'https://first.example.com/v1', modelId:'configured-custom-model'}});
      window.__pickerFixture = { calls: [], fail: ${JSON.stringify(fail)}, completeSignIn: () => { fixtureConfigured.add('chatgpt'); emit('connection-update', {status:'connected', message:'Synthetic plan consent completed.'}); } };
      mockBackend.invoke = async (command, args = {}) => {
        if (command === 'connect_chatgpt') { emit('connection-update', {status:'signing-in', message:'Complete synthetic sign-in in the browser.'}); return; }
        if (command === 'models') {
          const state = await fixtureInvoke('bootstrap');
          const provider = state.providers.find(entry => entry.id === args.providerId);
          window.__pickerFixture.calls.push({providerId:args.providerId, baseUrl:provider.connection.baseUrl});
          await new Promise(resolve => setTimeout(resolve, 200));
          if (window.__pickerFixture.fail) throw new Error('Synthetic discovery unavailable.');
          return [{id:args.providerId + '-fixture-one', name:'Fixture model one'}, {id:args.providerId + '-fixture-two', name:'Fixture model two'}];
        }
        const result = await fixtureInvoke(command, args);
        if (command === 'bootstrap') for (const provider of result.providers) {
          if (fixtureConfigured.has(provider.id)) { provider.configured = true; provider.status = 'Fixture connection · not live'; }
        }
        return result;
      };` });
  });
  await page.goto('/');
  await expect(page.getByText('Browser preview · Mock', { exact: true })).toBeVisible();
}

test('split picker discovers only the chosen family and caches its model list', async ({ page }) => {
  await fixture(page);
  expect(await page.evaluate(() => window.__pickerFixture.calls)).toEqual([]);
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await expect(page.getByRole('option', { name: /ChatGPT/ })).toHaveCount(0);
  await expect(page.getByRole('option', { name: /Gemini/ })).toHaveCount(0);
  expect(await page.evaluate(() => window.__pickerFixture.calls)).toEqual([]);
  await page.getByRole('option', { name: /Claude/ }).click();
  await expect(page.getByText('Loading models…', { exact: true })).toBeVisible();
  await page.getByRole('option', { name: /Fixture model two/ }).click();
  await expect(page.getByRole('button', { name: 'Choose model', exact: true })).toContainText('claude-fixture-two');
  await page.getByRole('button', { name: 'Choose model', exact: true }).click();
  await expect(page.getByRole('option', { name: /Fixture model two/ })).toHaveAttribute('aria-selected', 'true');
  expect(await page.evaluate(() => window.__pickerFixture.calls.length)).toBe(1);
  await page.getByRole('button', { name: 'Refresh models', exact: true }).click();
  await expect(page.getByRole('option', { name: /Fixture model one/ })).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.__pickerFixture.calls.length)).toBe(2);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Choose model', exact: true })).toBeFocused();
});

test('Qwen keeps Standard API and Token Plan model discovery on distinct routes', async ({ page }) => {
  await fixture(page);
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await page.getByRole('option', { name: /QwenCloud.*Token Plan/ }).click();
  await page.getByRole('option', { name: /Fixture model one/ }).click();
  await expect(page.getByRole('button', { name: 'Choose model', exact: true })).toContainText('Token Plan');
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await page.getByRole('option', { name: /QwenCloud.*Standard API/ }).click();
  await page.getByRole('option', { name: /Fixture model two/ }).click();
  const calls = await page.evaluate(() => window.__pickerFixture.calls);
  expect(calls.map(call => call.providerId)).toEqual(['qwen-token-plan', 'qwen-standard']);
  expect(calls[0].baseUrl).not.toBe(calls[1].baseUrl);
});

test('discovery errors preserve the configured model and allow an explicit model ID', async ({ page }) => {
  await fixture(page, ['claude'], true);
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await page.getByRole('option', { name: /Claude/ }).click();
  await expect(page.getByRole('alert')).toContainText('Synthetic discovery unavailable');
  await expect(page.getByRole('option', { name: /Configured model ID/ })).toBeVisible();
  await page.getByRole('button', { name: 'Enter model ID', exact: true }).click();
  await page.getByRole('textbox', { name: 'Model ID', exact: true }).fill('my-explicit-model');
  await page.getByRole('button', { name: 'Use model', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Choose model', exact: true })).toContainText('my-explicit-model');
  await page.getByRole('button', { name: 'Choose model', exact: true }).click();
  await expect(page.getByRole('option', { name: /my-explicit-model.*Configured model ID/ })).toHaveAttribute('aria-selected', 'true');
});

test('successful ChatGPT sign-in discovers account models without startup discovery', async ({ page }) => {
  await fixture(page, []);
  expect(await page.evaluate(() => window.__pickerFixture.calls)).toEqual([]);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Configure ChatGPT', exact: true }).click();
  await page.getByRole('button', { name: 'Continue with ChatGPT', exact: true }).click();
  await expect(page.getByText('Sign-in pending', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => window.__pickerFixture.calls)).toEqual([]);
  await page.evaluate(() => window.__pickerFixture.completeSignIn());
  await expect(page.getByText('Fixture model one', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => window.__pickerFixture.calls.map(call => call.providerId))).toEqual(['chatgpt']);
  await page.getByRole('button', { name: 'Close provider settings' }).click();
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await page.getByRole('option', { name: /ChatGPT/ }).click();
  await page.getByRole('option', { name: /Fixture model two/ }).click();
  expect(await page.evaluate(() => window.__pickerFixture.calls.length)).toBe(1);
});

test('changing a custom endpoint requires a separate model discovery result', async ({ page }) => {
  await fixture(page, ['custom']);
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await page.getByRole('option', { name: /Custom/ }).click();
  await page.getByRole('option', { name: /Fixture model one/ }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Configure Custom', exact: true }).click();
  await page.getByRole('textbox', { name: 'Base URL', exact: true }).fill('https://second.example.com/v1');
  await page.getByRole('button', { name: 'Save connection', exact: true }).click();
  await expect(page.getByText('Connection saved.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close provider settings' }).click();
  await page.getByRole('button', { name: 'New Chat', exact: true }).click();
  expect(await page.evaluate(() => window.__pickerFixture.calls.length)).toBe(1);
  await page.getByRole('button', { name: 'Choose model', exact: true }).click();
  await expect(page.getByText('Loading models…', { exact: true })).toBeVisible();
  await page.getByRole('option', { name: /Fixture model two/ }).click();
  expect(await page.evaluate(() => window.__pickerFixture.calls.map(call => call.baseUrl))).toEqual(['https://first.example.com/v1', 'https://second.example.com/v1']);
});

test('both picker halves support arrow keys, Enter, and focus return', async ({ page }) => {
  await fixture(page, ['claude']);
  await page.getByRole('button', { name: 'Choose provider' }).click();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('option', { name: /Claude/ })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('option', { name: /Fixture model one/ })).toBeVisible();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('option', { name: /Fixture model one/ })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('button', { name: 'Choose model', exact: true })).toContainText('claude-fixture-one');
  await expect(page.getByRole('button', { name: 'Choose model', exact: true })).toBeFocused();
});
