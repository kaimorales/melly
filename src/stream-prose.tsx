import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState, type ComponentProps } from 'react';
import type { ExtraProps } from 'react-markdown';
import type { Element, Root, Text } from 'hast';

export const proseFadeDuration = 180;
type Arrival = { start: number; end: number; at: number };
type Run = { text: string; start: number; at?: number };
declare module 'hast' { interface ElementData { streamRuns?: Run[] } }
const words = new Intl.Segmenter(undefined, { granularity: 'word' });
const excluded = new Set(['pre', 'code', 'a', 'button', 'svg', 'math']);

// A short, bounded arrival history follows the committed Markdown. Re-parsing
// emphasis or a list must not make previously received words fade in again.
export function useProseReveal(text: string, status: string) {
  const previous = useRef<{ text: string; arrivals: Arrival[] } | null>(null);
  const [settledText, setSettledText] = useState<string | null>(null);
  const reveal = useMemo(() => {
    const now = performance.now();
    const prior = previous.current;
    const enabled = status === 'streaming' || (status === 'complete' && prior !== null && settledText !== text);
    const append = prior ? text.startsWith(prior.text) : status === 'streaming';
    const arrivals = enabled && append ? (prior?.arrivals || []).filter(item => now - item.at < proseFadeDuration) : [];
    const start = prior?.text.length || 0;
    if (enabled && append && text.length > start) arrivals.push({ start, end: text.length, at: now });
    return { text, arrivals: arrivals.slice(-64) };
  }, [text, status, settledText]);
  useLayoutEffect(() => { previous.current = reveal; }, [reveal]);
  useEffect(() => {
    if (status !== 'complete' || !reveal.arrivals.length) return;
    // One cleanup at the end of the final fade, never an idle animation loop.
    const remaining = Math.max(0, reveal.arrivals.at(-1)!.at + proseFadeDuration - performance.now());
    const timer = window.setTimeout(() => setSettledText(text), remaining);
    return () => clearTimeout(timer);
  }, [reveal, status, text]);
  return reveal.arrivals;
}

export function rehypeProseReveal({ arrivals }: { arrivals: Arrival[] }) {
  return (tree: Root) => {
    if (!arrivals.length || document.hidden || document.documentElement.dataset.reduceMotion === 'true' || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    let budget = 96;
    const cutoff = Math.max(arrivals[0].start, arrivals.at(-1)!.end - 1536);
    function visit(parent: Root | Element) {
      for (let i = parent.children.length - 1; i >= 0 && budget > 0; i--) {
        const child = parent.children[i];
        if (child.type === 'element') { if (!excluded.has(child.tagName)) visit(child); continue; }
        if (child.type !== 'text' || !child.value.trim() || (child.position?.end.offset ?? 0) <= cutoff) continue;
        const offset = child.position?.start.offset;
        if (offset === undefined) continue;
        const segments = words.segment(child.value);
        const begin = segments.containing(Math.max(0, cutoff - offset))?.index || 0;
        const tail = Array.from(words.segment(child.value.slice(begin)));
        const runs: Run[] = [];
        let prefixEnd = child.value.length;
        for (let j = tail.length - 1; j >= 0 && budget > 0; j--) {
          const part = tail[j];
          const start = begin + part.index;
          const arrival = arrivals.find(item => offset + start >= item.start && offset + start < item.end);
          const at = part.segment.trim() ? arrival?.at : undefined;
          runs.unshift({ text: part.segment, start: offset + start, at });
          prefixEnd = start;
          if (at !== undefined) budget--;
        }
        if (!runs.some(run => run.at !== undefined)) continue;
        if (prefixEnd) runs.unshift({ text: child.value.slice(0, prefixEnd), start: offset });
        const wrapper: Element = { type: 'element', tagName: 'span', properties: {}, data: { streamRuns: runs }, children: [child as Text] };
        parent.children[i] = wrapper;
      }
    }
    visit(tree);
  };
}

const ArrivingWord = memo(function ArrivingWord({ text, at }: { text: string; at: number }) {
  // Keep the same CSS animation timeline while a partially received word grows.
  const delay = useMemo(() => -Math.min(proseFadeDuration, performance.now() - at), [at]);
  return <span className="arriving-word" style={{ animationDuration: `${proseFadeDuration}ms`, animationDelay: `${delay}ms` }}>{text}</span>;
});

export function StreamingSpan({ node, children, ...props }: ComponentProps<'span'> & ExtraProps) {
  const runs = node?.data?.streamRuns as Run[] | undefined;
  if (!runs) return <span {...props}>{children}</span>;
  return <>{runs.map(run => run.at === undefined ? run.text : <ArrivingWord key={run.start} text={run.text} at={run.at} />)}</>;
}
