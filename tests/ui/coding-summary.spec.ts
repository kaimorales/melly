import { expect, test, type Page } from '@playwright/test';

async function showResults(page: Page, results: unknown[]) {
  await page.route('**/src/backend.ts*', route => route.fulfill({contentType:'application/javascript', body:`
    import {mockBackend} from './mock-backend.ts';
    export const nativeApp=true;
    export const backend={async invoke(command,args={}) {
      if(command==='bootstrap') {const data=await mockBackend.invoke(command,args);return {...data,chats:[{id:'fixture',title:'Coding summary',folderId:null,updatedAt:1}]};}
      if(command==='messages') return [{id:'answer',chatId:'fixture',role:'assistant',content:'The change is ready.',status:'complete',sources:[],createdAt:1}];
      if(command==='coding_activity') return {answer:${JSON.stringify(results)}};
      if(command==='file_changes') return [{id:'change',path:'src/example.py',state:'applied'}];
      return mockBackend.invoke(command,args);
    },listen:mockBackend.listen};
  `}));
  await page.goto('/');
}
const edit = {action:'Edit',path:'src/example.py',diff:[{kind:'-',text:'return 1'},{kind:'+',text:'return 2'}]};
const check = {command:'python3 -m unittest',label:'Tests',passed:true,no_tests_found:false,exit_code:0,stdout:'Synthetic check output',stderr:''};

for (const colorScheme of ['light','dark'] as const) {
  test(`completed work counts files once and keeps the full check history in ${colorScheme}`, async ({page}, testInfo) => {
    await page.setViewportSize({width:760,height:740});
    await page.emulateMedia({colorScheme});
    await showResults(page,[edit,{...check,passed:false,exit_code:1},edit,{...edit,path:'src/other.py'},check]);
    const summary=page.locator('.completed-work > summary');
    await expect(summary).toHaveText('2 files changed · Checks passed');
    await expect(page.getByText('Tests failed',{exact:true})).toBeHidden();
    await expect(page.getByRole('button',{name:'Undo changes…'})).toBeVisible();
    await page.screenshot({path:testInfo.outputPath('completed-work.png')});
    await summary.press('Enter');
    await expect(page.getByText('Tests failed',{exact:true})).toBeVisible();
    await expect(page.getByText('Tests passed',{exact:true})).toBeVisible();
    await page.locator('.file-edit-saved').first().locator('summary').press('Enter');
    await expect(page.getByLabel('File diff').first()).toContainText('return 2');
  });
}

test('unresolved failures and missing tests stay visible', async ({page}) => {
  await showResults(page,[edit,{...check,passed:false,exit_code:1}]);
  await expect(page.getByText('Tests failed',{exact:true})).toBeVisible();
  await expect(page.locator('.completed-work')).toHaveCount(0);
  await page.unroute('**/src/backend.ts*');
  await showResults(page,[edit,{...check,no_tests_found:true}]);
  await expect(page.getByText('No tests found',{exact:true})).toBeVisible();
  await expect(page.locator('.completed-work')).toHaveCount(0);
});

test('reused operations are not presented as new changes', async ({page}) => {
  await showResults(page,[{...edit,reused:true}]);
  await expect(page.locator('.completed-work > summary')).toHaveText('Changes already applied');
});
