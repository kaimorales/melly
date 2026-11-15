import { useEffect, useState } from 'react';
import { backend } from './backend';
type Summary = { text: string; through: number; messages: number };
export function ContextSummary({ chatId, providerId, modelId, busy, onPending, onChanged }: {
  chatId: string; providerId: string; modelId: string; busy: boolean;
  onPending: (chatId: string | null) => void; onChanged: () => void;
}) {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let current = true;
    setSummary(null); setError('');
    void backend.invoke<Summary | null>('context_summary', { chatId }).then(value => { if (current) setSummary(value); }).catch(reason => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, [chatId]);
  const compact = async () => {
    setPending(true); setError(''); onPending(chatId);
    try { setSummary(await backend.invoke<Summary>('compact_context', { chatId, providerId, modelId })); onChanged(); }
    catch (reason) { setError(String(reason)); }
    finally { setPending(false); onPending(null); }
  };
  const restore = async () => {
    setError('');
    try { await backend.invoke('clear_context_summary', { chatId }); setSummary(null); onChanged(); }
    catch (reason) { setError(String(reason)); }
  };
  return <div className="context-summary">
    <div className="context-summary-actions">
      <button className="text-action" disabled={busy || pending} title="Summarize older messages with the selected model. Originals stay in this chat." onClick={() => void compact()}>{pending ? 'Compacting…' : 'Compact context'}</button>
      {pending ? <button className="text-action" onClick={() => { void backend.invoke('stop_chat', { chatId }).catch(reason => setError(String(reason))); }}>Stop</button>
        : summary && <button className="text-action" disabled={busy} onClick={() => void restore()}>Undo compaction</button>}
    </div>
    {summary && <details><summary>Task notes</summary><pre>{summary.text}</pre></details>}
    {error && <p role="alert" className="context-omission">{error}</p>}
  </div>;
}
