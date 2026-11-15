import { expect, test } from '@playwright/test';

test('proposes and applies a change in the conversation', async ({ page }) => {
  await page.route('**/src/backend.ts*', async route => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = true;
      let request;
      let response;
      let stream;
      window.__fileEdits = [];
      export const backend = {
        async invoke(command, args = {}) {
          if (command === 'bootstrap') {
            const state = await mockBackend.invoke(command, args);
            const provider = state.providers.find(entry => entry.id === 'muse-api');
            return { ...state, providers: [{ ...provider, configured: true }], settings: { ...state.settings, defaultProvider: provider.id, defaultModel: provider.connection.modelId } };
          }
          if (command === 'start_chat') {
            request = args.input;
            response = { id: 'answer', chatId: request.chatId, role: 'assistant', content: '', status: 'streaming', createdAt: 2, sources: [] };
            setTimeout(() => {
              response = { ...response, status: 'complete', content: ${JSON.stringify('I proposed changing the color.\n\n```melly-file-edit\n{"path":"sample.txt","edits":[{"old":"red","new":"green"}]}\n```')} };
              stream?.({ chatId: request.chatId, messageId: response.id, kind: 'done', text: '', sources: [] });
            }, 60);
            return response;
          }
          if (command === 'messages' && request?.chatId === args.chatId) {
            return [{ id: 'question', chatId: request.chatId, role: 'user', status: 'complete', content: request.content, createdAt: 1, sources: [] }, response];
          }
          if (command === 'applied_file_edit') return null;
          if (command === 'review_file_edit' || command === 'apply_file_edit') {
            window.__fileEdits.push({ command, args });
            if (command === 'review_file_edit') return ${JSON.stringify({ status: 'review', ticket: 'prepared', path: 'sample.txt', diff: [{ kind: '-', text: 'color=red' }, { kind: '+', text: 'color=green' }] })};
            return;
          }
          return mockBackend.invoke(command, args);
        },
        listen: (event, callback) => {
          if (event === 'chat-stream') stream = callback;
          return mockBackend.listen(event, callback);
        },
      };
    ` });
  });
  await page.goto('/');
  const composer = page.getByRole('textbox', { name: 'Message', exact: true });
  await composer.fill('Change sample.txt from red to green.');
  await page.getByRole('button', { name: 'Send message' }).press('Enter');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText('I proposed changing the color.')).toBeVisible();
  await expect(page.getByText('melly-file-edit', { exact: false })).toHaveCount(0);
  await page.getByRole('button', { name: 'Review file change' }).press('Enter');
  const review = page.getByRole('region', { name: 'Proposed file change' });
  await expect(review.getByText('color=green')).toBeVisible();
  const commands = () => page.evaluate(() => (window as unknown as { __fileEdits: { command: string }[] }).__fileEdits.map(entry => entry.command));
  expect(await commands()).toEqual(['review_file_edit']);
  await review.getByRole('button', { name: 'Apply change' }).press('Enter');
  await expect(page.locator('summary').getByText('sample.txt')).toBeVisible();
  expect(await commands()).toEqual(['review_file_edit', 'apply_file_edit']);
});
