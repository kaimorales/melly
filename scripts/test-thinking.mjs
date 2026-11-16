// Socket-free markup regressions; these do not replace browser interaction or visual QA.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createRequire } from 'node:module';
import { build } from 'esbuild';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

const directory = resolve('target/thinking-verification');
await mkdir(directory, { recursive: true });
const output = resolve(directory, 'Thinking.cjs');
await build({ entryPoints: ['src/Thinking.tsx'], outfile: output, bundle: true, platform: 'node', format: 'cjs', jsx: 'automatic', external: ['react', 'react/jsx-runtime'], loader: { '.css': 'empty' }, logLevel: 'silent' });
const { Thinking } = createRequire(import.meta.url)(output);
const base = { id: 'synthetic', chatId: 'synthetic-chat', role: 'assistant', content: '', reasoning: '', status: 'streaming', sources: [], createdAt: 0, providerId: 'ollama-local', modelId: 'synthetic', billingRoute: 'Local' };
const render = (message, activity = '') => renderToStaticMarkup(createElement(Thinking, { message: { ...base, ...message }, activity }));

const pending = render({});
assert.match(pending, /role="status"/);
assert.match(pending, /Working/);
assert.equal((pending.match(/--step:/g) || []).length, 9);
const reasoning = render({ reasoning: '<script>synthetic()</script>', phase: 'thinking' });
assert.match(reasoning, /aria-expanded="true"/);
assert.match(reasoning, /data-active="true"/);
assert.match(reasoning, /&lt;script&gt;synthetic\(\)&lt;\/script&gt;/);
assert.doesNotMatch(reasoning, /<script>/);
for (const status of ['complete', 'stopped', 'error', 'interrupted']) {
  const html = render({ reasoning: 'Compare the options.', content: 'Answer.', status });
  assert.match(html, /aria-expanded="false"/);
  assert.doesNotMatch(html, /thinking-grid/);
  assert.doesNotMatch(html, /Answer\./);
}
const answering = render({ reasoning: 'Compare.', content: 'Answer.', phase: 'answer' });
assert.match(answering, /aria-expanded="false"/);
assert.doesNotMatch(answering, /thinking-grid/);
assert.equal(render({ content: 'Ordinary answer.', status: 'complete' }), '');
assert.match(render({}, 'Searching the web'), /Searching the web/);
console.log('Thinking markup checks passed: waiting, separate escaped reasoning, answer transition, stopped/error states, and nine-cell activity. Browser interaction and visual checks remain separate.');
