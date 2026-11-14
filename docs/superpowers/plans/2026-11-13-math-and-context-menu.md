# Equations and chat actions

The user authorized these changes and routine implementation decisions. Work inline on main and push separate tested checkpoints.

## Design

Sidebar ellipsis and right-click menus contain Rename and Delete only. The header retains Move to folder as the keyboard-accessible alternative to dragging. Reuse the existing action component with a presentation flag; persistence, permissions and drag behavior do not change.

Use remark-math for Markdown parsing and locally bundled KaTeX 0.19.1 for equations. Support `$…$`, `$$…$$`, `\(...\)` and `\[…\]`. Backslash-delimiter normalization respects parser-identified code/HTML/link ranges. Keep ordinary prices and code literal; preserve the original stored/copied response. MathML accompanies visual output. Colors inherit the current theme, with bounded horizontal scrolling for wide equations.

Create the math worker and load styles/fonts only when math appears. Bound source, output, expansion count, element sizes and queued work; terminate stalled work. Render with `trust: false`, isolated macros and no raw error HTML. Incomplete, unsupported or oversized equations remain readable as source. No TeX executable, network fonts, file access or additional provider tool is introduced.

## Checkpoints

- [x] Update the existing menu/accessibility regressions, observe the old folder control failure, apply the focused `src/ChatActions.tsx` change, run chat-management tests and commit/push the menu alone.
- [x] Add `tests/ui/math.spec.ts` for delimiters, themes, code/prices, invalid/adversarial input, bounded work, local assets, macro isolation, exact copying and streaming/Stop. Observe the missing-equation failure.
- [x] Add `src/math-markdown.ts`, `src/MathFormula.tsx`, `src/math.ts`, `src/math.worker.ts` and lazy styles. Integrate the existing Markdown code/pre renderers, preserve code highlighting, add dependency/font notices and prebundle dev-only dependencies.
- [x] Run checks, the Rust/UI suites, dependency audit and the release build; verify its signature and native light/dark equations/menu. Preserve the current ChatGPT account through the existing reconnect flow if the ad-hoc rebuild requires it. Record actual evidence in `docs/verification.md` for the LaTeX checkpoint.
