import { memo, useEffect, useState, type ComponentPropsWithoutRef } from 'react';
import { renderMath } from './math';

export const MathFormula = memo(function MathFormula({ source, display = false, pending = false }: { source: string; display?: boolean; pending?: boolean }) {
  const [rendered, setRendered] = useState<{ source: string; display: boolean; html: string } | null>(null);
  useEffect(() => {
    if (pending) return;
    return renderMath(source, display, (html) => { if (html) setRendered({ source, display, html }); });
  }, [source, display, pending]);
  const html = !pending && rendered?.source === source && rendered.display === display ? rendered.html : null;
  return <span className={`math-expression ${display ? 'math-block' : 'math-inline'}`}>
    {/* Only bounded KaTeX output with trust:false reaches this sink, never raw source or errors. */}
    {html ? <span dangerouslySetInnerHTML={{ __html: html }} /> : <span className="math-source">{source}</span>}
  </span>;
});

type CodeProps = ComponentPropsWithoutRef<'code'> & { 'data-math-pending'?: boolean | string };
export function MarkdownInlineCode({ className, children, ...props }: CodeProps) {
  if (className?.includes('math-inline') || className?.includes('math-display')) {
    return <MathFormula source={String(children)} display={className.includes('math-display')} pending={props['data-math-pending'] === true || props['data-math-pending'] === 'true'} />;
  }
  return <code className={className}>{children}</code>;
}
