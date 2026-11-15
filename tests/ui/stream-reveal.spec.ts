import { expect, test } from '@playwright/test';

test('bursts reveal progressively without splitting an emoji and catch up by the deadline', async ({ page }) => {
  await page.goto('/');
  const frames = await page.evaluate(async () => {
    const url = '/src/stream-reveal.ts';
    const { advanceStream, streamRevealWindow } = await import(url);
    const text = 'A👩🏽‍💻 is writing a synthetic response for the stream test.';
    const deadline = streamRevealWindow;
    const first = advanceStream(text, 0, 0, deadline);
    const second = advanceStream(text, first, 32, deadline);
    return { text, first: text.slice(0, first), second: text.slice(0, second), finished: text.slice(0, advanceStream(text, second, deadline, deadline)), emoji: text.slice(0, advanceStream('A👩🏽‍💻', 0, 0, deadline)) };
  });
  expect(frames.first.length).toBeLessThan(frames.second.length);
  expect(frames.second.length).toBeLessThan(frames.text.length);
  expect(frames.emoji).toBe('A👩🏽‍💻');
  expect(frames.finished).toBe(frames.text);
});
