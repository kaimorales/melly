import { expect, test } from '@playwright/test';

test('New Chat keeps an open empty chat and its draft', async ({ page }) => {
  await page.goto('/');
  const newChat = page.getByRole('button', { name: 'New Chat', exact: true }).first();
  await newChat.press('Enter');
  const recent = page.getByRole('group', { name: 'Recent chats' });
  const blank = recent.getByRole('button', { name: 'New chat', exact: true });
  await expect(blank).toHaveCount(1);
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill('An unsent draft');
  await newChat.press('Enter');
  await expect(blank).toHaveCount(1);
  await expect(page.getByRole('textbox', { name: 'Message', exact: true })).toHaveValue('An unsent draft');
});
