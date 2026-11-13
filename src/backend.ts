import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Backend } from './types';

export const nativeApp = isTauri();

// Native execution never falls back to the preview, including on command errors.
export const backend: Backend = nativeApp ? {
  invoke: (command, args) => invoke(command, args),
  listen: async (event, callback) => listen(event, ({ payload }) => callback(payload as never)),
} : {
  invoke: async (command, args) => (await import('./mock-backend')).mockBackend.invoke(command, args),
  listen: async (event, callback) => (await import('./mock-backend')).mockBackend.listen(event, callback),
};
