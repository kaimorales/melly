import { expect, test } from '@playwright/test';
import { packet, streamFixture } from './fixtures/stream';

test('overlapping packets stay smooth, Unicode-safe, bounded, and meet their deadlines', async ({page}) => {
  await page.goto('/');
  const result=await page.evaluate(async () => {
    const url='/src/stream-reveal.ts';
    const {advanceStream,queueStream,streamRevealWindow}=await import(url);
    const text='A👩🏽‍💻é🇫🇷 '+ 'Synthetic text. '.repeat(30);
    const schedule={base:0,bursts:[]};
    queueStream(schedule,text.length,0);
    let visible=0;
    const frames=[];
    for(let now=0;now<=128;now+=16){visible=advanceStream(text,visible,now,schedule);frames.push(text.slice(0,visible));}
    const boundaries=[0,...Array.from(new Intl.Segmenter(undefined,{granularity:'grapheme'}).segment(text),s=>s.index+s.segment.length)];
    const overlap={base:0,bursts:[]};
    queueStream(overlap,120,0);queueStream(overlap,120,60);
    const lengths=[60,90,120,150,180].map(now=>advanceStream('x'.repeat(240),0,now,overlap));
    const many={base:0,bursts:[]};
    for(let i=0;i<1000;i++)queueStream(many,1,i/10);
    return {text,frames,safe:frames.every(frame=>boundaries.includes(frame.length)),lengths,maxQueue:many.bursts.length,finished:advanceStream('x'.repeat(1000),0,100+streamRevealWindow,many)};
  });
  expect(result.safe).toBe(true);
  expect(new Set(result.frames).size).toBeGreaterThanOrEqual(7);
  expect(result.frames.at(-1)).toBe(result.text);
  expect(result.lengths).toEqual([60,120,180,210,240]);
  expect(result.maxQueue).toBeLessThanOrEqual(64);
  expect(result.finished).toBe(1000);
});

test('real answer frames progress, finish promptly, and stop scheduling when caught up', async ({page}, testInfo) => {
  await streamFixture(page, 8);
  const result=await page.evaluate(async () => {
    const win=window as unknown as {fixtureStream:(kind:string,text:string)=>void};
    const text='A smooth synthetic reply with 👩🏽‍💻 and é. '.repeat(40);
    const node=document.querySelector('#message-stream-answer')!;
    const start=performance.now();
    const frames:Array<{at:number;length:number}>=[];
    const observer=new MutationObserver(()=>{
      const length=node.querySelector('.markdown')?.textContent?.length||0;
      if(length && length!==frames.at(-1)?.length)frames.push({at:performance.now()-start,length});
    });
    observer.observe(node,{subtree:true,childList:true,characterData:true});
    const original=requestAnimationFrame;
    let scheduled=0;
    window.requestAnimationFrame=callback=>{scheduled++;return original(callback);};
    win.fixtureStream('delta',text);
    await new Promise(resolve=>setTimeout(resolve,250));
    const afterReveal=scheduled;
    await new Promise(resolve=>setTimeout(resolve,120));
    const idleFrames=scheduled-afterReveal;
    window.requestAnimationFrame=original;
    observer.disconnect();
    return {frames,idleFrames,final:node.querySelector('.markdown')?.textContent,text:text.trimEnd()};
  });
  await testInfo.attach('synthetic-stream-frames.json',{body:JSON.stringify(result.frames),contentType:'application/json'});
  expect(result.frames.length).toBeGreaterThanOrEqual(4);
  expect(result.frames[0].length).toBeLessThan(result.text.length);
  expect(result.final).toBe(result.text);
  expect(result.idleFrames).toBe(0);
  await packet(page,'done');
  await expect(page.locator('#message-stream-answer .message-tools')).toBeVisible();
});

for(const terminal of ['done','stopped','error']) {
  test(`${terminal} preserves the full received text without a reveal backlog`,async ({page})=>{
    await streamFixture(page);
    await page.evaluate(terminal=>{
      const emit=(window as unknown as {fixtureStream:(kind:string,text?:string)=>void}).fixtureStream;
      emit('delta','Synthetic terminal reply. '.repeat(80));
      emit(terminal);
    },terminal);
    await expect(page.locator('#message-stream-answer .markdown')).toHaveText('Synthetic terminal reply. '.repeat(80).trim());
    await expect(page.locator('#message-stream-answer .message-tools')).toBeVisible();
    if(terminal==='stopped')await expect(page.getByText('Response stopped',{exact:true})).toBeVisible();
    if(terminal==='error')await expect(page.getByText('Response failed',{exact:true})).toBeVisible();
  });
}

for(const mode of ['reduced','hidden']) {
  test(`${mode} output flushes without intermediate animation frames`,async({page})=>{
    if(mode==='reduced')await page.emulateMedia({reducedMotion:'reduce'});
    await streamFixture(page);
    const lengths=await page.evaluate(async mode=>{
      const node=document.querySelector('#message-stream-answer')!;
      const lengths:number[]=[];
      const observer=new MutationObserver(()=>{const n=node.querySelector('.markdown')?.textContent?.length||0;if(n&&n!==lengths.at(-1))lengths.push(n);});
      observer.observe(node,{subtree:true,childList:true,characterData:true});
      if(mode==='hidden')Object.defineProperty(document,'hidden',{configurable:true,value:true});
      (window as unknown as {fixtureStream:(kind:string,text:string)=>void}).fixtureStream('delta','Synthetic reduced-motion reply.');
      await new Promise(resolve=>setTimeout(resolve,80));
      observer.disconnect();
      if(mode==='hidden')delete (document as unknown as {hidden?:boolean}).hidden;
      return lengths;
    },mode);
    expect(lengths).toEqual(['Synthetic reduced-motion reply.'.length]);
  });
}
