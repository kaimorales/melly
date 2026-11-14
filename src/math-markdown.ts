import { fromMarkdown } from 'mdast-util-from-markdown';
import type { Root, RootContent } from 'mdast';
import type { VFile } from 'vfile';

// Normalize the other common TeX delimiters before Markdown consumes their
// backslashes. Parser-identified literal ranges protect code, links and HTML.
// Ordinary messages take the fast path without another parse.
export function normalizeMathDelimiters(source: string): string {
  if (!source.includes('\\(') && !source.includes('\\[')) return source;
  const protectedRanges: Array<[number, number]> = [];
  const pending: Array<Root | RootContent> = [fromMarkdown(source)];
  while (pending.length) {
    const node = pending.pop()!;
    if (['code', 'inlineCode', 'html', 'link', 'image', 'definition'].includes(node.type) && node.position) {
      protectedRanges.push([node.position.start.offset!, node.position.end.offset!]);
    } else if ('children' in node) pending.push(...node.children);
  }
  protectedRanges.sort((a, b) => a[0] - b[0]);
  const parts: string[] = []; let range = 0; let index = 0;
  const missing = new Set<string>();
  while (index < source.length) {
    while (range < protectedRanges.length && index >= protectedRanges[range][1]) range++;
    const protectedRange = protectedRanges[range];
    if (protectedRange && index >= protectedRange[0]) {
      parts.push(source.slice(index, protectedRange[1])); index = protectedRange[1]; continue;
    }
    if (source[index] === '\\' && source[index + 1] === '\\') { parts.push('\\\\'); index += 2; continue; }
    if (source[index] === '\\' && ['(', '['].includes(source[index + 1])) {
      const open = source[index + 1]; const end = open === '(' ? '\\)' : '\\]';
      let close = missing.has(end) ? -1 : source.indexOf(end, index + 2);
      while (close !== -1) {
        let slashes = 0; for (let at = close - 1; at >= 0 && source[at] === '\\'; at--) slashes++;
        if (!(slashes % 2)) break;
        close = source.indexOf(end, close + 2);
      }
      if (close === -1) missing.add(end);
      // Don't bridge across a literal Markdown range into another paragraph.
      if (close !== -1 && (!protectedRange || protectedRange[0] >= close + 2)) {
        const delimiter = open === '(' ? '$' : '$$';
        const value = source.slice(index + 2, close);
        // Explicit TeX delimiters are unambiguous; insignificant outer spaces
        // must not trigger the price guard used for single-dollar expressions.
        parts.push(delimiter, open === '(' ? value.trim() : value, delimiter); index = close + 2; continue;
      }
    }
    parts.push(source[index++]);
  }
  return parts.join('');
}

export function remarkMathPresentation() {
  return (tree: Root, file: VFile) => {
    const source = String(file);
    const pending: Array<Root | RootContent> = [tree];
    while (pending.length) {
      const parent = pending.pop()!;
      if (!('children' in parent)) continue;
      const children = parent.children as RootContent[];
      children.forEach((node, index) => {
        if (node.type === 'inlineMath' && node.position) {
          const raw = source.slice(node.position.start.offset, node.position.end.offset);
          // A price range such as "$5 or $10" is prose, not an equation.
          const single = raw.startsWith('$') && !raw.startsWith('$$');
          if (single && (/^\$\s|\s\$$/.test(raw) || /\d/.test(source[node.position.end.offset!] || ''))) {
            children[index] = { type: 'text', value: raw }; return;
          }
          if (!single && node.data?.hProperties) node.data.hProperties.className = ['language-math', 'math-display'];
        } else if (node.type === 'math' && node.position) {
          const raw = source.slice(node.position.start.offset, node.position.end.offset);
          const closed = /\n[ >\t]*\${2,}[ \t]*$/.test(raw);
          const code = node.data?.hChildren?.[0];
          if (code?.type === 'element') code.properties['data-math-pending'] = !closed;
        }
        pending.push(node);
      });
    }
  };
}
