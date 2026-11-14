import { forwardRef, useImperativeHandle, useLayoutEffect, useRef } from 'react';
import { backend, nativeApp } from './backend';

export type ApiKeyFieldHandle = { save: () => Promise<boolean> };
type Props = { providerId: string; theme: string; configured: boolean; onError: (message: string) => void };

let lastGeneration = 0;
function nextGeneration() {
  lastGeneration = Math.max(Date.now() * 1000, lastGeneration + 1);
  return lastGeneration;
}

// The button reserves space and makes the native field keyboard reachable. It
// never receives a key: AppKit owns the real secure text field over this slot.
export const ApiKeyField = forwardRef<ApiKeyFieldHandle, Props>(function ApiKeyField({ providerId, theme, configured, onError }, ref) {
  const anchor = useRef<HTMLButtonElement>(null);
  const currentGeneration = useRef(0);
  const ready = useRef<Promise<void>>(Promise.resolve());
  const errorRef = useRef(onError); errorRef.current = onError;
  const themeRef = useRef(theme); themeRef.current = theme;
  const syncRef = useRef<() => void>(() => {});

  useImperativeHandle(ref, () => ({
    save: async () => {
      if (!nativeApp) throw new Error('Use the desktop app to add a key.');
      const generation = currentGeneration.current;
      await ready.current;
      return backend.invoke<boolean>('save_provider_key_field', { generation, providerId });
    },
  }), [providerId]);

  useLayoutEffect(() => {
    if (!nativeApp || !anchor.current) return;
    const generation = nextGeneration();
    currentGeneration.current = generation;
    const element = anchor.current;
    const dialog = element.closest('.dialog');
    let disposed = false; let frame = 0;
    const rect = () => {
      const box = element.getBoundingClientRect();
      const clip = dialog?.getBoundingClientRect();
      return { left: box.left, top: box.top, width: box.width, height: box.height,
        visible: box.width > 0 && box.height > 0 && box.top >= 0 && box.bottom <= innerHeight && (!clip || (box.top >= clip.top + 2 && box.bottom <= clip.bottom - 2)) };
    };
    let opened = false;
    const sync = () => {
      if (disposed || !opened || frame) return;
      frame = requestAnimationFrame(() => { frame = 0; if (!disposed) void backend.invoke('update_provider_key_field', { generation, rect: rect(), theme: themeRef.current }).catch(() => {}); });
    };
    syncRef.current = sync;
    const observer = new ResizeObserver(sync); observer.observe(element); if (dialog) observer.observe(dialog);
    window.addEventListener('resize', sync); document.addEventListener('scroll', sync, true);
    const open = backend.invoke<void>('open_provider_key_field', { generation, providerId, rect: rect(), theme: themeRef.current, configured })
      .then(() => { opened = true; sync(); })
      .catch((error) => { if (!disposed) errorRef.current(String(error)); throw error; });
    ready.current = open;
    // Observe rejection even if no save is attempted.
    void open.catch(() => {});
    return () => {
      disposed = true; syncRef.current = () => {}; cancelAnimationFrame(frame); observer.disconnect();
      window.removeEventListener('resize', sync); document.removeEventListener('scroll', sync, true);
      void open.then(() => backend.invoke('close_provider_key_field', { generation })).catch(() => {});
    };
  }, [providerId]);

  useLayoutEffect(() => { syncRef.current(); }, [theme]);

  useLayoutEffect(() => {
    if (!nativeApp) return;
    const keyAction = (event: Event) => {
      const action = (event as CustomEvent<string>).detail;
      const dialog = anchor.current?.closest<HTMLElement>('.dialog');
      if (!dialog) return;
      if (action === 'close') { dialog.querySelector<HTMLButtonElement>('[aria-label="Close provider settings"]')?.click(); return; }
      if (action === 'save') { dialog.querySelector<HTMLButtonElement>('[data-provider-save]')?.click(); return; }
      if (action === 'next' || action === 'previous') {
        const controls = Array.from(dialog.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex="0"]'));
        const index = controls.indexOf(anchor.current!);
        controls[index + (action === 'next' ? 1 : -1)]?.focus();
      }
    };
    window.addEventListener('melly-native-key', keyAction);
    return () => window.removeEventListener('melly-native-key', keyAction);
  }, []);

  const focus = () => {
    if (!nativeApp) { errorRef.current('Use the desktop app to add a key.'); return; }
    const generation = currentGeneration.current;
    void ready.current.then(() => backend.invoke('focus_provider_key_field', { generation })).catch((error) => errorRef.current(String(error)));
  };
  return <div className="field"><span>API key</span><button ref={anchor} type="button" className={`native-key-slot${nativeApp ? ' native-key-slot-active' : ''}`} aria-label="API key" onFocus={nativeApp ? focus : undefined} onClick={focus}>{configured ? 'Enter a new key to replace' : 'Paste API key'}</button></div>;
});
