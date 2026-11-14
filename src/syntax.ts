// This module is small and contains no grammar code. The worker starts only
// when a supported fenced code block actually needs highlighting.
export const MAX_CODE_LENGTH = 32_768;
export type SyntaxToken = { text: string; classes: string };
const families: Array<[string, string, string[]]> = [
  ['python', 'Python', ['py']], ['javascript', 'JavaScript', ['js', 'jsx']], ['typescript', 'TypeScript', ['ts', 'tsx']],
  ['rust', 'Rust', ['rs']], ['go', 'Go', ['golang']], ['bash', 'Shell', ['sh', 'shell', 'zsh']],
  ['json', 'JSON', ['jsonc']], ['xml', 'HTML', ['html', 'svg']], ['css', 'CSS', []],
  ['sql', 'SQL', []], ['yaml', 'YAML', ['yml']], ['java', 'Java', []],
  ['cpp', 'C++', ['c++', 'c', 'h', 'cc', 'cxx']], ['swift', 'Swift', []], ['markdown', 'Markdown', ['md']],
];
export function codeLanguage(value: string) {
  const name = value.toLowerCase();
  const family = families.find(([id, , aliases]) => id === name || aliases.includes(name));
  return family ? { id: family[0], label: family[1] } : { id: '', label: value.slice(0, 32) || 'Code' };
}

let worker: Worker | null = null;
let sequence = 0;
type Job = { done: (tokens: SyntaxToken[] | null) => void; timer: ReturnType<typeof setTimeout> };
const jobs = new Map<number, Job>();
function resetWorker() {
  worker?.terminate(); worker = null;
  const abandoned = [...jobs.values()]; jobs.clear();
  for (const job of abandoned) { clearTimeout(job.timer); job.done(null); }
}
export function highlightCode(code: string, language: string, done: Job['done']): () => void {
  if (!language || code.length > MAX_CODE_LENGTH || jobs.size >= 32) { done(null); return () => {}; }
  try {
    if (!worker) {
      worker = new Worker(new URL('./syntax.worker.ts', import.meta.url), { type: 'module' });
      worker.onmessage = (event: MessageEvent<{ id: number; tokens: SyntaxToken[] | null }>) => {
        const job = jobs.get(event.data.id); if (!job) return;
        jobs.delete(event.data.id); clearTimeout(job.timer); job.done(event.data.tokens);
      };
      worker.onerror = resetWorker;
      worker.onmessageerror = resetWorker;
    }
    const id = ++sequence;
    jobs.set(id, { done, timer: setTimeout(resetWorker, 2000) });
    worker.postMessage({ id, code, language });
    // Cancel publication, not the watchdog: abandoned grammar work must still
    // terminate if it stalls after its component has left the screen.
    return () => { const job = jobs.get(id); if (job) job.done = () => {}; };
  } catch { resetWorker(); done(null); return () => {}; }
}
