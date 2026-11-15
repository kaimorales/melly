import { useEffect, useState, type FormEvent } from 'react';
import { backend } from './backend';
import { Dialog } from './dialogs';
import type { Bootstrap, ChatModel, CompatibleConnection, ModelInfo } from './types';

type ModelDraft = { id: string; name: string; context: string; defaultContext: string; existing: boolean };

export function CompatibleConnectionDialog({ initial, close, changed, setDefault, defaultRoute }: {
  initial: CompatibleConnection | null; close: () => void; changed: () => Promise<Bootstrap>;
  setDefault: (connectionId: string, modelId: string) => Promise<void>; defaultRoute: ChatModel;
}) {
  const [connection, setConnection] = useState(initial);
  const [name, setName] = useState(initial?.name || '');
  const [baseUrl, setBaseUrl] = useState(initial?.baseUrl || '');
  const [renaming, setRenaming] = useState(false);
  const [discovered, setDiscovered] = useState<ModelInfo[] | null>(null);
  const [draft, setDraft] = useState<ModelDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [hasKey, setHasKey] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const id = connection?.id;
  const defaultModel = defaultRoute.providerId === id ? defaultRoute.modelId : null;

  useEffect(() => {
    if (!id) return;
    let current = true;
    void backend.invoke<boolean>('has_compatible_key', { providerId: id }).then(value => { if (current) setHasKey(value); }).catch(reason => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, [id]);

  const run = async (operation: () => Promise<void>) => {
    setBusy(true);
    setError('');
    setNotice('');
    try { await operation(); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };
  const save = async (models: ModelInfo[], textToolCalls = connection?.textToolCalls ?? false) => {
    const saved = await backend.invoke<string>('save_compatible_connection', { id: id || null, name, baseUrl, models, textToolCalls });
    const fresh = await changed();
    const updated = fresh.compatibleConnections?.find(item => item.id === saved);
    setConnection(updated || { id: saved, name: name.trim(), baseUrl: baseUrl.trim(), models, textToolCalls });
    if (updated) { setName(updated.name); setBaseUrl(updated.baseUrl); }
  };
  const edit = (model?: ModelInfo, existing = false) => setDraft({
    id: model?.id || '', name: model?.name || '', context: model?.contextWindow?.toString() || '', defaultContext: model?.defaultContextWindow?.toString() || '', existing,
  });
  const saveModel = (event: FormEvent) => {
    event.preventDefault();
    if (!draft || !connection) return;
    void run(async () => {
      const model = { id: draft.id.trim(), name: draft.name.trim() || draft.id.trim(), contextWindow: draft.context ? Number(draft.context) : null, defaultContextWindow: draft.defaultContext ? Number(draft.defaultContext) : null };
      const models = connection.models.some(item => item.id === model.id)
        ? connection.models.map(item => item.id === model.id ? model : item)
        : [...connection.models, model];
      await save(models);
      setDraft(null);
    });
  };
  const test = () => {
    void run(async () => {
      setDiscovered(await backend.invoke<ModelInfo[]>('models', { providerId: id }));
      setNotice('Connection verified.');
    });
  };
  const changeKey = () => {
    void run(async () => {
      if (await backend.invoke<boolean>('enter_compatible_key', { providerId: id })) setHasKey(true);
    });
  };

  return <Dialog title={connection ? connection.name : 'Add custom connection'} closeLabel="Close provider settings" onClose={close} className="custom-connection-dialog">
    {!connection ? <form onSubmit={event => { event.preventDefault(); void run(() => save([])); }}>
      <label className="field">Connection name<input data-autofocus maxLength={100} required value={name} onChange={event => setName(event.target.value)} /></label>
      <label className="field">Base URL<input required value={baseUrl} maxLength={1024} spellCheck={false} placeholder="http://127.0.0.1:8080/v1" onChange={event => setBaseUrl(event.target.value)} /></label>
      <div className="custom-connection-actions"><button className="button primary" disabled={busy || !name.trim() || !baseUrl.trim()}>Add connection</button></div>
    </form> : <>
      <div className="custom-connection-address">{connection.baseUrl}</div>
      {renaming ? <form onSubmit={event => {
        event.preventDefault();
        void run(async () => { await save(connection.models); setRenaming(false); });
      }}>
        <label className="field">Connection name<input maxLength={100} required value={name} onChange={event => setName(event.target.value)} /></label>
        <div className="custom-connection-actions">
          <button type="button" className="text-action" disabled={busy} onClick={() => { setName(connection.name); setRenaming(false); }}>Cancel</button>
          <button className="button" disabled={busy || !name.trim()}>Save name</button>
        </div>
      </form> : <div className="custom-connection-actions">
        <button className="button" disabled={busy} onClick={test}>Test connection</button>
        <button className="text-action" disabled={busy} onClick={changeKey}>{hasKey ? 'Replace API key' : 'Add API key'}</button>
        {hasKey && <button className="text-action" disabled={busy} onClick={() => { void run(async () => {
          await backend.invoke('remove_provider_key', { providerId: id }); setHasKey(false);
        }); }}>Remove key</button>}
        <button className="text-action" disabled={busy} onClick={() => setRenaming(true)}>Rename</button>
      </div>}
      <details className="custom-connection-compatibility">
        <summary>Compatibility</summary>
        <label className="field">Tool calls<select aria-label="Tool calls" disabled={busy} value={connection.textToolCalls ? 'text' : 'native'} onChange={event => { void run(() => save(connection.models, event.target.value === 'text')); }}>
          <option value="native">Native functions</option>
          <option value="text">Text fallback</option>
        </select></label>
      </details>
      <section className="custom-connection-models" aria-label="Models">
        <div className="custom-connection-model-header"><h3>Models</h3>
          {!draft && <button className="text-action" disabled={busy} onClick={() => edit()}>Add model</button>}
        </div>
        {discovered?.length ? <label className="field">Discovered models<select value="" disabled={busy} onChange={event => {
          const candidate = discovered.find(item => item.id === event.target.value);
          if (candidate) {
            const saved = connection.models.find(item => item.id === candidate.id);
            edit(saved || candidate, !!saved);
          }
        }}>
          <option value="">Choose a model</option>
          {[...new Map(discovered.map(item => [item.id, item])).values()].map(item => <option key={item.id} value={item.id}>{item.name || item.id}</option>)}
        </select></label> : null}
        {connection.models.map(item => <div className="custom-connection-model" key={item.id}>
          <button className="text-action" disabled={busy} title={item.id} onClick={() => edit(item, true)}>{item.name}</button>
          {defaultModel === item.id ? <span>Default</span> : <button className="text-action" disabled={busy} onClick={() => { void run(() => setDefault(connection.id, item.id)); }}>Use as default</button>}
        </div>)}
        {draft && <form onSubmit={saveModel} aria-label="Model details">
          <label className="field">Model ID<input value={draft.id} readOnly={draft.existing} maxLength={200} required spellCheck={false} onChange={event => setDraft({ ...draft, id: event.target.value })} /></label>
          <label className="field">Display name<input value={draft.name} maxLength={200} onChange={event => setDraft({ ...draft, name: event.target.value })} /></label>
          <label className="field">Context limit (optional)<input type="number" min={1024} max={100000000} value={draft.context} onChange={event => setDraft({ ...draft, context: event.target.value })} /></label>
          <label className="field">Default context (optional)<input type="number" min={1024} max={draft.context || 100000000} value={draft.defaultContext} onChange={event => setDraft({ ...draft, defaultContext: event.target.value })} /></label>
          <div className="custom-connection-actions">
            <button type="button" className="text-action" disabled={busy} onClick={() => setDraft(null)}>Cancel</button>
            <button className="button primary" disabled={busy || !draft.id.trim()}>Save model</button>
          </div>
        </form>}
      </section>
    </>}
    {error && <p role="alert" className="dialog-error">{error}</p>}
    {notice && <p role="status" className="usage-note">{notice}</p>}
  </Dialog>;
}
