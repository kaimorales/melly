import { expect, test } from '@playwright/test';

test('undo stays inline, keeps conflicts visible, and restores a saved result', async ({ page }) => {
  await page.route('**/src/backend.ts*', async route => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = true;
      let state = 'applied';
      let conflict = true;
      export const backend = {
        async invoke(command, args = {}) {
          if (command === 'bootstrap') {
            const data = await mockBackend.invoke(command,args);
            return {...data,chats:[{id:'fixture',title:'Undo fixture',folderId:null,updatedAt:1}]};
          }
          if (command === 'messages') return [{id:'reply',chatId:'fixture',role:'assistant',content:'Updated the file.',status:'complete',sources:[],createdAt:1}];
          if (command === 'coding_activity') return {};
          if (command === 'file_changes') return [{id:'change',path:'src/example.py',state}];
          if (command === 'undo_file_changes') {
            if (conflict) {conflict=false;throw new Error('File changed after Melly edited it. Your current file was left unchanged.');}
            state='undone';return;
          }
          return mockBackend.invoke(command,args);
        },listen:mockBackend.listen,
      };`
    });
  });
  await page.goto('/');
  await page.getByRole('button',{name:'Undo changes…',exact:true}).click();
  const review = page.getByRole('region',{name:'Undo file changes'});
  await expect(review.getByText('src/example.py')).toBeVisible();
  await review.getByRole('button',{name:'Undo change to src/example.py'}).click();
  await expect(review.getByRole('alert')).toContainText('left unchanged');
  await review.getByRole('button',{name:'Undo change to src/example.py'}).click();
  await expect(page.getByText('Changes undone',{exact:true})).toBeVisible();
  await expect(review).toHaveCount(0);
});
