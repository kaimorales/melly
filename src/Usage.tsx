import { useEffect, useState } from 'react';
import { backend } from './backend';
import { Dialog } from './dialogs';
import { ContextSelector } from './ContextSelector';
import { ContextSummary } from './ContextSummary';
import type { ContextLimit, ContextSelection } from './types';
import './usage.css';

type Preview = { selection: ContextSelection; estimatedTokens: number; capacity: number | null; omittedMessages: number; draftExceedsLimit: boolean };

export function ContextMeter({ draftContext, contextRevision, onLimit, onCompacting, chatId, providerId, modelId, draft, revision, busy }: {
  draftContext: ContextLimit | null; contextRevision: number; onLimit: (limit: ContextLimit | null) => Promise<void>; onCompacting: (chatId: string | null) => void; chatId: string | null; providerId: string; modelId: string; draft: string; revision: number; busy: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [summaryRevision, setSummaryRevision] = useState(0);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [error, setError] = useState('');
  useEffect(() => { setPreview(null); setError(''); }, [chatId, providerId, modelId]);
  useEffect(() => {
    if (busy) return;
    let active = true;
    const timer = window.setTimeout(() => {
      void backend.invoke<Preview>('context_preview', { chatId, providerId, modelId, draft, draftContext })
        .then(value => { if (active) { setPreview(value); setError(''); } })
        .catch(reason => { if (active) { setPreview(null); setError(String(reason)); } });
    }, 450);
    return () => { active = false; window.clearTimeout(timer); };
  }, [chatId, providerId, modelId, draft, revision, busy, draftContext, contextRevision, summaryRevision]);
  const percent = preview?.capacity ? Math.min(100, Math.round(preview.estimatedTokens / preview.capacity * 100)) : null;
  return <>
    <button type="button" className="context-meter" aria-label="Context and tokens" onClick={() => setOpen(true)}>
      <svg width="19" height="19" viewBox="0 0 20 20" aria-hidden="true">
        <circle cx="10" cy="10" r="7" className="context-track" />
        <circle cx="10" cy="10" r="7" className="context-fill" strokeDasharray={percent === null ? '2 3' : `${percent * .44} 44`} transform="rotate(-90 10 10)" />
      </svg>
    </button>
    {open && <Dialog title="Context" className="context-dialog" onClose={() => setOpen(false)}>
      {error ? <p role="alert">{error}</p> : preview ? <>
        <div className="context-total">≈ {preview.estimatedTokens.toLocaleString()} <span>tokens{preview.capacity ? ` of ${preview.capacity.toLocaleString()}` : ''}</span></div>
        {percent !== null && <div className="context-bar" aria-label={`Approximately ${percent}% of context capacity`}><span style={{ width: `${percent}%` }} /></div>}
        {preview.draftExceedsLimit && <p className="context-omission" role="alert">This draft exceeds the configured context limit.</p>}
        {preview.omittedMessages > 0 && <p className="context-omission">{preview.omittedMessages} older {preview.omittedMessages === 1 ? 'message' : 'messages'} omitted</p>}
        <ContextSelector selection={preview.selection} busy={busy} onChange={onLimit} />
        {chatId && <ContextSummary key={chatId} chatId={chatId} providerId={providerId} modelId={modelId} busy={busy} onPending={onCompacting} onChanged={() => setSummaryRevision(value => value + 1)} />}
      </> : <p role="status">Preparing context…</p>}
    </Dialog>}
  </>;
}
