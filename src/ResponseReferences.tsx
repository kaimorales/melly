import { Icon } from './icons';
import type { Source } from './types';
export function ResponseReferences({ sources, onSource }: { sources: Source[]; onSource: (source: Source) => void }) {
  const unique = [...new Map(sources.map(source => [source.id || `${source.kind}:${source.title}:${source.url}`, source])).values()];
  if (!unique.length) return null;
  return <details className="response-references">
    <summary><Icon name="chevron" /><span>{unique.length} {unique.length === 1 ? 'source' : 'sources'}</span></summary>
    <div className="source-row">{unique.map(source => <button key={source.id || `${source.kind}:${source.title}:${source.url}`} className="source" aria-label={`Open source ${source.title}`} title={source.snippet.slice(0, 300) || source.title} onClick={() => onSource(source)}>
      <Icon name={source.kind === 'file' ? 'folder' : source.kind === 'chat' ? 'message' : 'globe'} /><span>{source.title}</span><Icon name={source.kind === 'web' ? 'external' : 'right'} />
    </button>)}</div>
  </details>;
}
