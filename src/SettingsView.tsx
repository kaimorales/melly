import { useEffect, useRef, useState } from 'react';
import { backend, nativeApp } from './backend';
import { ApiKeyField, type ApiKeyFieldHandle } from './ApiKeyField';
import { Dialog } from './dialogs';
import { Icon, ProviderIcon } from './icons';
import { discoverModels, invalidateModelCatalog, useModelCatalog } from './model-catalog';
import type { Bootstrap, Connection, ProviderView, Settings } from './types';

export const providerName = (provider: ProviderView) => provider.id === 'mock' ? 'Demo' : provider.id === 'muse' ? 'Meta Muse Code' : provider.id === 'custom' ? 'Custom' : provider.id === 'moonshot' ? 'Kimi / Moonshot' : provider.name;
export function routeLabel(provider: ProviderView) {
  if (provider.id === 'mock') return 'Mock';
  if (provider.id === 'chatgpt') return 'Plan';
  if (provider.id === 'qwen-token-plan') return 'Token Plan';
  if (provider.id === 'qwen-standard') return 'Standard API';
  return 'API key';
}
export function modelLabel(provider: ProviderView, model: string) {
  if (provider.id === 'mock') return 'Demo';
  if (model.startsWith('claude-sonnet')) return 'Claude Sonnet';
  if (model.startsWith('claude-opus')) return 'Claude Opus';
  return model || providerName(provider);
}

function ProviderDialog({ initialId, data, theme, onClose, refresh }: { initialId: string; data: Bootstrap; theme: string; onClose: () => void; refresh: () => Promise<Bootstrap> }) {
  const [providerId, setProviderId] = useState(initialId);
  const provider = data.providers.find((entry) => entry.id === providerId)!;
  const qwen = providerId.startsWith('qwen');
  const configureModels = providerId !== 'chatgpt' || provider.configured;
  const [connection, setConnection] = useState<Connection>({ ...provider.connection });
  const [error, setError] = useState(''); const [notice, setNotice] = useState('');
  const [busy, setBusy] = useState(false); const [pending, setPending] = useState(providerId === 'chatgpt' && provider.status === 'Sign-in pending');
  const keyField = useRef<ApiKeyFieldHandle>(null);
  const catalog = useModelCatalog(provider);
  useEffect(() => { setConnection({ ...provider.connection }); setError(''); setNotice(''); }, [providerId]); // A route has its own endpoint, model and key.
  useEffect(() => {
    let disposed = false; let remove: (() => void) | undefined;
    backend.listen<unknown>('connection-update', (event) => {
      const update = event && typeof event === 'object' ? event as Record<string, unknown> : {};
      const status = String(update.status || '');
      if (/connected|error|fail|cancel|timeout/i.test(status) || update.error) setPending(false);
      if (status === 'signing-in') setPending(true);
      if (status === 'error' || update.error) { setError(String(update.message || update.error || 'Sign-in did not complete.')); setNotice(''); }
      else if (update.message) setNotice(String(update.message));
      void refresh();
    }).then((unlisten) => { if (disposed) unlisten(); else remove = unlisten; });
    return () => { disposed = true; remove?.(); };
  }, []);
  const run = async (operation: () => Promise<void>) => { setBusy(true); setError(''); setNotice(''); try { await operation(); } catch (reason) { setError(String(reason)); } finally { setBusy(false); } };
  const persistConnection = async () => {
    await backend.invoke('save_connection', { connection });
    if (data.settings.defaultProvider === providerId && data.settings.defaultModel !== connection.modelId) await backend.invoke('save_settings', { settings: { ...data.settings, defaultModel: connection.modelId } });
  };
  const save = async () => {
    await persistConnection();
    const saved = providerId !== 'chatgpt' && nativeApp ? await keyField.current!.save() : false;
    if (saved) invalidateModelCatalog(providerId);
    await refresh(); setNotice(saved ? 'Key saved · checked when used' : 'Connection saved.');
  };
  const connect = () => run(async () => {
    setPending(true); setNotice('Complete sign-in in your browser.');
    try { await backend.invoke('connect_chatgpt'); await refresh(); }
    catch (reason) { setPending(false); throw reason; }
  });
  return <Dialog title={qwen ? 'QwenCloud' : providerName(provider)} closeLabel="Close provider settings" onClose={onClose} icon={<ProviderIcon id={providerId} theme={theme} />}>
    {qwen && <div className="theme-options" role="group" aria-label="QwenCloud billing route">
      <button disabled={busy} aria-pressed={providerId === 'qwen-standard'} onClick={() => setProviderId('qwen-standard')}>Standard API</button>
      <button disabled={busy} aria-pressed={providerId === 'qwen-token-plan'} onClick={() => setProviderId('qwen-token-plan')}>Token Plan</button>
    </div>}
    <div className="connection-status"><strong>{providerId === 'qwen-token-plan' ? 'Token Plan subscription' : providerId === 'qwen-standard' ? 'Standard API · metered billing' : provider.billingRoute}</strong><span>{pending ? 'Sign-in pending' : provider.status}</span></div>
    {provider.disabledReason ? <p style={{ marginTop: 18 }}>{provider.disabledReason} No connection is available.</p> : providerId === 'mock' ? <p style={{ marginTop: 18 }}>A synthetic conversation for trying Melly. No provider is contacted and no account is needed.</p> : <>
      {configureModels && <>
      {providerId === 'ollama' && <p className="billing-note">Plan credits first; purchased credits can follow.</p>}
      {providerId === 'custom' && <label className="field">Base URL<input aria-label="Base URL" value={connection.baseUrl} placeholder="https://api.example.com/v1" spellCheck={false} onChange={(event) => setConnection({ ...connection, baseUrl: event.target.value })} /><small>Each endpoint has its own key.</small></label>}
      <label className="field">Model<input aria-label="Model" value={connection.modelId} spellCheck={false} placeholder="Enter a model name" onChange={(event) => setConnection({ ...connection, modelId: event.target.value })} /></label>
      <button className="button" disabled={busy || !provider.configured || (providerId === 'custom' && connection.baseUrl !== provider.connection.baseUrl)} onClick={() => run(async () => {
        if (connection.modelId !== provider.connection.modelId || connection.baseUrl !== provider.connection.baseUrl) await persistConnection();
        const fresh = await refresh();
        await discoverModels(fresh.providers.find((entry) => entry.id === providerId)!, true); setNotice('Model list refreshed.');
      })}>Refresh models</button>
      {catalog.status === 'loading' && <p role="status" className="model-discovery-status">Loading models…</p>}
      {catalog.status === 'error' && !error && <div className="dialog-error" role="alert">{catalog.error}</div>}
      {catalog.models.length > 0 && <div className="models-discovered" aria-label="Discovered models">{catalog.models.map((model) => <button key={model.id} onClick={() => setConnection({ ...connection, modelId: model.id })}>{model.name || model.id}</button>)}</div>}
      {providerId !== 'chatgpt' && <ApiKeyField key={`${providerId}-${provider.configured && connection.baseUrl === provider.connection.baseUrl}`} ref={keyField} providerId={providerId} theme={theme} configured={provider.configured && connection.baseUrl === provider.connection.baseUrl} onError={setError} />}
      </>}
      {error && <div className="dialog-error" role="alert">{error}</div>}
      {notice && <p role="status" style={{ marginTop: 14 }}>{notice}</p>}
      <div className="dialog-actions spread">
        {providerId === 'chatgpt' ? (provider.configured || pending ? <button className="button" disabled={busy} onClick={() => run(async () => { await backend.invoke('disconnect_chatgpt'); invalidateModelCatalog(providerId); setPending(false); await refresh(); setNotice(pending ? 'Sign-in cancelled.' : 'Disconnected.'); })}>{pending ? 'Cancel sign-in' : 'Disconnect'}</button> : <button className="button" disabled={busy} onClick={connect}>Continue with ChatGPT</button>) : provider.configured ? <button className="button" disabled={busy} onClick={() => run(async () => { await backend.invoke('remove_provider_key', { providerId }); invalidateModelCatalog(providerId); await refresh(); setNotice('Key removed.'); })}>Remove key</button> : <span />}
        {configureModels && <button data-provider-save className="button primary" disabled={busy || !connection.modelId.trim() || !connection.baseUrl.trim()} onClick={() => run(save)}>Save</button>}
      </div>
    </>}
  </Dialog>;
}

