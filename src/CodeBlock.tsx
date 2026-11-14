import { isValidElement, memo, useEffect, useRef, useState, type ReactNode } from 'react';
import { Icon } from './icons';
import { MathFormula } from './MathFormula';
import { codeLanguage, highlightCode, MAX_CODE_LENGTH, type SyntaxToken } from './syntax';

const CodeBlock = memo(function CodeBlock({ code, language }: { code: string; language: string }) {
  const info = codeLanguage(language);
  const latest = useRef({ code, language: info.id }); latest.current = { code, language: info.id };
  const [input, setInput] = useState(latest.current);
  const [colored, setColored] = useState<{ code: string; language: string; tokens: SyntaxToken[] } | null>(null);
  const [copyStatus, setCopyStatus] = useState<'idle' | 'copied' | 'error'>('idle');
  const throttle = useRef<ReturnType<typeof setTimeout> | null>(null);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    // At most one new input every 100 ms during streaming, without delaying text.
    if (throttle.current === null && (input.code !== code || input.language !== info.id)) throttle.current = setTimeout(() => {
      throttle.current = null; setInput(latest.current);
    }, 100);
  }, [code, info.id, input]);
  useEffect(() => highlightCode(input.code, input.language, (tokens) => {
    if (tokens) setColored({ ...input, tokens });
  }), [input]);
  useEffect(() => () => { if (throttle.current !== null) clearTimeout(throttle.current); if (copyTimer.current !== null) clearTimeout(copyTimer.current); }, []);
  const valid = code.length <= MAX_CODE_LENGTH && colored?.language === info.id && code.startsWith(colored.code);
  const copy = async () => {
    try { await navigator.clipboard.writeText(code); setCopyStatus('copied'); }
    catch { setCopyStatus('error'); }
    if (copyTimer.current !== null) clearTimeout(copyTimer.current);
    copyTimer.current = setTimeout(() => setCopyStatus('idle'), 1800);
  };
  return <section className="code-block" aria-label={`${info.label} code`}>
    <div className="code-header"><span>{info.label}</span><button className="copy-code" aria-label={copyStatus === 'copied' ? 'Code copied' : 'Copy code'} onClick={() => { void copy(); }}><Icon name={copyStatus === 'copied' ? 'check' : 'copy'} /><span role="status">{copyStatus === 'copied' ? 'Copied' : copyStatus === 'error' ? 'Copy failed' : 'Copy'}</span></button></div>
    <pre tabIndex={0}><code>{valid && colored ? <>{colored.tokens.map((token, index) => token.classes ? <span key={index} className={token.classes}>{token.text}</span> : token.text)}{code.slice(colored.code.length)}</> : code}</code></pre>
  </section>;
});

export function MarkdownCodeBlock({ children }: { children?: ReactNode }) {
  const child = Array.isArray(children) ? children[0] : children;
  if (!isValidElement<{ className?: string; children?: ReactNode; 'data-math-pending'?: boolean | string }>(child) || typeof child.props.children !== 'string') return <pre>{children}</pre>;
  if (child.props.className?.includes('math-display')) return <MathFormula source={child.props.children.trimEnd()} display pending={child.props['data-math-pending'] === true || child.props['data-math-pending'] === 'true'} />;
  const language = /(?:^|\s)language-([^\s]+)/.exec(child.props.className || '')?.[1] || '';
  return <CodeBlock code={child.props.children} language={language} />;
}
