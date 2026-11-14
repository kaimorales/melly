import { createLowlight } from 'lowlight';
import python from 'highlight.js/lib/languages/python';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import rust from 'highlight.js/lib/languages/rust';
import go from 'highlight.js/lib/languages/go';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import xml from 'highlight.js/lib/languages/xml';
import css from 'highlight.js/lib/languages/css';
import sql from 'highlight.js/lib/languages/sql';
import yaml from 'highlight.js/lib/languages/yaml';
import java from 'highlight.js/lib/languages/java';
import cpp from 'highlight.js/lib/languages/cpp';
import swift from 'highlight.js/lib/languages/swift';
import markdown from 'highlight.js/lib/languages/markdown';
import type { RootContent } from 'hast';
import type { SyntaxToken } from './syntax';

const lowlight = createLowlight({ python, javascript, typescript, rust, go, bash, json, xml, css, sql, yaml, java, cpp, swift, markdown });
function tokenize(code: string, language: string): SyntaxToken[] | null {
  if (code.length > 32_768 || !lowlight.registered(language)) return null;
  const tokens: SyntaxToken[] = [];
  const visit = (nodes: RootContent[], inherited: string[] = [], depth = 0) => {
    if (depth > 64) throw new Error('Token nesting limit');
    for (const node of nodes) {
      if (tokens.length >= 6000) throw new Error('Token count limit');
      if (node.type === 'text') tokens.push({ text: node.value, classes: inherited.join(' ') });
      else if (node.type === 'element') {
        const classes = Array.isArray(node.properties.className) ? node.properties.className.filter((value): value is string => typeof value === 'string' && /^[\w-]{1,60}$/.test(value)) : [];
        visit(node.children, [...inherited, ...classes], depth + 1);
      }
    }
  };
  visit(lowlight.highlight(language, code).children);
  // Preserve source exactly, even if a grammar behaves unexpectedly.
  return tokens.map((token) => token.text).join('') === code ? tokens : null;
}
self.onmessage = (event: MessageEvent<{ id: number; code: string; language: string }>) => {
  const { id, code, language } = event.data;
  let tokens: SyntaxToken[] | null = null;
  try { if (typeof code === 'string' && typeof language === 'string') tokens = tokenize(code, language); } catch { /* Plain text is the safe fallback. */ }
  self.postMessage({ id, tokens });
};
