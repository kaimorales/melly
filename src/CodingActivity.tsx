import { useState } from 'react';
import { backend } from './backend';
import { compactPath, Diff, type DiffLine } from './InlineFileEdit';
import { Icon } from './icons';

export type CodingApproval = { id: string; label: string; path: string | null; diff: DiffLine[] };
type CodingResult = { action: string; reused?: boolean; path: string; diff: DiffLine[] }
  | { command: string; label?: string; passed: boolean; no_tests_found: boolean; exit_code: number | null; stdout: string; stderr: string };
export type CodingActivityState = { approval?: CodingApproval; results: CodingResult[] };

export function CodingActivity({ activity, settled }: { activity: CodingActivityState; settled: boolean }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  async function resolve(id: string, approved: boolean) {
    setPending(true);
    setError('');
    try { await backend.invoke('confirm_coding_action', { id, approved }); }
    catch (reason) { setError(String(reason)); }
    finally { setPending(false); }
  }
  // A failed first run may be followed by a passing rerun. The latest result
  // for each exact command determines the summary; every attempt stays inspectable.
  const checks = new Map(activity.results.flatMap(result => 'command' in result ? [[result.command, result] as const] : []));
  const paths = new Set(activity.results.flatMap(result => 'action' in result && !result.reused ? [result.path] : []));
  const problems = [...checks.values()].some(result => !result.passed || result.no_tests_found);
  const compact = settled && !activity.approval && activity.results.length > 0 && !problems;
  const label = [paths.size ? `${paths.size} ${paths.size === 1 ? 'file' : 'files'} changed` : '', checks.size ? 'Checks passed' : ''].filter(Boolean).join(' · ') || 'Changes already applied';
  const rows = activity.results.map((result, index) => 'command' in result
      ? <details className="file-edit-card file-edit-saved" key={index}>
          <summary><Icon name={result.passed ? 'check' : 'close'} /><span>{result.no_tests_found ? 'No tests found' : `${result.label || 'Tests'} ${result.passed ? 'passed' : 'failed'}`}</span><Icon name="chevron" /></summary>
          <pre className="file-edit-diff">{[result.command, result.stdout, result.stderr].filter(Boolean).join('\n')}</pre>
        </details>
      : <details className="file-edit-card file-edit-saved" key={index}>
          <summary><Icon name="check" /><span>{result.reused ? 'Already completed' : result.action === 'Create' ? 'Created' : result.action === 'Delete' ? 'Deleted' : 'Edited'} <code title={result.path}>{compactPath(result.path)}</code></span><Icon name="chevron" /></summary>
          <p className="file-edit-path">{result.path}</p><Diff lines={result.diff} />
        </details>);
  return <>
    {compact ? <details className="completed-work">
      <summary><Icon name="check" /><span>{label}</span><Icon name="chevron" /></summary>
      <div className="completed-work-details">{rows}</div>
    </details> : rows}
    {activity.approval && <section className="file-edit-card file-edit-review" aria-label="Permission requested">
      <header><Icon name="lock" />{activity.approval.label} {activity.approval.path && <code>{activity.approval.path}</code>}</header>
      {!!activity.approval.diff.length && <Diff lines={activity.approval.diff} />}
      {error && <span role="alert">{error}</span>}
      <div className="file-edit-actions">
        <button type="button" className="button quiet" disabled={pending} onClick={() => void resolve(activity.approval!.id, false)}>Decline</button>
        <button type="button" className="button primary" disabled={pending} onClick={() => void resolve(activity.approval!.id, true)}>Allow</button>
      </div>
    </section>}
  </>;
}
