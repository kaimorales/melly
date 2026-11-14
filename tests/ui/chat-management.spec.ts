import { expect, test, type Page } from '@playwright/test';

const folder = (page: Page, title: string) => page.locator('.folder-wrap').filter({ has: page.getByRole('button', { name: title, exact: true }) });
const recent = (page: Page) => page.getByRole('group', { name: 'Recent chats', exact: true });

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Try an example' }).click();
});

test('drags into a collapsed folder and back to Recent without changing the open conversation', async ({ page }) => {
  await page.getByRole('button', { name: 'Writing', exact: true }).click();
  await page.getByRole('button', { name: 'Reading notes', exact: true }).dragTo(page.getByRole('button', { name: 'Writing', exact: true }));
  await expect(folder(page, 'Writing').getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
  await expect(page.locator('.chat-title')).toHaveText('Interview prep');
  await folder(page, 'Writing').getByRole('button', { name: 'Reading notes', exact: true }).dragTo(recent(page).getByText('Recent', { exact: true }));
  await expect(recent(page).getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
  await expect(folder(page, 'Writing').getByRole('button', { name: 'Reading notes', exact: true })).toHaveCount(0);
});

test('spring opens a hovered folder and Escape abandons the drag', async ({ page }) => {
  await page.getByRole('button', { name: 'Writing', exact: true }).click();
  const source = (await page.getByRole('button', { name: 'Reading notes', exact: true }).boundingBox())!;
  const target = (await page.getByRole('button', { name: 'Writing', exact: true }).boundingBox())!;
  await page.mouse.move(source.x + 20, source.y + source.height / 2); await page.mouse.down();
  await page.mouse.move(target.x + 30, target.y + target.height / 2, { steps: 12 });
  await expect(page.locator('.chat-drag-preview')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Writing', exact: true })).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect(recent(page).getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
  await expect(page.locator('[data-drop-active="true"]')).toHaveCount(0);
  await expect(page.locator('.chat-drag-preview')).toBeHidden();
});

test('internal dragging never starts a system drag and releasing outside a destination cancels', async ({ page }) => {
  await page.evaluate(() => { document.addEventListener('dragstart', () => { document.body.dataset.systemDrag = 'true'; }); });
  const source = (await page.getByRole('button', { name: 'Reading notes', exact: true }).boundingBox())!;
  await page.mouse.move(source.x + 20, source.y + source.height / 2); await page.mouse.down();
  await page.mouse.move(700, 200, { steps: 10 });
  await expect(page.locator('.chat-drag-preview')).toBeVisible();
  await expect(page.locator('body')).not.toHaveAttribute('data-system-drag', 'true');
  await page.mouse.up();
  await expect(page.locator('.chat-drag-preview')).toBeHidden();
  await expect(recent(page).getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
  await expect(page.locator('.chat-title')).toHaveText('Interview prep');
});

test('edge scrolling stops when a drag loses window focus', async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 560 });
  const source = page.getByRole('button', { name: 'Reading notes', exact: true });
  await source.scrollIntoViewIfNeeded();
  const bounds = (await source.boundingBox())!;
  const list = page.locator('.sidebar-list');
  const region = (await list.boundingBox())!;
  const initial = await list.evaluate((element) => element.scrollTop);
  expect(initial).toBeGreaterThan(0);
  await page.mouse.move(bounds.x + 20, bounds.y + bounds.height / 2); await page.mouse.down();
  await page.mouse.move(region.x + 60, region.y + 5, { steps: 10 });
  await expect.poll(() => list.evaluate((element) => element.scrollTop)).toBeLessThan(initial);
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await expect(page.locator('.chat-drag-preview')).toBeHidden();
  await expect(page.locator('body')).not.toHaveClass(/is-chat-dragging/);
  const stopped = await list.evaluate((element) => element.scrollTop);
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  expect(await list.evaluate((element) => element.scrollTop)).toBe(stopped);
  await page.mouse.up();
  await expect(page.locator('.chat-title')).toHaveText('Interview prep');
});

test('rejects external drops even when they contain an existing chat ID', async ({ page }) => {
  const transfer = await page.evaluateHandle(async () => {
    const { mockBackend } = await import('/src/mock-backend.ts');
    const state = await mockBackend.invoke('bootstrap');
    const data = new DataTransfer();
    data.setData('application/x-melly-chat', state.chats.find((chat) => chat.title === 'Reading notes').id);
    data.setData('text/plain', 'external text');
    return data;
  });
  await page.getByRole('button', { name: 'Writing', exact: true }).dispatchEvent('drop', { dataTransfer: transfer });
  await expect(recent(page).getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
});

test('failed moves preserve the chat location and draft', async ({ page }) => {
  await page.getByRole('button', { name: 'Reading notes', exact: true }).click();
  await page.getByRole('textbox', { name: 'Message' }).fill('An unsent thought');
  await page.evaluate(async () => {
    const { mockBackend } = await import('/src/mock-backend.ts');
    const original = mockBackend.invoke.bind(mockBackend);
    mockBackend.invoke = async (command, args) => {
      if (command === 'move_chat') throw new Error('Move could not be saved.');
      return original(command, args);
    };
  });
  await page.getByRole('button', { name: 'Reading notes', exact: true }).dragTo(page.getByRole('button', { name: 'Writing', exact: true }));
  await expect(page.getByRole('alert')).toContainText('Move could not be saved');
  await expect(page.locator('.chat-location')).toHaveText('No folder');
  await expect(recent(page).getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Message' })).toHaveValue('An unsent thought');
});

test('deletes a background chat from its row without changing the selected chat or draft', async ({ page }) => {
  await page.getByRole('textbox', { name: 'Message' }).fill('Keep this draft');
  await page.getByRole('button', { name: 'Chat options for My experience', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Delete chat', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('My experience');
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(folder(page, 'Job search').getByRole('button', { name: 'My experience', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'My experience', exact: true }).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Delete chat', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete chat', exact: true }).click();
  await expect(folder(page, 'Job search').getByRole('button', { name: 'My experience', exact: true })).toHaveCount(0);
  await expect(page.locator('.chat-title')).toHaveText('Interview prep');
  await expect(page.getByRole('textbox', { name: 'Message' })).toHaveValue('Keep this draft');
  await page.getByRole('button', { name: 'Open source My experience', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('source chat is no longer available');
});

test('deletion failures remain in the confirmation and keep the chat', async ({ page }) => {
  await page.evaluate(async () => {
    const { mockBackend } = await import('/src/mock-backend.ts');
    const original = mockBackend.invoke.bind(mockBackend);
    mockBackend.invoke = async (command, args) => {
      if (command === 'delete_chat') throw new Error('Chat could not be deleted.');
      return original(command, args);
    };
  });
  await page.getByRole('button', { name: 'Chat options for Reading notes', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Delete chat', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Delete chat', exact: true }).click();
  await expect(page.getByRole('dialog').getByRole('alert')).toHaveText('Chat could not be deleted.');
  await expect(recent(page).getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
});

test('row actions contain only rename and delete, with keyboard movement in the header', async ({ page }) => {
  const trigger = page.getByRole('button', { name: 'Chat options for Reading notes', exact: true });
  await trigger.focus(); await page.keyboard.press('Enter');
  await expect(page.getByRole('menuitem')).toHaveText(['Rename chat', 'Delete chat']);
  await expect(page.getByRole('combobox', { name: 'Move to folder' })).toHaveCount(0);
  await expect(page.getByRole('menuitem', { name: 'Rename chat' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(trigger).toBeFocused();
  await page.getByRole('button', { name: 'Reading notes', exact: true }).click({ button: 'right' });
  await expect(page.getByRole('menuitem')).toHaveText(['Rename chat', 'Delete chat']);
  await expect(page.getByRole('combobox', { name: 'Move to folder' })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Reading notes', exact: true }).click();
  await page.getByRole('button', { name: 'Chat options', exact: true }).focus();
  await page.keyboard.press('Enter');
  await page.getByRole('combobox', { name: 'Move to folder' }).selectOption({ label: 'Writing' });
  await expect(folder(page, 'Writing').getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
});

test('a generating chat cannot be dragged, moved or deleted', async ({ page }) => {
  await page.getByRole('textbox', { name: 'Message' }).fill('Write a longer answer');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Stop response' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Interview prep', exact: true })).toHaveAttribute('draggable', 'false');
  await page.getByRole('button', { name: 'Chat options for Interview prep', exact: true }).click();
  await expect(page.getByRole('menuitem', { name: 'Delete chat', exact: true })).toBeDisabled();
  await expect(page.getByRole('combobox', { name: 'Move to folder' })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Chat options', exact: true }).click();
  await expect(page.getByRole('combobox', { name: 'Move to folder' })).toBeDisabled();
});

test('moving the selected chat keeps the draft but revokes its previous folder permission', async ({ page }) => {
  await page.getByRole('textbox', { name: 'Message' }).fill('Look in my other Job search chats for an example');
  await expect(page.getByRole('button', { name: 'Use other chats in Job search' })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Interview prep', exact: true }).dragTo(page.getByRole('button', { name: 'Writing', exact: true }));
  await expect(page.locator('.chat-location')).toHaveText('Writing');
  await expect(page.getByRole('textbox', { name: 'Message' })).toHaveValue('Look in my other Job search chats for an example');
  await expect(page.getByRole('button', { name: 'Use other chats in Writing' })).toHaveAttribute('aria-pressed', 'false');
});
