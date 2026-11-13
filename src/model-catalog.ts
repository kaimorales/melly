import { useSyncExternalStore } from 'react';
import { backend } from './backend';
import type { ModelInfo, ProviderView } from './types';

export interface ModelCatalog { status: 'idle' | 'loading' | 'ready' | 'error'; models: ModelInfo[]; error: string }
interface Entry { providerId: string; state: ModelCatalog; promise?: Promise<ModelInfo[]> }
const empty: ModelCatalog = { status: 'idle', models: [], error: '' };
const entries = new Map<string, Entry>();
const subscribers = new Set<() => void>();
const notify = () => subscribers.forEach((subscriber) => subscriber());
const subscribe = (subscriber: () => void) => { subscribers.add(subscriber); return () => { subscribers.delete(subscriber); }; };

export function modelCatalogKey(provider: ProviderView) {
  let endpoint = provider.connection.baseUrl.replace(/\/+$/, '');
  try { endpoint = new URL(endpoint).toString().replace(/\/+$/, ''); } catch { /* An invalid connection is rejected by Rust. */ }
  return `${provider.id}\n${endpoint}`;
}
export function useModelCatalog(provider: ProviderView) {
  const key = modelCatalogKey(provider);
  return useSyncExternalStore(subscribe, () => entries.get(key)?.state || empty);
}
export function invalidateModelCatalog(providerId: string) {
  for (const [key, entry] of entries) if (entry.providerId === providerId) entries.delete(key);
  notify();
}
export function discoverModels(provider: ProviderView, force = false): Promise<ModelInfo[]> {
  if ((!provider.configured && provider.id !== 'mock') || provider.disabledReason) return Promise.reject(new Error('Connect this provider in Settings to discover models.'));
  const key = modelCatalogKey(provider);
  const cached = entries.get(key);
  if (cached?.promise) return cached.promise;
  if (!force && cached?.state.status === 'ready') return Promise.resolve(cached.state.models);
  if (!force && cached?.state.status === 'error') return Promise.reject(new Error(cached.state.error));
  const entry: Entry = cached || { providerId: provider.id, state: empty };
  entry.state = { status: 'loading', models: entry.state.models, error: '' };
  entries.set(key, entry);
  // Bound endpoint history even when a custom connection is edited repeatedly.
  while (entries.size > 24) entries.delete(entries.keys().next().value!);
  entry.promise = backend.invoke<ModelInfo[]>('models', { providerId: provider.id }).then((result) => {
    const seen = new Set<string>();
    const models = result.filter((model) => {
      if (!model.id.trim() || seen.has(model.id)) return false;
      seen.add(model.id); return true;
    });
    if (entries.get(key) === entry) { entry.state = { status: 'ready', models, error: '' }; entry.promise = undefined; notify(); }
    return models;
  }).catch((reason) => {
    const error = reason instanceof Error ? reason.message : String(reason);
    if (entries.get(key) === entry) { entry.state = { status: 'error', models: entry.state.models, error }; entry.promise = undefined; notify(); }
    throw reason;
  });
  notify();
  return entry.promise;
}
