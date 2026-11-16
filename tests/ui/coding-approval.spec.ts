import { expect, test } from '@playwright/test';

test('a coding action pauses for inline approval and shows its result', async ({ page }) => {
  await page.route('**/src/backend.ts*', async route => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = true;
      let request;
      let response;
      let stream;
      let result = {};
      window.__approval = null;
      export const backend = {
        async invoke(command, args = {}) {
          if (command === 'bootstrap') {
            const state = await mockBackend.invoke(command, args);
            const provider = state.providers.find(item => item.id === 'muse-api');
            const folder = { id: 'linked', title: 'Fixture project', contextEnabled: true };
            const chat = { id: 'fixture', title: 'Fixture chat', folderId: folder.id, updatedAt: 1 };
            return { ...state, folders: [folder], chats: [chat], providers: [{ ...provider, configured: true }], settings: { ...state.settings, defaultProvider: provider.id, defaultModel: provider.connection.modelId } };
          }
          if (command === 'file_edit_mode') return false;
          if (command === 'project_link') return { folderId: 'linked', name: 'Fixture project' };
          if (command === 'coding_activity') return result;
          if (command === 'start_chat') {
            request = args.input;
            response = { id: 'answer', chatId: request.chatId, role: 'assistant', content: '', status: 'streaming', createdAt: 2, sources: [] };
            setTimeout(() => stream?.({ chatId: request.chatId, messageId: response.id, kind: 'coding-approval',
              text: JSON.stringify({ id: 'pending-tool', label: 'Create', path: 'example.py', diff: [{ kind: '+', text: 'print(42)' }] }), sources: [] }), 60);
            return response;
          }
          if (command === 'messages' && args.chatId === 'fixture') {
            return request ? [{ id: 'question', chatId: request.chatId, role: 'user', status: 'complete', content: request.content, createdAt: 1, sources: [] }, response] : [];
          }
          if (command === 'confirm_coding_action') {
            window.__approval = args;
            response = { ...response, status: 'complete', content: 'Saved the file.' };
            result = { answer: [{ action: 'Create', path: 'example.py', diff: [{ kind: '+', text: 'print(42)' }] }] };
            stream?.({ chatId: request.chatId, messageId: response.id, kind: 'coding-approval-closed', text: 'pending-tool', sources: [] });
            stream?.({ chatId: request.chatId, messageId: response.id, kind: 'coding-result', text: JSON.stringify(result.answer[0]), sources: [] });
            stream?.({ chatId: request.chatId, messageId: response.id, kind: 'done', text: '', sources: [] });
            return;
          }
          return mockBackend.invoke(command, args);
        },
        listen: (event, callback) => { if (event === 'chat-stream') stream = callback; return mockBackend.listen(event, callback); },
      };`
    });
  });
  await page.goto('/');
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill('Create a test file.');
  await page.getByRole('button', { name: 'Send message' }).press('Enter');
  const approval = page.getByRole('region', { name: 'Permission requested' });
  await expect(approval.getByText('example.py')).toBeVisible();
  await expect(approval.getByText('print(42)', { exact: false })).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __approval: unknown }).__approval)).toBeNull();
  await approval.getByRole('button', { name: 'Allow' }).press('Enter');
  await expect(approval).toHaveCount(0);
  await expect(page.locator('.completed-work > summary')).toHaveText('1 file changed');
  await page.locator('.completed-work > summary').press('Enter');
  await expect(page.locator('summary').getByText('example.py')).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __approval: unknown }).__approval)).toEqual({ id: 'pending-tool', approved: true });
});

test('an interrupted coding reply offers Continue and preserves its previous result', async ({ page }) => {
  await page.route('**/src/backend.ts*', async route => {
    await route.fulfill({ contentType: 'application/javascript', body: `
      import { mockBackend } from './mock-backend.ts';
      export const nativeApp = true;
      let continued = false;
      let stream;
      const prior = {id:'prior',chatId:'fixture',role:'assistant',content:'Saved the synthetic file.',status:'interrupted',createdAt:2,sources:[]};
      export const backend = {
        async invoke(command,args={}) {
          if(command==='bootstrap') {const data=await mockBackend.invoke(command,args);const provider=data.providers.find(p=>p.id==='muse-api');return {...data,providers:[{...provider,configured:true}],settings:{...data.settings,defaultProvider:provider.id,defaultModel:provider.connection.modelId},chats:[{id:'fixture',title:'Continuation fixture',folderId:null,updatedAt:1}]};}
          if(command==='messages') return continued ? [prior,{...prior,id:'continued',content:'Continued the task.',status:'complete',createdAt:4}] : [prior];
          if(command==='coding_activity') return {prior:[{action:'Create',path:'example.py',diff:[{kind:'+',text:'print(42)'}]}]};
          if(command==='start_chat') {
            if(!args.input.retry) throw new Error('Expected continuation');
            continued=true;
            const reply={...prior,id:'continued',content:'',status:'streaming',createdAt:4};
            setTimeout(()=>stream?.({chatId:'fixture',messageId:'continued',kind:'done',text:'',sources:[]}),30);
            return reply;
          }
          return mockBackend.invoke(command,args);
        },listen:(event,callback)=>{if(event==='chat-stream')stream=callback;return mockBackend.listen(event,callback);}
      };`
    });
  });
  await page.goto('/');
  await page.getByRole('button',{name:'Continue task',exact:true}).click();
  await expect(page.getByText('Continued the task.',{exact:true})).toBeVisible();
  await expect(page.getByText('Saved the synthetic file.',{exact:true})).toBeVisible();
  await expect(page.locator('summary').getByText('example.py')).toBeVisible();
});