export function SettingsView({ data, theme, onSettings, refresh, onSeed, report }: { data: Bootstrap; theme: string; onSettings: (settings: Settings) => Promise<void>; refresh: () => Promise<Bootstrap>; onSeed: () => Promise<void>; report: (message: string) => void }) {
  const [providerId, setProviderId] = useState<string | null>(null);
  const [prompt, setPrompt] = useState(data.settings.systemPrompt); const [promptSaved, setPromptSaved] = useState(false);
  const update = (patch: Partial<Settings>) => { void onSettings({ ...data.settings, ...patch }).catch((reason) => report(String(reason))); };
  const providers = [...data.providers.filter((provider) => provider.id !== 'mock' && provider.id !== 'qwen-token-plan'), ...data.providers.filter((provider) => !nativeApp && provider.id === 'mock')];
  return <div className="settings-scroll"><div className="settings-content">
    <section className="settings-section" aria-labelledby="providers-title"><h2 id="providers-title">Providers</h2><div className="provider-group">
      {providers.map((provider) => {
        const name = provider.id === 'qwen-standard' ? 'QwenCloud' : providerName(provider);
        const route = provider.id === 'qwen-standard' ? 'Standard API or Token Plan' : provider.id === 'muse' ? 'Plan' : provider.id === 'mock' ? 'Synthetic preview' : provider.id === 'moonshot' ? 'General API key' : provider.id === 'ollama' ? 'API key · plan or usage credits' : provider.id === 'custom' ? 'API key · OpenAI-compatible' : routeLabel(provider);
        return <div className="provider-row" key={provider.id}>
          <ProviderIcon id={provider.id} theme={theme} /><div className="provider-copy"><div className="provider-name">{name}</div><div className="provider-description">{route}{provider.id !== 'custom' && <><span className="provider-separator">·</span>{provider.id === 'qwen-standard' ? (data.providers.some((entry) => entry.id.startsWith('qwen') && entry.configured) ? 'Key saved' : 'Not connected') : provider.status}</>}</div></div>
          <button className={`provider-action ${provider.configured || provider.disabledReason ? 'provider-action-quiet' : ''}`} aria-label={`Configure ${name}`} onClick={() => setProviderId(provider.id)}>{provider.disabledReason ? 'Details' : provider.id === 'chatgpt' && !provider.configured ? 'Connect' : provider.id === 'custom' || provider.id === 'qwen-standard' ? 'Configure' : provider.configured ? 'Manage' : 'Add key'}{(provider.configured || provider.disabledReason) && <Icon name="chevron" />}</button>
        </div>;
      })}
    </div></section>
    <section className="settings-section" aria-labelledby="appearance-title"><h2 id="appearance-title">Appearance</h2><div className="settings-group">
      <div className="settings-row"><span className="settings-row-label">Theme</span><div className="theme-options" role="group" aria-label="Theme">{['system', 'light', 'dark'].map((value) => <button key={value} aria-pressed={data.settings.theme === value} onClick={() => update({ theme: value })}>{value[0].toUpperCase() + value.slice(1)}</button>)}</div></div>
      <div className="settings-row"><span className="settings-row-label">Text size</span><div className="theme-options" role="group" aria-label="Text size">{[14, 16, 18].map((value) => <button key={value} aria-pressed={data.settings.fontSize === value} onClick={() => update({ fontSize: value })}>{value}</button>)}</div></div>
      <div className="settings-row"><span className="settings-row-label">Reduce motion</span><button role="switch" className="switch" aria-label="Reduce motion" aria-checked={data.settings.reduceMotion} onClick={() => update({ reduceMotion: !data.settings.reduceMotion })}><span /></button></div>
    </div></section>
    <section className="settings-section" aria-labelledby="conversation-title"><h2 id="conversation-title">Conversation</h2><div className="settings-group">
      <div className="settings-row"><span className="settings-row-label">Send message</span><div className="theme-options" role="group" aria-label="Send message shortcut"><button aria-pressed={data.settings.sendBehavior === 'enter'} onClick={() => update({ sendBehavior: 'enter' })}>Enter</button><button aria-pressed={data.settings.sendBehavior === 'cmd-enter'} onClick={() => update({ sendBehavior: 'cmd-enter' })}>⌘ Enter</button></div></div>
      <div className="settings-row"><label className="settings-row-label" htmlFor="system-prompt">System prompt</label></div>
      <textarea id="system-prompt" className="settings-textarea" value={prompt} onChange={(event) => { setPrompt(event.target.value); setPromptSaved(false); }} />
      <div className="settings-save"><button className="button" disabled={prompt === data.settings.systemPrompt} onClick={() => { void onSettings({ ...data.settings, systemPrompt: prompt }).then(() => setPromptSaved(true)).catch((reason) => report(String(reason))); }}>Save prompt</button>{promptSaved && <span role="status">Saved</span>}</div>
    </div></section>
    <section className="settings-section" aria-labelledby="search-title"><h2 id="search-title">Web search</h2><div className="settings-group"><div className="settings-row"><div><div className="settings-row-label">External search</div><div className="settings-row-description">{data.searchConfigured ? 'Tavily · search key saved' : 'Tavily · optional for providers without built-in search'}</div></div><div className="search-key-actions">{data.searchConfigured && <button className="button quiet" onClick={() => { void backend.invoke('remove_search_key').then(refresh).then(() => report('Search key removed.')).catch((reason) => report(String(reason))); }}>Remove key</button>}<button className="button" onClick={() => { void backend.invoke<boolean>('enter_search_key').then(async (saved) => { await refresh(); if (saved) report('Search key saved in Keychain.'); }).catch((reason) => report(String(reason))); }}>{data.searchConfigured ? 'Replace key' : 'Add key'}</button></div></div></div><p className="settings-note">Search runs only when you enable it for a message. External search uses its own billing.</p></section>
    {!nativeApp && <section className="settings-section" aria-labelledby="demo-title"><h2 id="demo-title">Try Melly</h2><div className="settings-group"><div className="settings-row"><div><div className="settings-row-label">Example conversations</div><div className="settings-row-description">Synthetic chats, folder references and citations</div></div><button className="button" onClick={() => { void onSeed().catch((reason) => report(String(reason))); }}>Try an example</button></div></div></section>}
  </div>{providerId && <ProviderDialog key={providerId} initialId={providerId} data={data} theme={theme} onClose={() => setProviderId(null)} refresh={refresh} />}</div>;
}
