import { useEffect, useState } from 'react';
import { backend } from './backend';
import { Dialog } from './dialogs';
import { ContextSelector } from './ContextSelector';
import { ContextSummary } from './ContextSummary';
import type { ContextLimit, ContextSelection } from './types';
import type { AttachmentInfo, Source } from './types';
import './usage.css';
type Preview = { selection: ContextSelection; estimatedTokens: number; capacity: number | null; omittedMessages: number; sources: Source[]; warning?: string | null };
type Rates = { inputPerMillion: number; outputPerMillion: number };
type Summary = { input: number; output: number; requests: number; missingUsage: number; reportedCost: number; estimatedCost: number; unpricedRequests: number; reportedRequests: number; estimatedRequests: number; rates: Rates | null };
const number = (value: number) => value.toLocaleString();
const dollars = (value: number) => new Intl.NumberFormat(undefined, { style: 'currency', currency: 'USD', minimumFractionDigits: 2, maximumFractionDigits: 4 }).format(value);
export function ContextMeter({ draftContext, contextRevision, onLimit, onCompacting, chatId, providerId, modelId, draft, attachments, folderContext, revision, busy, onSource }: { draftContext: ContextLimit | null; contextRevision: number; onLimit: (limit: ContextLimit | null) => Promise<void>; onCompacting: (chatId: string | null) => void; chatId: string | null; providerId: string; modelId: string; draft: string; attachments: AttachmentInfo[]; folderContext: boolean; revision: number; busy: boolean; onSource: (source: Source) => void }) {
  const [open, setOpen] = useState(false);
  const [summaryRevision, setSummaryRevision] = useState(0);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [error, setError] = useState('');
  const ids = attachments.map(file => file.id).join(',');
  useEffect(() => { setPreview(null); setError(''); }, [chatId, providerId, modelId]);
  useEffect(() => {
    if (busy) return;
    let current = true;
    const timer = window.setTimeout(() => {
      void backend.invoke<Preview>('context_preview', { chatId, providerId, modelId, draft, draftContext, attachmentIds: ids ? ids.split(',') : [], folderContext }).then(value => { if (current) { setPreview(value); setError(''); } }).catch(reason => { if (current) { setPreview(null); setError(String(reason)); } });
    }, 450);
    return () => { current = false; window.clearTimeout(timer); };
  }, [chatId, providerId, modelId, draft, ids, folderContext, revision, busy, draftContext, contextRevision, summaryRevision]);
  const percent = preview?.capacity ? Math.min(100, Math.round(preview.estimatedTokens / preview.capacity * 100)) : null;
  return <>
    <button type="button" className={`context-meter ${percent !== null && percent >= 85 ? 'near-limit' : ''}`} aria-label="Context and tokens" title={preview ? `About ${number(preview.estimatedTokens)} context tokens${percent !== null ? ` · ${percent}%` : ''}` : 'Context and tokens'} onClick={() => setOpen(true)}>
      <svg width="19" height="19" viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="7" className="context-track" /><circle cx="10" cy="10" r="7" className="context-fill" strokeDasharray={percent === null ? '2 3' : `${percent * .44} 44`} transform="rotate(-90 10 10)" /></svg>
    </button>
    {open && <Dialog title="Context" className="context-dialog" onClose={() => setOpen(false)}>
      {error ? <p role="alert">{error}</p> : preview ? <>
        <div className="context-total">≈ {number(preview.estimatedTokens)} <span>tokens{preview.capacity ? ` of ${number(preview.capacity)}` : ''}</span></div>
        {preview.capacity && <div className="context-bar" aria-label={`Approximately ${percent}% of context capacity`}><span style={{ width: `${percent}%` }} /></div>}
        {preview.warning && <p role="alert" className="context-omission">{preview.warning}</p>}
        {preview.omittedMessages > 0 && <p className="context-omission">{preview.omittedMessages} older {preview.omittedMessages === 1 ? 'message' : 'messages'} omitted</p>}
        {!!preview.sources.length && <div className="context-references"><span>References</span>{preview.sources.map(source => <button key={source.id} title={source.snippet.slice(0, 150)} onClick={() => { setOpen(false); onSource(source); }}>{source.title}</button>)}</div>}
        <ContextSelector selection={preview.selection} busy={busy} onChange={onLimit} />
        {chatId && <ContextSummary key={chatId} chatId={chatId} providerId={providerId} modelId={modelId} busy={busy} onPending={onCompacting} onChanged={() => setSummaryRevision(value => value + 1)} />}
      </> : <p role="status">Preparing context…</p>}
    </Dialog>}
  </>;
}
const accountUrls: Record<string, string> = { chatgpt: 'https://chatgpt.com/#settings/Usage', openrouter: 'https://openrouter.ai/activity', claude: 'https://platform.claude.com/usage', gemini: 'https://aistudio.google.com/usage', deepseek: 'https://platform.deepseek.com/usage', 'ollama': 'https://ollama.com/settings' };
export function ProviderUsage({ providerId, modelId }: { providerId: string; modelId: string }) {
  const [summary, setSummary] = useState<Summary | null>(null);
  const [error, setError] = useState('');
  const [inputRate, setInputRate] = useState(''); const [outputRate, setOutputRate] = useState('');
  const [saving, setSaving] = useState(false); const [saved, setSaved] = useState(false);
  const [account, setAccount] = useState<{ usedUsd: number; limitUsd: number | null; remainingUsd: number | null } | null>(null);
  const [checking, setChecking] = useState(false);
  useEffect(() => {
    let current = true; setSummary(null); setError(''); setSaved(false); setAccount(null);
    void backend.invoke<Summary>('provider_usage', { providerId, modelId }).then(value => { if (current) { setSummary(value); setInputRate(value.rates ? String(value.rates.inputPerMillion) : ''); setOutputRate(value.rates ? String(value.rates.outputPerMillion) : ''); } }).catch(reason => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, [providerId, modelId]);
  const local = providerId === 'ollama-local' || providerId === 'mock';
  const plan = providerId === 'chatgpt' || providerId === 'qwen-token-plan' || providerId === 'ollama';
  const save = async () => {
    setSaving(true); setError(''); setSaved(false);
    try { await backend.invoke('save_usage_rates', { providerId, modelId, rates: { inputPerMillion: Number(inputRate), outputPerMillion: Number(outputRate) } }); setSaved(true); }
    catch (reason) { setError(String(reason)); }
    finally { setSaving(false); }
  };
  return <details className="provider-usage"><summary>Usage & costs</summary>
    <p className="usage-note">This month (UTC), recorded in Melly for this connection. Other apps and earlier usage aren’t included.</p>
    {summary && <><dl className="usage-facts"><div><dt>Input tokens</dt><dd>{number(summary.input)}</dd></div><div><dt>Output tokens</dt><dd>{number(summary.output)}</dd></div>{!local && !plan && <><div><dt>Reported charges</dt><dd>{summary.reportedRequests > 0 ? dollars(summary.reportedCost) : 'Not reported'}</dd></div>{summary.estimatedRequests > 0 && <div><dt>Estimated charges</dt><dd>≈ {dollars(summary.estimatedCost)}</dd></div>}</>}</dl>
      {summary.missingUsage > 0 && <p className="usage-note">{summary.missingUsage} {summary.missingUsage === 1 ? 'request did' : 'requests did'} not report complete token counts.</p>}
      {!local && !plan && summary.unpricedRequests > 0 && <p className="usage-note">Cost is unknown for {summary.unpricedRequests} {summary.unpricedRequests === 1 ? 'request' : 'requests'}. These aren’t counted as free.</p>}
      {local && <p className="usage-note">No provider API charge for local inference.</p>}
      {plan && <p className="usage-note">Plan allowance: not exposed to Melly. Token counts cannot determine a reliable subscription percentage.</p>}
    </>}
    {providerId === 'openrouter' && <div className="account-usage"><button className="text-action" disabled={checking} onClick={() => { setChecking(true); void backend.invoke<{ usedUsd: number; limitUsd: number | null; remainingUsd: number | null }>('account_usage', { providerId }).then(setAccount).catch(reason => setError(String(reason))).finally(() => setChecking(false)); }}>{checking ? 'Checking…' : 'Refresh key usage'}</button>{account && <><p className="usage-note">OpenRouter reports {dollars(account.usedUsd)} used by this API key across apps.{account.remainingUsd !== null ? ` ${dollars(account.remainingUsd)} remains in its configured budget.` : ' No remaining key budget was reported.'}</p>{account.limitUsd !== null && account.limitUsd > 0 && account.remainingUsd !== null && <div className="context-bar" aria-label="API key budget used"><span style={{ width: `${Math.max(0, Math.min(100, (1 - account.remainingUsd / account.limitUsd) * 100))}%` }} /></div>}</>}</div>}
    {accountUrls[providerId] && <button className="text-action" onClick={() => { void backend.invoke('open_source', { source: { id: '', kind: 'web', title: 'Account usage', url: accountUrls[providerId], chatId: null, messageId: null, snippet: '' } }).catch(reason => setError(String(reason))); }}>View account usage ↗</button>}
    {!local && !plan && <details className="usage-rates"><summary>Set estimate rates for {modelId}</summary><p className="usage-note">USD per million tokens for future requests. Estimates exclude cache discounts, tool fees and taxes. Your provider’s bill is authoritative.</p><div className="usage-rate-fields"><label>Input<input aria-label="Input price per million tokens" type="number" min="0" max="10000" step="any" value={inputRate} onChange={event => { setInputRate(event.target.value); setSaved(false); }} /></label><label>Output<input aria-label="Output price per million tokens" type="number" min="0" max="10000" step="any" value={outputRate} onChange={event => { setOutputRate(event.target.value); setSaved(false); }} /></label></div><button className="button" disabled={saving || inputRate === '' || outputRate === ''} onClick={() => { void save(); }}>{saved ? 'Rates saved' : 'Save rates'}</button></details>}
    {error && <p className="dialog-error" role="alert">{error}</p>}
  </details>;
}
