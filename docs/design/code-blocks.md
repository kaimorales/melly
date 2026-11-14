# Code readability

Fenced code has a language label, Copy action and restrained syntax colors in both themes. Inline code stays inline. No line numbers, editor controls or execution actions are added to chat.

`MarkdownCodeBlock` is a stable Markdown renderer; `CodeBlock` is memoized. Text appears immediately while coloring updates at most every 100 ms during streaming. `syntax.ts` starts a worker on the first supported block, never on launch. Fifteen selected grammars are bundled separately, without automatic language detection or runtime downloads. Lowlight returns syntax trees; only text and classed spans are rendered, never HTML from a response.

Highlighting is limited to 32,768 UTF-16 code units, 6,000 tokens and 64 nesting levels. A two-second watchdog terminates stalled worker work, including cancelled jobs. Unknown languages, excessive blocks, worker failure and saturation fall back to the exact plain code. Cancellation prevents stale results from replacing current text. A colored prefix remains while new streamed text arrives. Copy preserves the exact code content, including its trailing newline.

The dev server pre-bundles worker dependencies to avoid a dependency-discovery reload on the first code block. The release worker is a separate local asset and needs no network access. The basic palette's sampled syntax tokens meet 4.5:1 contrast against the code background in the UI regression test. Native WKWebView appearance is verified separately.

Libraries: [lowlight 3.3.0](https://github.com/wooorm/lowlight), MIT; [highlight.js 11.11.2](https://github.com/highlightjs/highlight.js), BSD-3-Clause. The direct grammar dependency matches lowlight's supported 11.11 series to avoid duplicate highlighter versions. Notices for these and the small devlop dependency are included in `public/licenses/` and the app bundle.
