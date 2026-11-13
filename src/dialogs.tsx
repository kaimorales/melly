import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { backend } from './backend';
import { Icon } from './icons';
import type { Chat, Folder } from './types';

export function Dialog({ title, closeLabel = 'Close dialog', onClose, children, className = '', icon }: { title: string; closeLabel?: string; onClose: () => void; children: ReactNode; className?: string; icon?: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose); closeRef.current = onClose;
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const dialog = ref.current;
    const focusable = () => Array.from(dialog?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex="0"]') || []);
    (dialog?.querySelector<HTMLElement>('[data-autofocus]') || focusable()[0] || dialog)?.focus();
    const keydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closeRef.current(); }
      if (event.key === 'Tab') {
        const items = focusable(); const first = items[0]; const last = items.at(-1);
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
      }
    };
    dialog?.addEventListener('keydown', keydown);
    return () => { dialog?.removeEventListener('keydown', keydown); previous?.focus(); };
  }, []);
  return <div className="dialog-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <div ref={ref} className={`dialog ${className}`} role="dialog" aria-modal="true" aria-label={title} tabIndex={-1}>
      <div className="dialog-header">{icon}<h2>{title}</h2><button className="icon-button" aria-label={closeLabel} onClick={onClose}><Icon name="close" /></button></div>
      {children}
    </div>
  </div>;
}

export function NameDialog({ title, initial = '', action, onClose, onSave }: { title: string; initial?: string; action: string; onClose: () => void; onSave: (name: string) => Promise<void> }) {
  const [name, setName] = useState(initial); const [error, setError] = useState(''); const [saving, setSaving] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault(); if (!name.trim() || saving) return;
    setSaving(true); setError('');
    try { await onSave(name.trim()); onClose(); } catch (reason) { setError(String(reason)); setSaving(false); }
  };
  return <Dialog title={title} onClose={onClose}><form onSubmit={submit}>
    <label className="field">Name<input data-autofocus value={name} onChange={(event) => setName(event.target.value)} maxLength={180} required /></label>
    {error && <div className="dialog-error" role="alert">{error}</div>}
    <div className="dialog-actions"><button className="button quiet" type="button" onClick={onClose}>Cancel</button><button className="button primary" disabled={!name.trim() || saving}>{saving ? 'Saving…' : action}</button></div>
  </form></Dialog>;
}

export function SearchDialog({ folders, onClose, onSelect }: { folders: Folder[]; onClose: () => void; onSelect: (chat: Chat) => void }) {
  const [query, setQuery] = useState(''); const [results, setResults] = useState<Chat[]>([]); const [error, setError] = useState('');
  useEffect(() => {
    let cancelled = false;
    backend.invoke<Chat[]>('search_chats', { query }).then((chats) => { if (!cancelled) { setResults(chats); setError(''); } }).catch((reason) => { if (!cancelled) setError(String(reason)); });
    return () => { cancelled = true; };
  }, [query]);
  return <Dialog title="Search chats" onClose={onClose} className="search-dialog">
    <input className="search-input" aria-label="Search chats" data-autofocus placeholder="Search titles and messages" value={query} onChange={(event) => setQuery(event.target.value)} />
    {error && <div className="dialog-error" role="alert">{error}</div>}
    <div className="search-results">
      {results.map((chat) => <button className="search-result" key={chat.id} aria-label={chat.title} onClick={() => { onSelect(chat); onClose(); }}><Icon name="message" /><span>{chat.title}</span><small>{folders.find((folder) => folder.id === chat.folderId)?.title}</small></button>)}
      {!results.length && <p>{query ? 'No chats match this search.' : 'Your chats will appear here.'}</p>}
    </div>
  </Dialog>;
}
