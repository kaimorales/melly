// Give the view a few paint opportunities for a burst of provider text.
const paintInterval = 32;
export const streamRevealWindow = 120;
const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

export function advanceStream(text: string, visible: number, now: number, deadline: number) {
  const remaining = text.length - visible;
  if (remaining <= 3 || now >= deadline) return text.length;
  const frames = Math.max(2, Math.ceil((deadline - now) / paintInterval));
  const boundary = visible + Math.max(2, Math.ceil(remaining / frames));
  const segment = graphemes.segment(text).containing(boundary - 1);
  return segment ? segment.index + segment.segment.length : text.length;
}
