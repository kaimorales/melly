import katex from 'katex';

self.onmessage = (event: MessageEvent<{ id: number; source: string; display: boolean }>) => {
  const { id, source, display } = event.data;
  let html: string | null = null;
  try {
    if (typeof source === 'string' && source.length <= 8192) {
      const result = katex.renderToString(source, {
        displayMode: display === true, output: 'htmlAndMathml', throwOnError: true,
        trust: false, strict: 'error', maxSize: 16, maxExpand: 500,
        macros: Object.create(null), globalGroup: false,
      });
      if (result.length <= 131072) html = result;
    }
  } catch { /* Keep malformed/unsupported equations as source; never log input. */ }
  self.postMessage({ id, html });
};
