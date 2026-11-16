import { useRef, useState } from 'react';
import { backend, nativeApp } from './backend';
import { Dialog } from './dialogs';
type Prepared = { ticket: string; projects: number; chats: number; messages: number; files: number; attachments: number };
export function DataSettings({ refresh, report }: { refresh: () => Promise<unknown>; report: (message: string) => void }) {
  const [prepared, setPrepared] = useState<Prepared | null>(null);
  const [busy, setBusy] = useState(false); const pending = useRef(false);
  const [error, setError] = useState('');
  if (!nativeApp) return null;
  const run = async (operation: () => Promise<void>) => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError('');
    try { await operation(); } catch (reason) { setError(String(reason)); }
    finally { pending.current = false; setBusy(false); }
  };
  const cancel = () => { if (pending.current) return; if (prepared) void backend.invoke('finish_restore', { ticket: prepared.ticket, confirm: false }).catch(() => {}); setPrepared(null); };
  return <section className="settings-section" aria-labelledby="data-title"><h2 id="data-title">Your data</h2><div className="settings-group"><div className="settings-row"><span className="settings-row-label">Backup & restore</span><div className="search-key-actions"><button className="button" disabled={busy} onClick={() => { void run(async () => { if (await backend.invoke<boolean>('export_data', { chatId: null, folderId: null })) report('Backup saved.'); }); }}>Save backup…</button><button className="button" disabled={busy} onClick={() => { void run(async () => { setPrepared(await backend.invoke<Prepared | null>('prepare_restore')); }); }}>Restore…</button></div></div></div><p className="settings-note">Backups contain readable chats, imported files, and chat attachments. Keep them private. Keys, sign-ins, linked folders, and billing records aren’t included.</p>{error && !prepared && <p className="dialog-error" role="alert">{error}</p>}
    {prepared && <Dialog title="Restore this backup?" onClose={cancel}><p>Add {prepared.projects} projects, {prepared.chats} chats, {prepared.files} project files and {prepared.attachments} attachments?</p><p className="usage-note">Existing data stays in place. Restored items receive new identities. Reconnect linked folders and enable project context when you’re ready.</p>{error && <p className="dialog-error" role="alert">{error}</p>}<div className="dialog-actions"><button className="button quiet" disabled={busy} onClick={cancel}>Cancel</button><button className="button primary" disabled={busy} onClick={() => { void run(async () => { await backend.invoke('finish_restore', { ticket: prepared.ticket, confirm: true }); await refresh(); setPrepared(null); report('Backup restored as new projects and chats.'); }); }}>{busy ? 'Restoring…' : 'Restore as new items'}</button></div></Dialog>}
  </section>;
}
