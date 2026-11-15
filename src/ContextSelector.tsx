import { useState } from 'react';
import type { ContextLimit, ContextSelection } from './types';

const presets = [4096, 8192, 16384, 32768, 65536, 131072, 262144];
const maximum = 100_000_000;
const label = (tokens: number) => tokens % 1024 === 0 ? `${tokens / 1024}K` : tokens.toLocaleString();

export function ContextSelector({ selection, busy, onChange }: {
  selection: ContextSelection; busy: boolean; onChange: (limit: ContextLimit | null) => Promise<void>;
}) {
  const [custom, setCustom] = useState(false);
  const [tokens, setTokens] = useState('');
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  const ceiling = selection.ceiling ?? maximum;
  const choices = presets.filter(value => value <= ceiling);
  if (selection.ceiling !== null && !choices.includes(selection.ceiling)) choices.push(selection.ceiling);
  const current = selection.choice;
  if (typeof current === 'number' && !choices.includes(current)) choices.push(current);
  choices.sort((first, second) => first - second);
  const selected = current === null || (current === 'auto' && selection.modelDefault === null) ? 'default' : String(current);
  const save = async (limit: ContextLimit | null) => {
    setPending(true);
    setError('');
    try { await onChange(limit); setCustom(false); }
    catch (reason) { setError(String(reason)); }
    finally { setPending(false); }
  };

  return <div className="context-selection">
    <label className="context-choice">Context limit<select value={custom ? 'custom' : selected} disabled={busy || pending} onChange={event => {
      const next = event.target.value;
      if (next === 'custom') { setTokens(String(selection.effective ?? '')); setCustom(true); return; }
      void save(next === 'default' ? null : next === 'auto' ? 'auto' : Number(next));
    }}>
      <option value="default">{selection.modelDefault === null ? 'Auto' : `${label(selection.modelDefault)} (default)`}</option>
      {selection.modelDefault !== null && <option value="auto">Auto</option>}
      {choices.map(value => <option key={value} value={value}>{label(value)}{value > ceiling ? ' (capped by model)' : ''}</option>)}
      <option value="custom">Custom…</option>
    </select></label>
    {custom && <form className="context-custom" onSubmit={event => { event.preventDefault(); void save(Number(tokens)); }}>
      <input aria-label="Context limit in tokens" type="number" min={1024} max={ceiling} required value={tokens} disabled={busy || pending} onChange={event => setTokens(event.target.value)} />
      <button className="button" disabled={busy || pending}>Apply</button>
    </form>}
    {error && <p role="alert" className="context-omission">{error}</p>}
  </div>;
}
