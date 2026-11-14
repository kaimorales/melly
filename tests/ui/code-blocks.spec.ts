import { expect, test, type Page } from '@playwright/test';

async function showCode(page: Page, content: string) {
  await page.goto('/');
  await page.evaluate(async (markdown) => {
    const { mockBackend } = await import('/src/mock-backend.ts');
    const original = mockBackend.invoke.bind(mockBackend);
    mockBackend.invoke = async (command, args) => {
      const result = await original(command, args);
      if (command === 'messages') for (const message of result) if (message.role === 'assistant') { message.content = markdown; message.sources = []; }
      return result;
    };
  }, content);
  await page.getByRole('button', { name: 'Try an example' }).click();
}

const python = '# A small greeting\ndef greet(name: str):\n    count = 42\n    return "Hello, " + name\n';

test('fenced code has readable syntax colors in dark and light themes', async ({ page }) => {
  await showCode(page, `\`\`\`python\n${python}\`\`\``);
  const block = page.getByRole('region', { name: 'Python code' });
  await expect(block.locator('.hljs-keyword').first()).toHaveText('def');
  await expect(block.locator('.hljs-string')).toHaveText('"Hello, "');
  await expect(block.getByRole('button', { name: 'Copy code' })).toBeVisible();
  const contrast = async () => block.evaluate((element) => {
    const rgb = (value: string) => value.match(/[\d.]+/g)!.slice(0, 3).map(Number).map((x) => { const c = x / 255; return c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4; });
    const luminance = (value: string) => { const [r, g, b] = rgb(value); return .2126 * r + .7152 * g + .0722 * b; };
    const background = luminance(getComputedStyle(element).backgroundColor);
    return [...element.querySelectorAll('code span[class]')].map((token) => {
      const foreground = luminance(getComputedStyle(token).color);
      return (Math.max(background, foreground) + .05) / (Math.min(background, foreground) + .05);
    });
  });
  expect(Math.min(...await contrast())).toBeGreaterThanOrEqual(4.5);
  const darkColor = await block.locator('.hljs-keyword').first().evaluate((node) => getComputedStyle(node).color);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await page.getByRole('button', { name: 'Back to chat' }).click();
  await expect(block.locator('.hljs-keyword').first()).toBeVisible();
  expect(await block.locator('.hljs-keyword').first().evaluate((node) => getComputedStyle(node).color)).not.toBe(darkColor);
  expect(Math.min(...await contrast())).toBeGreaterThanOrEqual(4.5);
});

test('language aliases are highlighted and copying preserves code exactly', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async (text: string) => { window.__copiedCode = text; } } });
  });
  await showCode(page, '```js\nconst answer = "hello";\n```\n\n```rs\nfn main() { let count = 42; }\n```');
  await expect(page.getByRole('region', { name: 'JavaScript code' }).locator('.hljs-keyword')).toHaveText('const');
  await expect(page.getByRole('region', { name: 'Rust code' }).locator('.hljs-number')).toHaveText('42');
  await page.getByRole('region', { name: 'JavaScript code' }).getByRole('button', { name: 'Copy code' }).click();
  await expect.poll(() => page.evaluate(() => window.__copiedCode)).toBe('const answer = "hello";\n');
  await expect(page.getByRole('button', { name: 'Code copied' })).toBeVisible();
});

test('unknown languages and oversized blocks remain intact without syntax work', async ({ page }) => {
  const large = 'let answer = 42;\n'.repeat(2200);
  await showCode(page, `\`\`\`futurelang\nkeep <this> & exactly\n\`\`\`\n\n\`\`\`js\n${large}\`\`\``);
  const blocks = page.locator('.code-block');
  await expect(blocks).toHaveCount(2);
  await expect(blocks.first().locator('code')).toHaveText('keep <this> & exactly\n');
  await expect(blocks.last().locator('code')).toHaveText(large);
  await expect(blocks.locator('code span')).toHaveCount(0);
  expect(page.workers()).toHaveLength(0);
  await expect(page.getByRole('textbox', { name: 'Message' })).toBeEditable();
});

test('highlighted HTML remains inert text and inline code stays inline', async ({ page }) => {
  const html = '<script>window.__codeRan = true</script>\n<img src="https://example.com/no-request" onerror="alert(1)">\n';
  await showCode(page, `Inline \`name\` remains inline.\n\n\`\`\`html\n${html}\`\`\``);
  const block = page.getByRole('region', { name: 'HTML code' });
  await expect(block.locator('.hljs-name').first()).toBeVisible();
  await expect(block.locator('code')).toHaveText(html);
  await expect(block.locator('script, img')).toHaveCount(0);
  expect(await page.evaluate(() => window.__codeRan)).toBeUndefined();
  await expect(page.locator('.markdown > p > code')).toHaveText('name');
});

test('streamed code receives syntax colors and Stop stays responsive', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'Message' }).fill('Show Python code in a longer answer');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(page.locator('.code-block .hljs-keyword').first()).toBeVisible();
  await page.getByRole('button', { name: 'Stop response' }).click();
  await expect(page.getByText('Response stopped', { exact: true })).toBeVisible();
  await expect(page.locator('.code-block code')).toContainText('def summarize');
});

test('an unavailable syntax worker preserves readable code and an editable composer', async ({ page }) => {
  await page.addInitScript(() => { window.Worker = class { constructor() { throw new Error('Worker unavailable'); } } as typeof Worker; });
  await showCode(page, `\`\`\`python\n${python}\`\`\``);
  await expect(page.getByRole('region', { name: 'Python code' }).locator('code')).toHaveText(python);
  await page.getByRole('textbox', { name: 'Message' }).fill('The app still responds');
  await expect(page.getByRole('textbox', { name: 'Message' })).toHaveValue('The app still responds');
});
