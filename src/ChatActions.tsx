import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Dialog } from './dialogs';
import type { Chat, Folder } from './types';

export type ChatActionsProps = {
  chat: Chat;
  folders: Folder[];
  busy: boolean;
  onRename: (chat: Chat) => void;
  onDelete: (chat: Chat) => void;
  onMove: (chat: Chat, folderId: string | null) => void;
  onClose: () => void;
};

export function ChatActions({ chat, folders, busy, onRename, onDelete, onMove, onClose, showMove = true }: ChatActionsProps & { showMove?: boolean }) {
  return <>
    <button className="menu-item" role="menuitem" onClick={() => { onClose(); onRename(chat); }}>Rename chat</button>
    {showMove && <label className="menu-field">Move to folder
      <select aria-label="Move to folder" disabled={busy} value={chat.folderId || ''} onChange={(event) => { onClose(); onMove(chat, event.target.value || null); }}>
        <option value="">No folder</option>{folders.map((folder) => <option key={folder.id} value={folder.id}>{folder.title}</option>)}
      </select>
    </label>}
    <button className="menu-item danger" role="menuitem" disabled={busy} title={busy ? 'Stop the response before deleting this chat' : undefined} onClick={() => { onClose(); onDelete(chat); }}>Delete chat</button>
  </>;
}

export function ChatActionsMenu({ anchor, point, ...props }: ChatActionsProps & { anchor: HTMLElement; point?: { x: number; y: number } }) {
  const ref = useRef<HTMLDivElement>(null);
  const close = useRef(props.onClose); close.current = props.onClose;
  const [position, setPosition] = useState({ left: 0, top: 0 });
  useLayoutEffect(() => {
    const bounds = anchor.getBoundingClientRect();
    const menu = ref.current!;
    setPosition({
      left: Math.max(8, Math.min(point?.x ?? bounds.right - 8, window.innerWidth - menu.offsetWidth - 8)),
      top: Math.max(8, Math.min(point?.y ?? bounds.top, window.innerHeight - menu.offsetHeight - 8)),
    });
    menu.querySelector<HTMLElement>('button:not(:disabled)')?.focus();
  }, [anchor, point]);
  useEffect(() => {
    const outside = (event: MouseEvent) => { if (!ref.current?.contains(event.target as Node) && !anchor.contains(event.target as Node)) close.current(); };
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); close.current(); anchor.focus(); }
    };
    const dismiss = () => close.current();
    document.addEventListener('mousedown', outside); document.addEventListener('keydown', escape);
    window.addEventListener('resize', dismiss);
    return () => { document.removeEventListener('mousedown', outside); document.removeEventListener('keydown', escape); window.removeEventListener('resize', dismiss); };
  }, [anchor]);
  return createPortal(<div ref={ref} className="menu sidebar-chat-menu" style={position} role="menu" aria-label={`Actions for ${props.chat.title}`} data-menu-root onBlur={(event) => {
    if (event.relatedTarget && !event.currentTarget.contains(event.relatedTarget as Node)) props.onClose();
  }} onKeyDown={(event) => {
    if (event.target instanceof HTMLSelectElement || !['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    const controls = [...event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), select:not(:disabled)')];
    const at = controls.indexOf(document.activeElement as HTMLElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? controls.length - 1 : (at + (event.key === 'ArrowDown' ? 1 : -1) + controls.length) % controls.length;
    event.preventDefault(); controls[next]?.focus();
  }}><ChatActions {...props} showMove={false} /></div>, document.body);
}

export function DeleteChatDialog({ chat, busy, onClose, onDelete }: { chat: Chat; busy: boolean; onClose: () => void; onDelete: (chat: Chat) => Promise<void> }) {
  const [saving, setSaving] = useState(false); const [error, setError] = useState('');
  const pending = useRef(false);
  const remove = async () => {
    if (pending.current || busy) return;
    pending.current = true; setSaving(true); setError('');
    try { await onDelete(chat); onClose(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { pending.current = false; setSaving(false); }
  };
  return <Dialog title="Delete this chat?" onClose={() => { if (!pending.current) onClose(); }}>
    <p>“{chat.title}” and its messages will be permanently removed.</p>
    {error && <div className="dialog-error" role="alert">{error}</div>}
    {busy && <p className="message-status">Stop the response before deleting this chat.</p>}
    <div className="dialog-actions">
      <button className="button quiet" disabled={saving} onClick={onClose}>Cancel</button>
      <button className="button danger" disabled={saving || busy} onClick={() => { void remove(); }}>{saving ? 'Deleting…' : 'Delete chat'}</button>
    </div>
  </Dialog>;
}
