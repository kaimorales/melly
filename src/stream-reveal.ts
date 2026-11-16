// A short rolling buffer evens out provider packets without accumulating a
// typing-animation backlog. Every packet catches up within this window.
export const streamRevealWindow = 120;
export type StreamSchedule = { base: number; bursts: Array<{ length: number; at: number }> };
const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

export function queueStream(schedule: StreamSchedule, length: number, now: number) {
  schedule.bursts.push({ length, at: now });
  // Bound bookkeeping even when a provider emits hundreds of tiny deltas.
  // Coalescing only advances the older text; it never extends the deadline.
  if (schedule.bursts.length > 64) {
    const first = schedule.bursts.shift()!;
    schedule.bursts[0] = { length: first.length + schedule.bursts[0].length, at: first.at };
  }
}

export function flushStream(schedule: StreamSchedule, length: number) {
  schedule.base = length;
  schedule.bursts.length = 0;
  return length;
}

export function advanceStream(text: string, visible: number, now: number, schedule: StreamSchedule) {
  while (schedule.bursts.length && now - schedule.bursts[0].at >= streamRevealWindow) {
    schedule.base += schedule.bursts.shift()!.length;
  }
  if (!schedule.bursts.length || visible >= text.length) return text.length;
  let position = schedule.base;
  for (const burst of schedule.bursts) position += burst.length * Math.min(1, Math.max(0, (now - burst.at) / streamRevealWindow));
  const boundary = Math.min(text.length, Math.max(visible + 1, Math.floor(position)));
  // Segment only the unrevealed suffix, rather than rescanning a long answer.
  const segment = graphemes.segment(text.slice(visible)).containing(boundary - visible - 1);
  return segment ? visible + segment.index + segment.segment.length : text.length;
}
