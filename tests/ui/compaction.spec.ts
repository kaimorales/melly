import { expect, test } from '@playwright/test';

test('context notes stay in the ring, can be undone, and leave the draft intact', async ({ page }) => {
  await page.route('**/src/backend.ts*', async route => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = false;
      let summary = null;
      window.__compactionRequest = null;
      export const backend = {
        async invoke(command,args={}) {
          if(command==='context_summary') return summary;
          if(command==='compact_context') {
            window.__compactionRequest = args;
            await new Promise(resolve=>setTimeout(resolve,40));
            summary = {text:'Goal: preserve the synthetic task. Pending: verify the next edit.',through:1,messages:8};
            return summary;
          }
          if(command==='clear_context_summary') {summary=null;return;}
          return mockBackend.invoke(command,args);
        },listen:mockBackend.listen,
      };`
    });
  });
  await page.goto('/');
  const input = page.getByRole('textbox',{name:'Message',exact:true});
  await input.fill('Synthetic context history');
  await page.getByRole('button',{name:'Send message'}).click();
  await expect(page.getByRole('button',{name:'Retry response'})).toBeVisible();
  await input.fill('Keep this unsent draft');
  await page.getByRole('button',{name:'Context and tokens'}).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button',{name:'Compact context',exact:true}).click();
  await dialog.getByText('Task notes',{exact:true}).click();
  await expect(dialog.getByText('Goal: preserve the synthetic task. Pending: verify the next edit.',{exact:true})).toBeVisible();
  expect(await page.evaluate(()=>(window as unknown as {__compactionRequest:{providerId:string}}).__compactionRequest.providerId)).toBe('mock');
  await dialog.getByRole('button',{name:'Undo compaction'}).click();
  await expect(dialog.getByText('Task notes',{exact:true})).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(input).toHaveValue('Keep this unsent draft');
  await expect(page.locator('.user-message')).toContainText('Synthetic context history');
});
