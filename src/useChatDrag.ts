import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from 'react';
import type { Chat } from './types';

type Options = {
  list: RefObject<HTMLDivElement | null>; chats: Chat[]; busy: Set<string>; collapsed: Set<string>;
  reveal: (folderId: string) => void; onStart: () => void;
  onMove: (chat: Chat, folderId: string | null) => void;
};
type Gesture = { chatId: string; pointerId: number; source: HTMLButtonElement; startX: number; startY: number; x: number; y: number; active: boolean };

// Only internal pointer gestures can move a chat. No OS drag session, clipboard
// payload, external drop data or drag framework is involved.
export function useChatDrag(options: Options) {
  const current = useRef(options); current.current = options;
  const gesture = useRef<Gesture | null>(null);
  const preview = useRef<HTMLDivElement>(null);
  const target = useRef<string | null>(null); // Empty string is Recent; null is outside.
  const expand = useRef<ReturnType<typeof setTimeout> | null>(null);
  const frame = useRef<number | null>(null); const lastFrame = useRef(0);
  const skipClick = useRef(false);
  const cleanup = useRef<() => void>(() => {});
  const [dragging, setDragging] = useState<string | null>(null);
  const [destination, setDestination] = useState<string | null>(null);
  const chat = () => current.current.chats.find((value) => value.id === gesture.current?.chatId && !current.current.busy.has(value.id));
  const stopExpand = () => { if (expand.current !== null) clearTimeout(expand.current); expand.current = null; };
  const placePreview = () => {
    const value = gesture.current;
    if (value && preview.current) preview.current.style.transform = `translate3d(${Math.min(value.x + 14, window.innerWidth - preview.current.offsetWidth - 8)}px, ${Math.min(value.y + 12, window.innerHeight - preview.current.offsetHeight - 8)}px, 0)`;
  };
  const updateTarget = () => {
    const value = gesture.current; if (!value) return;
    const element = document.elementFromPoint(value.x, value.y)?.closest<HTMLElement>('[data-drop-folder]');
    const id = element && current.current.list.current?.contains(element) ? element.dataset.dropFolder ?? null : null;
    if (target.current === id) return;
    stopExpand(); target.current = id; setDestination(id);
    if (id && current.current.collapsed.has(id)) expand.current = setTimeout(() => {
      expand.current = null; current.current.reveal(id);
    }, 450);
  };
  const scrollSpeed = () => {
    const value = gesture.current; const list = current.current.list.current;
    if (!value?.active || !list) return 0;
    const bounds = list.getBoundingClientRect(); const margin = 36;
    if (value.x < bounds.left || value.x > bounds.right || value.y < bounds.top || value.y > bounds.bottom) return 0;
    return value.y < bounds.top + margin ? -Math.min(540, (bounds.top + margin - value.y) * 15) : value.y > bounds.bottom - margin ? Math.min(540, (value.y - bounds.bottom + margin) * 15) : 0;
  };
  const scroll = (time: number) => {
    const speed = scrollSpeed(); const list = current.current.list.current;
    if (!speed || !list) { frame.current = null; lastFrame.current = 0; return; }
    const elapsed = lastFrame.current ? Math.min(32, time - lastFrame.current) : 16;
    lastFrame.current = time; list.scrollTop += speed * elapsed / 1000; updateTarget();
    frame.current = requestAnimationFrame(scroll);
  };
  const finish = () => {
    const value = gesture.current; gesture.current = null;
    if (value?.active) skipClick.current = true;
    if (value?.source.hasPointerCapture(value.pointerId)) value.source.releasePointerCapture(value.pointerId);
    stopExpand(); if (frame.current !== null) cancelAnimationFrame(frame.current);
    frame.current = null; lastFrame.current = 0; target.current = null;
    document.body.classList.remove('is-chat-dragging');
    cleanup.current(); cleanup.current = () => {};
    setDragging(null); setDestination(null);
  };
  const move = (event: PointerEvent) => {
    const value = gesture.current;
    if (!value || value.pointerId !== event.pointerId) return;
    if (!chat()) { finish(); return; }
    value.x = event.clientX; value.y = event.clientY;
    if (!value.active) {
      if (Math.hypot(value.x - value.startX, value.y - value.startY) < 6) return;
      value.active = true; setDragging(value.chatId); current.current.onStart();
      document.body.classList.add('is-chat-dragging');
    }
    event.preventDefault(); placePreview(); updateTarget();
    if (scrollSpeed() && frame.current === null) frame.current = requestAnimationFrame(scroll);
  };
  const up = (event: PointerEvent) => {
    const value = gesture.current; if (!value || value.pointerId !== event.pointerId) return;
    if (value.active) { value.x = event.clientX; value.y = event.clientY; updateTarget(); }
    const selected = chat(); const id = target.current; const active = value.active;
    finish();
    if (active && selected && id !== null && selected.folderId !== (id || null)) {
      if (id) current.current.reveal(id);
      current.current.onMove(selected, id || null);
    }
  };
  const key = (event: KeyboardEvent) => { if (event.key === 'Escape') { event.preventDefault(); finish(); } };
  const start = (event: ReactPointerEvent<HTMLButtonElement>, selected: Chat) => {
    skipClick.current = false;
    if (event.button !== 0 || !event.isPrimary || event.pointerType === 'touch' || current.current.busy.has(selected.id)) return;
    finish();
    gesture.current = { chatId: selected.id, pointerId: event.pointerId, source: event.currentTarget, startX: event.clientX, startY: event.clientY, x: event.clientX, y: event.clientY, active: false };
    event.currentTarget.setPointerCapture(event.pointerId);
    document.addEventListener('pointermove', move, { passive: false }); document.addEventListener('pointerup', up);
    document.addEventListener('pointercancel', finish); document.addEventListener('keydown', key);
    window.addEventListener('blur', finish);
    cleanup.current = () => {
      document.removeEventListener('pointermove', move); document.removeEventListener('pointerup', up);
      document.removeEventListener('pointercancel', finish); document.removeEventListener('keydown', key);
      window.removeEventListener('blur', finish);
    };
  };
  useLayoutEffect(placePreview, [dragging]);
  useEffect(() => finish, []);
  return { dragging, destination, preview, start, consumeClick: () => { const skip = skipClick.current; skipClick.current = false; return skip; } };
}
