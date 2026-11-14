export const MAX_MATH_LENGTH = 8192;
type Result = (html: string | null) => void;
type Job = { done: Result; timer: ReturnType<typeof setTimeout> };
let worker: Worker | null = null;
let styles: Promise<unknown> | null = null;
let sequence = 0;
const jobs = new Map<number, Job>();
function reset() {
  worker?.terminate(); worker = null;
  const abandoned = [...jobs.values()]; jobs.clear();
  for (const job of abandoned) { clearTimeout(job.timer); job.done(null); }
}

export function renderMath(source: string, display: boolean, done: Result): () => void {
  if (!source.trim() || source.length > MAX_MATH_LENGTH) { done(null); return () => {}; }
  let cancelled = false; let cancelJob = () => {};
  styles ??= import('./math-styles').catch(() => { styles = null; throw new Error('Math styles unavailable'); });
  void styles.then(() => {
    if (cancelled) return;
    if (jobs.size >= 64) { done(null); return; }
    try {
      if (!worker) {
        worker = new Worker(new URL('./math.worker.ts', import.meta.url), { type: 'module' });
        worker.onmessage = (event: MessageEvent<{ id: number; html: string | null }>) => {
          const job = jobs.get(event.data.id); if (!job) return;
          jobs.delete(event.data.id); clearTimeout(job.timer);
          job.done(typeof event.data.html === 'string' && event.data.html.length <= 131072 ? event.data.html : null);
        };
        worker.onerror = reset; worker.onmessageerror = reset;
      }
      const id = ++sequence;
      jobs.set(id, { done, timer: setTimeout(reset, 2000) });
      worker.postMessage({ id, source, display });
      // Cancellation stops publication; the watchdog still bounds abandoned work.
      cancelJob = () => { const job = jobs.get(id); if (job) job.done = () => {}; };
    } catch { reset(); done(null); }
  }).catch(() => { if (!cancelled) done(null); });
  return () => { cancelled = true; cancelJob(); };
}
