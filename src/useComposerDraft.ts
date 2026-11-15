import { useCallback, useEffect, useReducer, useRef, type SetStateAction } from 'react';
import { backend } from './backend';
import type { Message } from './types';

type Attachment = { id: string; name: string; bytes: number; kind: string };
type Composer = { content: string; attachments: Attachment[]; editing: Message | null };
const empty = (): Composer => ({ content: '', attachments: [], editing: null });
const keyFor = (id: string | null) => id ?? 'new';

export function useComposerDraft(chatId: string | null, enabled: boolean, report: (error: string) => void) {
  const key = keyFor(chatId);
  const currentKey = useRef(key); currentKey.current = key;
  const forgotten = useRef(new Set<string>());
  const cache = useRef(new Map<string, Composer>());
  const pending = useRef(new Map<string, Composer>());
  const writing = useRef<Promise<void> | null>(null);
  const failure = useRef<unknown>(null);
  const active = useRef(true);
  const reportRef = useRef(report); reportRef.current = report;
  const [, render] = useReducer(value => value + 1, 0);
  const notify = useCallback(() => { if (active.current) render(); }, []);
  const flush = useCallback(async () => {
    if (!writing.current) {
      failure.current = null;
      writing.current = (async () => {
        while (pending.current.size) {
          const [target, value] = pending.current.entries().next().value!;
          pending.current.delete(target);
          try {
            await backend.invoke('save_composer_draft', { chatId: target === 'new' ? null : target, draft: { content: value.content, attachmentIds: value.attachments.map(file => file.id), editingMessageId: value.editing?.id ?? null } });
          } catch (reason) {
            if (forgotten.current.has(target)) continue;
            if (!pending.current.has(target)) pending.current.set(target, value);
            failure.current = reason;
            reportRef.current(`Could not save this draft: ${String(reason)}`);
            break;
          }
        }
      })().finally(() => { writing.current = null; if (pending.current.size && !failure.current) queueMicrotask(() => { void flush().catch(() => {}); }); });
    }
    await writing.current;
    if (failure.current) throw failure.current;
  }, []);
  const put = useCallback((target: string, value: Composer) => {
    forgotten.current.delete(target);
    cache.current.set(target, value);
    pending.current.set(target, value);
    notify();
    void flush().catch(() => {});
  }, [flush, notify]);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  useEffect(() => {
    if (!enabled || cache.current.has(key)) return;
    let current = true;
    void backend.invoke<Composer>('composer_draft', { chatId }).then(value => {
      if (current && !cache.current.has(key)) { cache.current.set(key, value); notify(); }
    }).catch(reason => { if (current) reportRef.current(`Could not restore this draft: ${String(reason)}`); });
    return () => { current = false; };
  }, [chatId, enabled, key, notify]);
  const value = cache.current.get(key) ?? empty();
  const set = <K extends keyof Composer>(field: K, action: SetStateAction<Composer[K]>) => {
    const target = currentKey.current;
    const previous = cache.current.get(target) ?? empty();
    const next = typeof action === 'function' ? (action as (old: Composer[K]) => Composer[K])(previous[field]) : action;
    put(target, { ...previous, [field]: next });
  };
  return {
    ...value, ready: enabled && cache.current.has(key), key, flush,
    setContent: (next: SetStateAction<string>) => set('content', next),
    setAttachments: (next: SetStateAction<Attachment[]>) => set('attachments', next),
    setEditing: (next: SetStateAction<Message | null>) => set('editing', next),
    clear: (target: string) => put(target, empty()),
    forget: (target: string) => { forgotten.current.add(target); cache.current.delete(target); pending.current.delete(target); notify(); },
    adopt: (id: string) => put(id, { ...(cache.current.get(key) ?? empty()), editing: null }),
  };
}
