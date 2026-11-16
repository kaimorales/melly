import { useId, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import { Icon } from './icons';
import type { Message } from './types';
import './thinking.css';

// Clockwise perimeter, with a quieter pulse at the center. CSS owns all motion.
const steps = [0, 1, 2, 7, 0, 3, 6, 5, 4];
export function ThinkingGrid({ active }: { active: boolean }) {
  return <span className="thinking-grid" data-active={active} aria-hidden="true">
    {steps.map((step, index) => <span key={index} style={{ '--step': step } as CSSProperties} />)}
  </span>;
}

export function Thinking({ message, activity }: { message: Message; activity?: string }) {
  const reasoning = message.reasoning || '';
  const streaming = message.status === 'streaming';
  const active = streaming && (message.phase === 'thinking' || (!message.content && !!reasoning));
  const [expanded, setExpanded] = useState<boolean | null>(null);
  const open = expanded ?? active;
  const id = useId();
  const body = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  useLayoutEffect(() => {
    if (open && active && follow.current && body.current) body.current.scrollTop = body.current.scrollHeight;
  }, [reasoning.length, open, active]);
  if (!reasoning) {
    return streaming && (!message.content || activity) ? <div className="response-activity" role="status"><ThinkingGrid active /><span>{activity || 'Working'}</span></div> : null;
  }
  return <section className="thinking" data-thinking={active}>
    <button className="thinking-toggle" aria-expanded={open} aria-controls={id} onClick={() => { setExpanded(!open); follow.current = true; }}>
      {active && <ThinkingGrid active />}<span>Thinking</span><Icon name="chevron" className="thinking-chevron" />
      {active && <span className="sr-only">in progress</span>}
    </button>
    <div id={id} ref={body} className="thinking-body" hidden={!open} onScroll={() => {
      const element = body.current;
      if (element) follow.current = element.scrollHeight - element.scrollTop - element.clientHeight < 24;
    }}>{reasoning}</div>
    {streaming && activity && !active && <div className="response-activity" role="status"><ThinkingGrid active /><span>{activity}</span></div>}
  </section>;
}
