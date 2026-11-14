import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { Icon, ProviderIcon } from './icons';
import { discoverModels, modelCatalogKey, useModelCatalog } from './model-catalog';
import { modelLabel, providerName, routeLabel } from './SettingsView';
import type { ProviderView } from './types';

function usePicker(close: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => { (ref.current?.querySelector<HTMLElement>('[aria-selected="true"]') || ref.current?.querySelector<HTMLElement>('[role="option"],button'))?.focus(); }, []);
  const keydown = (event: KeyboardEvent) => {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); return; }
    if ((event.target as HTMLElement).tagName === 'INPUT' || !['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const items = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>('[role="option"]:not(:disabled)') || []);
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
    items[next]?.focus();
  };
  return { ref, keydown };
}

export function ProviderPicker({ providers, selected, theme, close, choose, settings }: { providers: ProviderView[]; selected: string; theme: string; close: () => void; choose: (provider: ProviderView) => Promise<void>; settings: () => void }) {
  const { ref, keydown } = usePicker(close);
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  const connected = providers.filter((provider) => (provider.configured || provider.id === 'mock') && !provider.disabledReason);
  return <div className="menu model-menu provider-menu" ref={ref} onKeyDown={keydown}>
    <div className="menu-label">Your providers</div><div role="listbox" aria-label="Providers">
      {connected.map((provider) => <button role="option" aria-selected={provider.id === selected} disabled={busy} className="model-option" key={provider.id} onClick={() => { setBusy(true); void choose(provider).catch((reason) => { setError(String(reason)); setBusy(false); }); }}>
        <ProviderIcon id={provider.id} theme={theme} small /><span className="model-option-copy"><span>{providerName(provider)}</span><small>{provider.id === 'mock' ? 'Mock · no billing' : `${routeLabel(provider)} · ${provider.status}`}</small></span>{provider.id === selected && <Icon name="check" />}
      </button>)}
    </div>{error && <div className="picker-error" role="alert">{error}</div>}
    <button className="menu-footer" onClick={settings}>Manage providers in Settings<Icon name="right" className="small" style={{ display: 'inline', marginLeft: 9, verticalAlign: 'middle' }} /></button>
  </div>;
}

export function ModelPicker({ provider, selected, close, choose }: { provider: ProviderView; selected: string; close: () => void; choose: (modelId: string) => Promise<void> }) {
  const { ref, keydown } = usePicker(close);
  const catalog = useModelCatalog(provider);
  const [manual, setManual] = useState(false); const [modelId, setModelId] = useState(''); const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  const key = modelCatalogKey(provider);
  useEffect(() => { void discoverModels(provider).catch(() => {}); }, [key, provider.configured]);
  useEffect(() => { if (manual) input.current?.focus(); }, [manual]);
  useEffect(() => {
    if (catalog.status === 'ready' && document.activeElement === document.body) (ref.current?.querySelector<HTMLElement>('[aria-selected="true"]') || ref.current?.querySelector<HTMLElement>('[role="option"]'))?.focus();
  }, [catalog.status]);
  const select = async (id: string) => { setBusy(true); setError(''); try { await choose(id); } catch (reason) { setError(String(reason)); setBusy(false); } };
  const configured = selected || provider.connection.modelId;
  const configuredListed = catalog.models.some((model) => model.id === configured);
  return <div className="menu model-menu" ref={ref} onKeyDown={keydown}>
    <div className="picker-heading"><span>{providerName(provider)}</span><span>{routeLabel(provider)}</span></div>
    <div className="picker-toolbar"><span className="menu-label">Models</span><button className="text-action" disabled={catalog.status === 'loading'} onClick={() => { void discoverModels(provider, true).catch(() => {}); }}>Refresh models</button></div>
    {!provider.configured && provider.id !== 'mock' && <div className="picker-status">Connect {providerName(provider)} in Settings.</div>}
    {catalog.status === 'loading' && <div className="picker-status" role="status">Loading models…</div>}
    {catalog.status === 'error' && <div className="picker-error" role="alert">{catalog.error}</div>}
    {catalog.status === 'ready' && !catalog.models.length && <div className="picker-status">No models found.</div>}
    <div role="listbox" aria-label="Models">
      {configured && !configuredListed && <button role="option" aria-selected={true} disabled={busy} className="model-option" onClick={() => { void select(configured); }}><span className="model-option-copy"><span>{provider.id === 'mock' ? modelLabel(provider, configured) : configured}</span></span><Icon name="check" /></button>}
      {catalog.models.map((model) => <button role="option" aria-selected={model.id === selected} disabled={busy} className="model-option" key={model.id} onClick={() => { void select(model.id); }}><span className="model-option-copy"><span>{model.name || model.id}</span>{model.name && model.name !== model.id && <small>{model.id}</small>}</span>{model.id === selected && <Icon name="check" />}</button>)}
    </div>
    {error && <div className="picker-error" role="alert">{error}</div>}
    {manual ? <form className="manual-model" onSubmit={(event) => { event.preventDefault(); if (modelId.trim()) void select(modelId.trim()); }}><label>Model ID<input ref={input} value={modelId} maxLength={200} placeholder="Enter a model ID" spellCheck={false} onChange={(event) => setModelId(event.target.value)} /></label><button className="button" disabled={!modelId.trim() || busy}>Use model</button></form> : provider.id !== 'mock' && <button className="menu-footer" onClick={() => setManual(true)}>Enter model ID</button>}
  </div>;
}
