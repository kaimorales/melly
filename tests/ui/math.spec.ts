import { expect, test, type Page } from '@playwright/test';

async function showMath(page: Page, content: string) {
  await page.goto('/');
  await page.evaluate(async (markdown) => {
    const { mockBackend } = await import('/src/mock-backend.ts');
    const invoke = mockBackend.invoke.bind(mockBackend);
    mockBackend.invoke = async (command, args) => {
      const result = await invoke(command, args);
      if (command === 'messages') for (const message of result) if (message.role === 'assistant') { message.content = markdown; message.sources = []; }
      return result;
    };
  }, content);
  await page.getByRole('button', { name: 'Try an example' }).click();
}

test('renders inline and display LaTeX with local fonts and readable themes', async ({ page }) => {
  const external: string[] = [];
  page.on('request', (request) => { if (!request.url().startsWith('http://127.0.0.1:1420')) external.push(request.url()); });
  await showMath(page, String.raw`The identity $e^{i\pi}+1=0$ is compact.

$$
\int_0^1 x^2\,dx=\frac{1}{3}
$$

Also \( a^2+b^2=c^2 \), with a matrix:

\[\begin{bmatrix}1 & 2 \\ 3 & 4\end{bmatrix}\]`);
  await expect(page.locator('.math-inline .katex')).toHaveCount(2);
  await expect(page.locator('.math-block .katex')).toHaveCount(2);
  await expect(page.locator('.math-expression math')).toHaveCount(4);
  await expect(page.locator('.code-block')).toHaveCount(0);
  await page.evaluate(() => document.fonts.ready);
  expect(await page.locator('.katex').first().evaluate((node) => document.fonts.check(`${getComputedStyle(node).fontSize} KaTeX_Main`))).toBe(true);
  const dark = await page.locator('.math-expression').first().evaluate((node) => getComputedStyle(node).color);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await page.getByRole('button', { name: 'Back to chat' }).click();
  await expect(page.locator('.math-expression .katex')).toHaveCount(4);
  expect(await page.locator('.math-expression').first().evaluate((node) => getComputedStyle(node).color)).not.toBe(dark);
  expect(external).toEqual([]);
});

test('keeps code, escaped dollars, ordinary prices and unfinished equations literal', async ({ page }) => {
  const code = String.raw`$x^2$ and \(x+y\) and \[z\]`;
  await showMath(page, `Costs $5 or $10, and \\$20 is escaped. Unfinished $x+1.\n\nInline \`${code}\`.\n\n\`\`\`latex\n${code}\n\`\`\`\n\n    ${code}`);
  await expect(page.locator('.math-expression')).toHaveCount(0);
  await expect(page.locator('.markdown')).toContainText('Costs $5 or $10, and $20 is escaped. Unfinished $x+1.');
  await expect(page.locator('.markdown > p > code')).toHaveText(code);
  await expect(page.locator('.code-block code')).toHaveText([`${code}\n`, `${code}\n`]);
  expect(page.workers()).toHaveLength(0);
});

test('rejects unsafe commands and malformed or expensive math without exposing HTML', async ({ page }) => {
  const external: string[] = [];
  const errors: string[] = [];
  page.on('request', (request) => { if (request.url().startsWith('https://example.com')) external.push(request.url()); });
  page.on('console', (message) => { if (['error', 'warning'].includes(message.type())) errors.push(message.text()); });
  const invalid = String.raw`\notARealCommand{<img src=x onerror=alert(1)>}`;
  const loop = String.raw`\def\a{\a}\a`;
  await showMath(page, String.raw`$\href{https://example.com/no-navigation}{link}$ $\includegraphics{https://example.com/no-image}$ $\htmlStyle{position:fixed}{bad}$` + `\n\n$${invalid}$\n\n$${loop}$\n\nA valid equation after those: $x+1$`);
  await expect(page.locator('.math-expression')).toHaveCount(6);
  await expect(page.locator('.math-expression').last().locator('.katex')).toBeVisible();
  await expect(page.locator('.math-expression a, .math-expression img, .math-expression script')).toHaveCount(0);
  await expect(page.locator('.math-source').filter({ hasText: invalid })).toBeVisible();
  await expect(page.locator('.math-source').filter({ hasText: loop })).toBeVisible();
  await page.getByRole('textbox', { name: 'Message' }).fill('Still responsive');
  await expect(page.getByRole('textbox', { name: 'Message' })).toHaveValue('Still responsive');
  expect(external).toEqual([]); expect(errors).toEqual([]);
});

test('isolates equation macros and copies the original response exactly', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async (value: string) => { window.__copiedMath = value; } } });
  });
  const content = String.raw`$\gdef\mellymacro{x}\mellymacro$

$\mellymacro$

\(\frac{1}{2}\)`;
  await showMath(page, content);
  await expect(page.locator('.math-expression .katex')).toHaveCount(2);
  await expect(page.locator('.math-source')).toHaveText(String.raw`\mellymacro`);
  await page.getByRole('button', { name: 'Copy response' }).click();
  await expect.poll(() => page.evaluate(() => window.__copiedMath)).toBe(content);
});

test('bounds oversized equations and recovers when the math worker cannot start', async ({ page }) => {
  const large = 'x+'.repeat(5000) + '1';
  await showMath(page, `$$\n${large}\n$$`);
  await expect(page.locator('.math-source')).toHaveText(large);
  expect(page.workers()).toHaveLength(0);
  await page.addInitScript(() => { window.Worker = class { constructor() { throw new Error('Worker unavailable'); } } as typeof Worker; });
  await showMath(page, String.raw`$\frac{1}{2}$`);
  await expect(page.locator('.math-source')).toHaveText(String.raw`\frac{1}{2}`);
  await page.getByRole('textbox', { name: 'Message' }).fill('Continue');
  await expect(page.getByRole('textbox', { name: 'Message' })).toHaveValue('Continue');
});

test('streams equations and preserves responsive Stop', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'Message' }).fill('Demonstrate LaTeX in a longer answer');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(page.locator('.math-expression .katex').first()).toBeVisible();
  await page.getByRole('button', { name: 'Stop response' }).click();
  await expect(page.getByText('Response stopped', { exact: true })).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Message' })).toBeEditable();
});

test('keeps an unfinished display equation readable and scrolls wide equations within the transcript', async ({ page }) => {
  await showMath(page, '$$\n\\frac{1}{2}');
  await expect(page.locator('.math-source')).toHaveText(String.raw`\frac{1}{2}`);
  await expect(page.locator('.katex')).toHaveCount(0);
  expect(page.workers()).toHaveLength(0);
  await page.setViewportSize({ width: 760, height: 700 });
  await showMath(page, `$$\n${'x+'.repeat(100)}1\n$$`);
  await expect(page.locator('.math-block .katex')).toBeVisible();
  const bounds = await page.locator('.math-block').evaluate((node) => ({ width: node.clientWidth, content: node.scrollWidth, end: node.getBoundingClientRect().right, viewport: innerWidth }));
  expect(bounds.content).toBeGreaterThan(bounds.width);
  expect(bounds.end).toBeLessThanOrEqual(bounds.viewport);
  await page.locator('.math-block').evaluate((node) => { node.scrollLeft = 100; });
  expect(await page.locator('.math-block').evaluate((node) => node.scrollLeft)).toBeGreaterThan(0);
  await expect(page.getByRole('textbox', { name: 'Message' })).toBeEditable();
});
