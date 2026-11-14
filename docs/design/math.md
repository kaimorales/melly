# Equations in chat

Assistant responses use remark-math and locally bundled KaTeX 0.19.1. Inline math uses `$…$` or `\(...\)`; display math uses `$$…$$` or `\[…\]`. Math adopts the conversation's text color in both themes. Wide display equations scroll within the transcript. No new settings or toolbar are needed.

`math-markdown.ts` normalizes backslash delimiters before Markdown consumes them. It protects parser-identified code, HTML and link ranges; messages without those delimiters avoid an extra parse. A presentation plugin distinguishes ordinary price ranges and marks unfinished display blocks. Stored text and Copy response remain unchanged. Fenced `latex` is a literal, copyable code example, not executed math.

`MathFormula.tsx` shows readable source immediately. `math.ts` loads KaTeX CSS/fonts and creates `math.worker.ts` only when a complete equation appears. Only the math syntax parser joins startup JavaScript. MathML accompanies the visual HTML for accessibility. The generated HTML is the sole HTML insertion path: [KaTeX's documented security model](https://katex.org/docs/security) applies with `trust: false`, fresh macros for every equation and strict errors. Raw source and error messages never enter that path; no raw Markdown HTML is enabled.

Work is bounded to 8,192 input characters, 131,072 output characters, 500 macro expansions, 16-em user-specified sizes and 64 queued jobs. A two-second watchdog terminates stalled work; unmounted jobs cannot publish. Unsupported, malformed, oversized and incomplete math falls back to source. Rendering has no provider call, file tool, shell process, network font or credential access. It supports KaTeX's math commands, not full LaTeX document compilation.

The dependency override keeps micromark's optional HTML renderer on the same patched KaTeX version. Melly uses only that package's syntax parser; the unused HTML renderer is tree-shaken out of the startup bundle. Review the override when updating remark-math/micromark. MIT dependency notices and SIL OFL font notices are in `public/licenses/` and ship in the application.

UI regressions cover delimiters, MathML, local font loading, both themes, exact copying, literal code/prices, incomplete/oversized expressions, macro isolation, forbidden active content, worker startup failure, wide equations, streaming and Stop. Run `npm run test:ui -- math.spec.ts code-blocks.spec.ts`; the browser suite uses synthetic data, while native verification is recorded separately in `docs/verification.md`.
