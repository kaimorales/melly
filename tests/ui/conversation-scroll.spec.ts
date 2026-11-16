import { expect, test } from '@playwright/test';
import { packet, streamFixture } from './fixtures/stream';

const longReply = Array.from({length:20},(_,i)=>`New paragraph ${i}. ${'Synthetic streamed text. '.repeat(15)}`).join('\n\n');

test('follows new text, protects a reading position, and resumes from the return control', async ({page}) => {
  await streamFixture(page, 12);
  const scroll=page.locator('.conversation-scroll');
  const gap=()=>scroll.evaluate(node=>node.scrollHeight-node.clientHeight-node.scrollTop);
  await expect.poll(gap).toBeLessThan(2);
  await packet(page,'delta',longReply);
  await expect(page.locator('#message-stream-answer')).toContainText('New paragraph 19.');
  await expect.poll(gap).toBeLessThan(2);
  await scroll.hover();
  await page.mouse.wheel(0,-900);
  await expect(page.getByRole('button',{name:'Jump to latest'})).toBeVisible();
  const top=await scroll.evaluate(node=>node.scrollTop);
  await packet(page,'delta','\n\n'+longReply);
  await expect(page.locator('#message-stream-answer')).toContainText(longReply.slice(-80));
  await page.waitForTimeout(200); // Reveal window has elapsed; the reader must still be anchored.
  expect(await scroll.evaluate(node=>node.scrollTop)).toBeCloseTo(top,0);
  await page.getByRole('button',{name:'Jump to latest'}).click();
  await expect.poll(gap).toBeLessThan(2);
  await expect(page.getByRole('button',{name:'Jump to latest'})).toBeHidden();
  await packet(page,'delta','\n\n'+longReply);
  await expect.poll(gap).toBeLessThan(2);
  await expect(page.getByRole('textbox',{name:'Message'})).toBeFocused();
});

test('opening details keeps the disclosure in place, including with the keyboard', async ({page}) => {
  await streamFixture(page, 4);
  const summary=page.locator('#message-history-3 .response-references summary');
  await summary.scrollIntoViewIfNeeded();
  await summary.focus();
  const before=await summary.evaluate(node=>node.getBoundingClientRect().top);
  await summary.press('Enter');
  await expect(page.locator('#message-history-3 .source-row')).toBeVisible();
  expect(await summary.evaluate(node=>node.getBoundingClientRect().top)).toBeCloseTo(before,0);
  await packet(page,'delta',longReply);
  await expect(page.locator('#message-stream-answer')).toContainText('New paragraph 19.');
  expect(await summary.evaluate(node=>node.getBoundingClientRect().top)).toBeCloseTo(before,0);
  await expect(page.getByRole('button',{name:'Jump to latest'})).toBeVisible();
});


test('the same citation can be reopened after scrolling away from its highlight', async ({page}) => {
  await streamFixture(page,12,true);
  await packet(page,'delta','See [the earlier paragraph](melly-source:jump).');
  const source=page.getByRole('button',{name:'Open source Earlier paragraph',exact:true}).first();
  const target=page.locator('#message-history-0');
  for(let visit=0;visit<2;visit++) {
    await page.locator('.conversation-scroll').evaluate(node=>{node.scrollTop=node.scrollHeight;});
    await source.click();
    await expect(target).toBeInViewport();
    await expect(target).toHaveClass(/highlighted/);
  }
});
