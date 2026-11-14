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
  const transfer = await page.evaluateHandle(() => new DataTransfer());
  await page.getByRole('button', { name: 'Reading notes', exact: true }).dispatchEvent('dragstart', { dataTransfer: transfer });
  await page.getByRole('button', { name: 'Writing', exact: true }).dispatchEvent('dragover', { dataTransfer: transfer });
  await expect(page.getByRole('button', { name: 'Writing', exact: true })).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Writing', exact: true }).dispatchEvent('drop', { dataTransfer: transfer });
  await expect(recent(page).getByRole('button', { name: 'Reading notes', exact: true })).toBeVisible();
  await expect(page.locator('[data-drop-active="true"]')).toHaveCount(0);
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

test('row menu has a keyboard move alternative and restores focus on Escape', async ({ page }) => {
  const trigger = page.getByRole('button', { name: 'Chat options for Reading notes', exact: true });
  await trigger.focus(); await page.keyboard.press('Enter');
  await expect(page.getByRole('menuitem', { name: 'Rename chat' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(trigger).toBeFocused();
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
