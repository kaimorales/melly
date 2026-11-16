import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'New project' }).click();
  await page.getByRole('textbox', { name: 'Name' }).fill('Research');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await page.getByRole('button', { name: 'Project options for Research' }).click();
  await page.getByRole('menuitem', { name: 'New chat in project' }).click();
});

test('project context is remembered across chats and project files disclose native operations', async ({ page }) => {
  const context = page.getByRole('button', { name: 'Project context', exact: true });
  await expect(context).toHaveAttribute('aria-pressed', 'false');
  await context.click();
  await expect(context).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Project options for Research' }).click();
  await page.getByRole('menuitem', { name: 'New chat in project' }).click();
  await expect(context).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Project options for Research' }).click();
  await page.getByRole('menuitem', { name: 'Project files' }).click();
  await expect(page.getByRole('dialog')).toContainText('Linked folder');
  await page.getByRole('button', { name: 'Add file', exact: true }).click();
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('desktop app');
});

test('archive restores existing chats and delete requires explicit confirmation', async ({ page }) => {
  await page.getByRole('button', { name: 'Project options for Research' }).click();
  await page.getByRole('menuitem', { name: 'Archive project' }).click();
  await expect(page.getByRole('button', { name: 'Project options for Research' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Archived projects', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('Research');
  await page.getByRole('button', { name: 'Restore', exact: true }).click();
  await page.getByRole('button', { name: 'Close dialog' }).click();
  await page.getByRole('button', { name: 'Project options for Research' }).click();
  await page.getByRole('menuitem', { name: 'Delete project…' }).click();
  await expect(page.getByRole('dialog')).toContainText('Chat deletion is permanent');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Project options for Research' })).toBeVisible();
  await page.getByRole('button', { name: 'Project options for Research' }).click();
  await page.getByRole('menuitem', { name: 'Delete project…' }).click();
  await page.getByRole('button', { name: 'Delete project', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Project options for Research' })).toHaveCount(0);
});

test('local provider setup has no credential field and explains the external runtime', async ({ page }) => {
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByText('Browse providers', { exact: true }).click();
  await page.getByRole('button', { name: 'Configure Ollama Local' }).click();
  await expect(page.getByRole('dialog')).toContainText('127.0.0.1');
  await expect(page.getByRole('dialog').getByRole('button', { name: 'API key', exact: true })).toHaveCount(0);
});
