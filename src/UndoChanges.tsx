import { useEffect, useRef, useState } from 'react';
import { backend } from './backend';

type Change = { id: string; path: string; state: string };
export function UndoChanges({ chatId, messageId, busy }: { chatId: string; messageId: string; busy: boolean }) {
  const [changes, setChanges] = useState<Change[]>([]);
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  const button = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (busy) return;
    let current = true;
    void backend.invoke<Change[]>('file_changes', { chatId, messageId }).then(value => { if (current) setChanges(value); }).catch(() => {});
    return () => { current = false; };
  }, [chatId, messageId, busy]);
  const remaining = changes.filter(change => change.state !== 'undone');
  async function undo(changeId: string | null) {
    if (busy || pending) return;
    setPending(true); setError('');
    try {
      await backend.invoke('undo_file_changes', { chatId, messageId, changeId });
      setChanges(await backend.invoke<Change[]>('file_changes', { chatId, messageId }));
    } catch (reason) {
      setError(String(reason));
      try { setChanges(await backend.invoke<Change[]>('file_changes', { chatId, messageId })); } catch { /* Retain the actionable undo error. */ }
    } finally { setPending(false); }
  }
  if (!changes.length) return null;
  return <div className="undo-changes">
    {remaining.length ? <button ref={button} type="button" className="text-action" aria-expanded={open} disabled={busy || pending} onClick={() => { setOpen(!open); setError(''); }}>Undo changes…</button> : <span className="message-status" role="status">Changes undone</span>}
    {open && remaining.length > 0 && <section className="undo-review" aria-label="Undo file changes" onKeyDown={event => { if (event.key === 'Escape' && !pending) { setOpen(false); button.current?.focus(); } }}>
      {remaining.map(change => <div className="undo-file" key={change.id}><code>{change.path}</code><button type="button" className="text-action" disabled={pending || busy} aria-label={`Undo change to ${change.path}`} onClick={() => void undo(change.id)}>Undo</button></div>)}
      {remaining.length > 1 && <button type="button" className="text-action" disabled={pending || busy} onClick={() => void undo(null)}>Undo all changes in this response</button>}
      {error && <p className="file-edit-error" role="alert">{error}</p>}
    </section>}
  </div>;
}
