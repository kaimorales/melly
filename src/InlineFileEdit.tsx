import { UndoChanges } from './UndoChanges';
import { useEffect, useState } from 'react';
import { backend } from './backend';
import { Icon } from './icons';
import './file-edits.css';

const marker = '```melly-file-edit';

export function splitFileEdit(text: string, streaming: boolean) {
  const start = text.search(/(^|\n)```melly-file-edit(?:\r?\n|$)/);
  if (start >= 0) {
    const afterMarker = text.indexOf('\n', start === 0 && text.startsWith(marker) ? start : start + 1);
    const before = text.slice(0, start).trimEnd();
    if (afterMarker < 0) return { text: before, proposed: false };
    const rest = text.slice(afterMarker + 1);
    const closing = /(^|\n)```[ \t]*(?:\r?\n|$)/.exec(rest);
    if (!closing) return { text: before, proposed: false };
    const after = rest.slice(closing.index + closing[0].length).trim();
    return { text: [before, after].filter(Boolean).join('\n\n'), proposed: true };
  }
  const lastLine = text.lastIndexOf('\n') + 1;
  const tail = text.slice(lastLine);
  if (streaming && tail.startsWith('```') && marker.startsWith(tail)) {
    return { text: text.slice(0, lastLine), proposed: false };
  }
  return { text, proposed: false };
}

export type DiffLine = { kind: ' ' | '+' | '-' | '…'; text: string };
type Preview = { status: 'review'; ticket: string; path: string; diff: DiffLine[] } | { status: 'applied'; path: string; diff: DiffLine[] };

export function Diff({ lines }: { lines: DiffLine[] }) {
  return <pre className="file-edit-diff" tabIndex={0} aria-label="File diff">{lines.map((line, index) => (
    <span key={index} data-change={line.kind === '+' ? 'added' : line.kind === '-' ? 'removed' : undefined}>{line.kind} {line.text}{'\n'}</span>
  ))}</pre>;
}

export function FileEditMode({ chatId, projectId, report }: { chatId: string; projectId: string; report: (error: string) => void }) {
  const [mode, setMode] = useState<boolean | null>(null);
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    let current = true;
    setMode(null);
    void backend.invoke<boolean | null>('file_edit_mode', { chatId }).then(value => {
      if (current) setMode(value);
    }).catch(() => {});
    return () => { current = false; };
  }, [chatId, projectId]);
  if (mode === null) return null;
  const change = async (automatic: boolean) => {
    setSaving(true);
    try {
      await backend.invoke('set_file_edit_mode', { chatId, automatic });
      setMode(automatic);
    } catch (reason) { report(String(reason)); }
    finally { setSaving(false); }
  };
  return <label className="file-edit-mode" title="File edits in this linked project. Auto-apply lasts until Melly quits.">
    <Icon name={mode ? 'edit' : 'lock'} />
    <select aria-label="File edit permissions" value={mode ? 'automatic' : 'review'} disabled={saving} onChange={event => void change(event.target.value === 'automatic')}>
      <option value="review">Ask</option>
      <option value="automatic">Auto-apply</option>
    </select>
  </label>;
}

export function InlineFileEdit({ chatId, messageId, busy }: { chatId: string; messageId: string; busy: boolean }) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    if (busy) return;
    let current = true;
    void backend.invoke<Preview | null>('applied_file_edit', { chatId, messageId }).then(result => {
      if (current && result?.status === 'applied') setPreview(result);
    }).catch(() => {});
    return () => { current = false; };
  }, [busy, chatId, messageId]);

  async function review() {
    setPending(true);
    setError('');
    try {
      setPreview(await backend.invoke<Preview>('review_file_edit', { chatId, messageId }));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPending(false);
    }
  }

  async function apply() {
    if (preview?.status !== 'review' || pending || busy) return;
    setPending(true);
    setError('');
    try {
      await backend.invoke('apply_file_edit', { messageId, ticket: preview.ticket });
      setPreview({ status: 'applied', path: preview.path, diff: preview.diff });
    } catch (reason) {
      setPreview(null);
      setError(String(reason));
    } finally {
      setPending(false);
    }
  }

  if (preview?.status === 'applied') {
    const title = <><Icon name="check" /><span>Saved <code>{preview.path}</code></span></>;
    return preview.diff.length
      ? <details className="file-edit-card file-edit-saved"><summary>{title}<Icon name="chevron" /></summary><Diff lines={preview.diff} /><UndoChanges chatId={chatId} messageId={messageId} busy={busy} /></details>
      : <div className="file-edit-card" role="status">{title}</div>;
  }
  if (!preview) return <div className="file-edit-card">
    <button type="button" className="button" disabled={pending || busy} onClick={() => void review()}><Icon name="edit" />{pending ? 'Loading change…' : 'Review file change'}</button>
    {error && <span className="file-edit-error" role="alert">{error}</span>}
  </div>;

  return <section className="file-edit-card file-edit-review" aria-label="Proposed file change">
    <header><Icon name="edit" /><code>{preview.path}</code></header>
    <Diff lines={preview.diff} />
    <div className="file-edit-actions">
      <button type="button" className="button quiet" disabled={pending} onClick={() => setPreview(null)}>Dismiss</button>
      <button type="button" className="button primary" disabled={pending || busy} onClick={() => void apply()}>{pending ? 'Saving…' : 'Apply change'}</button>
    </div>
  </section>;
}
