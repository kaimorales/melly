import { useState } from 'react';
import { backend } from './backend';
import { Diff, type DiffLine } from './InlineFileEdit';
import { Icon } from './icons';

export type CodingApproval = { id: string; label: string; path: string | null; diff: DiffLine[] };
type CodingResult = { action: string; reused?: boolean; path: string; diff: DiffLine[] }
  | { command: string; label?: string; passed: boolean; no_tests_found: boolean; exit_code: number | null; stdout: string; stderr: string };
export type CodingActivityState = { approval?: CodingApproval; results: CodingResult[] };

export function CodingActivity({ activity }: { activity: CodingActivityState }) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  async function resolve(id: string, approved: boolean) {
    setPending(true);
    setError('');
    try { await backend.invoke('confirm_coding_action', { id, approved }); }
    catch (reason) { setError(String(reason)); }
    finally { setPending(false); }
  }
  return <>
    {activity.results.map((result, index) => 'command' in result
      ? <details className="file-edit-card file-edit-saved" key={index}>
          <summary><Icon name={result.passed ? 'check' : 'close'} /><span>{result.no_tests_found ? 'No tests found' : `${result.label || 'Tests'} ${result.passed ? 'passed' : 'failed'}`}</span><Icon name="chevron" /></summary>
          <pre className="file-edit-diff">{[result.command, result.stdout, result.stderr].filter(Boolean).join('\n')}</pre>
        </details>
      : <details className="file-edit-card file-edit-saved" key={index}>
          <summary><Icon name="check" /><span>{result.reused ? 'Already completed' : result.action === 'Create' ? 'Created' : result.action === 'Delete' ? 'Deleted' : 'Edited'} <code>{result.path}</code></span><Icon name="chevron" /></summary>
          <Diff lines={result.diff} />
        </details>)}
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
