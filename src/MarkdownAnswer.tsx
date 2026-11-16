import { memo, useMemo } from 'react';
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import { MarkdownCodeBlock } from './CodeBlock';
import { MarkdownInlineCode } from './MathFormula';
import { normalizeMathDelimiters, remarkMathPresentation } from './math-markdown';
import { Icon } from './icons';
import type { Source } from './types';
import { rehypeProseReveal, StreamingSpan, useProseReveal } from './stream-prose';

// Only the changing answer needs Markdown work on a streaming frame. Earlier
// answers retain their parsed tree and code/math workers remain lazy.
export const MarkdownAnswer = memo(function MarkdownAnswer({ text, sources, status, onSource }: { text: string; status: string; sources: Source[]; onSource: (source: Source) => void }) {
  const markdown = useMemo(() => normalizeMathDelimiters(text), [text]);
  const arrivals = useProseReveal(markdown, status);
  const revealPlugins = useMemo(() => [[rehypeProseReveal, { arrivals }] as [typeof rehypeProseReveal, { arrivals: typeof arrivals }]], [arrivals]);
  return <div className="markdown"><ReactMarkdown rehypePlugins={revealPlugins} remarkPlugins={[remarkGfm, remarkMath, remarkMathPresentation]} urlTransform={url => url.startsWith('melly-source:') ? url : defaultUrlTransform(url)} components={{
    a: ({ href, children }) => {
      if (href?.startsWith('melly-source:')) {
        const source = sources.find(entry => entry.id === href.slice('melly-source:'.length));
        return source ? <button className="source inline-source" aria-label={`Open source ${source.title}`} title={source.snippet} onClick={() => onSource(source)}><Icon name={source.kind === 'file' ? 'folder' : source.kind === 'chat' ? 'message' : 'globe'} /><span>{children}</span><Icon name={source.kind === 'web' ? 'external' : 'right'} /></button> : <span>{children}</span>;
      }
      return <a href={href} onClick={event => { event.preventDefault(); if (href) onSource({ id: '', kind: 'web', title: String(children), url: href, chatId: null, messageId: null, snippet: '' }); }}>{children}</a>;
    },
    img: ({ alt }) => <span className="message-status">[Image{alt ? `: ${alt}` : ''}]</span>,
    span: StreamingSpan,
    pre: MarkdownCodeBlock,
    code: MarkdownInlineCode,
  }}>{markdown}</ReactMarkdown></div>;
});
