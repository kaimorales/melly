import { expect, test } from '@playwright/test';

for (const colorScheme of ['light','dark'] as const) {
  test(`sources and long change paths stay compact at a narrow width in ${colorScheme}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 480, height: 740 });
    await page.emulateMedia({ colorScheme });
    const path = `src/${'long_directory_'.repeat(18)}/feature.py`;
    await page.route('**/src/backend.ts*', async route => {
      await route.fulfill({ contentType:'application/javascript', body:`
        import { mockBackend } from './mock-backend.ts';
        export const nativeApp=true;
        const sources=[{id:'file:one',kind:'file',title:${JSON.stringify(path)},url:null,chatId:null,messageId:null,snippet:'Synthetic current file excerpt'},{id:'web:two',kind:'web',title:'Synthetic reference',url:'https://example.com',chatId:null,messageId:null,snippet:'Synthetic reference text'}];
        export const backend={
          async invoke(command,args={}){
            if(command==='bootstrap'){const data=await mockBackend.invoke(command,args);return {...data,chats:[{id:'fixture',title:'Polish fixture',folderId:null,updatedAt:1}]};}
            if(command==='messages')return [{id:'reply',chatId:'fixture',role:'assistant',content:'The edit was saved before the response stopped.',status:'interrupted',sources,createdAt:1}];
            if(command==='coding_activity')return {reply:[{action:'Edit',path:${JSON.stringify(path)},diff:[{kind:'+',text:'print(42)'}]}]};
            return mockBackend.invoke(command,args);
          },listen:mockBackend.listen
        };
      ` });
    });
    await page.goto('/');
    const hideSidebar=page.getByRole('button',{name:'Hide sidebar'});
    if(await hideSidebar.isVisible())await hideSidebar.click();
    await expect(page.getByText('Changes kept',{exact:true})).toBeVisible();
    const references=page.locator('.response-references');
    await expect(references.locator('summary')).toHaveText('2 sources');
    await expect(references.locator('.source-row')).toBeHidden();
    await expect(page.locator('.retrieval-line')).toHaveCount(0);
    await references.locator('summary').press('Enter');
    await expect(references.getByRole('button')).toHaveCount(2);
    expect(await page.locator('.assistant').evaluate(element=>element.scrollWidth<=element.clientWidth)).toBe(true);
    expect(await page.locator('body').evaluate(element=>element.scrollWidth<=window.innerWidth)).toBe(true);
    await page.screenshot({path:testInfo.outputPath('response.png')});
  });
}
