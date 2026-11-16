import { expect, test } from '@playwright/test';
import { packet, streamFixture } from './fixtures/stream';

for (const colorScheme of ['light','dark'] as const) {
  test(`new prose fades while older text stays steady in ${colorScheme}`, async ({page},testInfo) => {
    await page.emulateMedia({colorScheme});
    await streamFixture(page);
    await packet(page,'delta','The earlier sentence is already readable.\n\n');
    await expect(page.locator('.markdown')).toContainText('already readable.');
    await page.waitForTimeout(320);
    const samples=await page.evaluate(async()=>{
      const emit=(window as unknown as {fixtureStream:(kind:string,text:string)=>void}).fixtureStream;
      const old=document.querySelector('.markdown p')!;
      const initial=old.getBoundingClientRect();
      emit('delta','A quick reply should feel fluid, even when several words arrive together. '.repeat(8));
      const samples:Array<{opacity:number;count:number;oldOpacity:number;oldY:number;oldWidth:number}>=[];
      const until=performance.now()+240;
      await new Promise<void>(resolve=>{
        const frame=()=>{
          const spans=Array.from(document.querySelectorAll<HTMLElement>('.arriving-word'));
          samples.push({opacity:Math.min(1,...spans.map(span=>Number(getComputedStyle(span).opacity))),count:spans.length,oldOpacity:Number(getComputedStyle(old).opacity),oldY:old.getBoundingClientRect().top-initial.top,oldWidth:old.getBoundingClientRect().width-initial.width});
          if(performance.now()<until)requestAnimationFrame(frame);else resolve();
        };
        requestAnimationFrame(frame);
      });
      return samples;
    });
    expect(samples.some(sample=>sample.opacity>0 && sample.opacity<.85)).toBe(true);
    expect(samples.every(sample=>sample.count<=96)).toBe(true);
    expect(samples.every(sample=>sample.oldOpacity===1 && Math.abs(sample.oldWidth)<1)).toBe(true);
    await testInfo.attach('prose-opacity-frames.json',{body:JSON.stringify(samples),contentType:'application/json'});
    await packet(page,'done');
    await expect(page.locator('#message-stream-answer .message-tools')).toBeVisible();
    await expect(page.locator('.arriving-word')).toHaveCount(0);
  });
}

test('formatting changes do not fade old words again and code stays untouched',async({page})=>{
  await streamFixture(page);
  await packet(page,'delta','Already read this. **Start');
  await expect(page.locator('.markdown')).toContainText('Start');
  await page.waitForTimeout(320);
  await packet(page,'delta',' of the new bold text**\n\n```python\nprint("hello")\n```\n\nNew 👩🏽‍💻 é 中文 words.');
  await expect(page.locator('.markdown strong')).toContainText('new bold text');
  await expect(page.locator('.arriving-word').filter({hasText:/Already|read|this|Start/})).toHaveCount(0);
  await expect(page.locator('.code-block .arriving-word')).toHaveCount(0);
  await packet(page,'done');
  await expect(page.locator('.markdown')).toContainText('New 👩🏽‍💻 é 中文 words.');
  await expect(page.locator('.arriving-word')).toHaveCount(0);
});

test('Stop removes fades immediately and preserves all received text',async({page})=>{
  await streamFixture(page);
  await packet(page,'delta','Synthetic cancellation text. '.repeat(30));
  await expect.poll(()=>page.locator('.arriving-word').count()).toBeGreaterThan(0);
  await packet(page,'stopped');
  await expect(page.getByText('Response stopped',{exact:true})).toBeVisible();
  await expect(page.locator('.arriving-word')).toHaveCount(0);
  await expect(page.locator('.markdown')).toHaveText('Synthetic cancellation text. '.repeat(30).trim());
});

test('reduced motion uses ordinary text without a word animation layer',async({page})=>{
  await page.emulateMedia({reducedMotion:'reduce'});
  await streamFixture(page);
  await packet(page,'delta','A reduced-motion response arrives directly.');
  await expect(page.locator('.markdown')).toHaveText('A reduced-motion response arrives directly.');
  await expect(page.locator('.arriving-word')).toHaveCount(0);
});
