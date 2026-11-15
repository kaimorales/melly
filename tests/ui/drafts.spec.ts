import { expect, test } from '@playwright/test';

test('unsent drafts stay with their chat, survive reload, and remain after a failed send', async ({ page }) => {
  await page.route('**/src/backend.ts*', async route => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = false;
      const chats = [{id:'first',title:'First draft',folderId:null,updatedAt:2},{id:'second',title:'Second draft',folderId:null,updatedAt:1}];
      const key = args => 'fixture-draft-' + (args.chatId ?? 'new');
      export const backend = {
        async invoke(command,args={}) {
          if(command==='bootstrap') return {...await mockBackend.invoke(command,args),chats};
          if(command==='messages') return [];
          if(command==='composer_draft') {
            await new Promise(resolve=>setTimeout(resolve,25));
            return JSON.parse(sessionStorage.getItem(key(args)) || '{"content":"","attachments":[],"editing":null}');
          }
          if(command==='save_composer_draft') {
            await new Promise(resolve=>setTimeout(resolve,25));
            sessionStorage.setItem(key(args),JSON.stringify({content:args.draft.content,attachments:[],editing:null}));return;
          }
          if(command==='start_chat') throw new Error('Synthetic server offline');
          return mockBackend.invoke(command,args);
        },listen:mockBackend.listen,
      };`
    });
  });
  await page.goto('/');
  const composer = page.getByRole('textbox', { name: 'Message', exact: true });
  await composer.fill('First unsent message');
  await page.getByRole('button', { name: 'Second draft', exact: true }).click();
  await expect(composer).toHaveValue('');
  await composer.fill('Second unsent message');
  await page.getByRole('button', { name: 'First draft', exact: true }).click();
  await expect(composer).toHaveValue('First unsent message');
  await expect.poll(() => page.evaluate(() => JSON.parse(sessionStorage.getItem('fixture-draft-second') || '{}').content)).toBe('Second unsent message');
  await page.reload();
  await expect(composer).toHaveValue('First unsent message');
  await page.getByRole('button',{name:'Send message'}).click();
  await expect(page.getByText('Synthetic server offline',{exact:true})).toBeVisible();
  await expect(composer).toHaveValue('First unsent message');
  await page.getByRole('button', { name: 'Second draft', exact: true }).click();
  await expect(composer).toHaveValue('Second unsent message');
});
