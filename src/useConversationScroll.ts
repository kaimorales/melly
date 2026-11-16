import { useCallback, useLayoutEffect, useRef, useState, type MouseEvent, type RefObject } from 'react';

// Observe layout only while the conversation is mounted. Streaming, font loads,
// and opened details all use the same anchor; there is no idle scroll timer.
export function useConversationScroll(scroll: RefObject<HTMLDivElement | null>, follow: RefObject<boolean>, chatId: string | null, enabled: boolean) {
  const content = useRef<HTMLDivElement>(null);
  const anchor = useRef<{ node: HTMLElement; offset: number } | null>(null);
  const expectedTop = useRef<number | null>(null);
  const [away, setAway] = useState(false);
  const distance = useCallback(() => {
    const node = scroll.current;
    return node ? node.scrollHeight - node.clientHeight - node.scrollTop : 0;
  }, [scroll]);
  const remember = useCallback((preferred?: HTMLElement) => {
    const viewport = scroll.current;
    if (!viewport) return;
    const rect = viewport.getBoundingClientRect();
    let node = preferred;
    if (!node) {
      for (const y of [12, 40, 80]) {
        const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + y);
        const candidate = hit?.closest<HTMLElement>('p, pre, li, summary, h1, h2, h3, .thinking-toggle, .message');
        if (candidate && viewport.contains(candidate)) { node = candidate; break; }
      }
    }
    anchor.current = node ? { node, offset: node.getBoundingClientRect().top - rect.top } : null;
  }, [scroll]);
  const move = useCallback((top: number) => {
    const viewport = scroll.current;
    if (!viewport || Math.abs(viewport.scrollTop - top) < 1) return;
    viewport.scrollTop = top;
    expectedTop.current = viewport.scrollTop;
  }, [scroll]);
  const settle = useCallback(() => {
    const viewport = scroll.current;
    if (!viewport) return;
    if (follow.current) move(viewport.scrollHeight);
    else {
      const saved = anchor.current;
      if (saved?.node.isConnected && saved.node.getClientRects().length) {
        move(viewport.scrollTop + saved.node.getBoundingClientRect().top - viewport.getBoundingClientRect().top - saved.offset);
      } else remember();
    }
    setAway(distance() > 40);
  }, [distance, follow, move, remember, scroll]);
  useLayoutEffect(() => {
    if (!enabled || !scroll.current || !content.current) return;
    anchor.current = null;
    expectedTop.current = null;
    settle();
    const observer = new ResizeObserver(settle);
    observer.observe(scroll.current);
    observer.observe(content.current);
    return () => observer.disconnect();
  }, [chatId, enabled, scroll, settle]);
  const onScroll = useCallback(() => {
    const viewport = scroll.current;
    if (!viewport) return;
    if (expectedTop.current === null || Math.abs(viewport.scrollTop - expectedTop.current) > 1) follow.current = distance() <= 40;
    expectedTop.current = null;
    setAway(distance() > 40);
    remember();
  }, [distance, follow, remember, scroll]);
  const pause = useCallback(() => { follow.current = false; remember(); }, [follow, remember]);
  const onInteract = useCallback((event: MouseEvent<HTMLDivElement>) => {
    const target = event.target instanceof Element ? event.target.closest<HTMLElement>('summary, .thinking-toggle') : null;
    if (target) { follow.current = false; remember(target); }
  }, [follow, remember]);
  const latest = useCallback(() => { follow.current = true; anchor.current = null; settle(); }, [follow, settle]);
  return { content, away, onScroll, onInteract, pause, remember, latest };
}
