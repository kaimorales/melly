# Ollama typography for v3 static prototypes

Source: official Ollama at commit [5632619189ab493c70bd3f9435214b8788cc1c80](https://github.com/ollama/ollama/commit/5632619189ab493c70bd3f9435214b8788cc1c80). Read 2026-11-13. Lockfile resolves Tailwind **4.1.11** and Typography **0.5.16**. Values below assume normal browser zoom, 16px root size, and desktop width ≥640px. These are source-derived CSS values, not pixel measurements of the supplied screenshots. Root inspected the supplied dark references; this researcher could not access their protected temporary paths.

## Main correction

**Do not make settings names or sidebar rows semibold.** Most settings names and descriptions are weight **400**. Buttons, time-group captions, and toolbar title are **500**. The current source is slightly smaller than the initial “15px everywhere” visual estimate.

| Surface | Family | CSS size / line height | Weight | Geometry and colors |
|---|---|---:|---:|---|
| Base body | System sans below | 16px / 24px | 400 | No global tracking change |
| Sidebar navigation: Apps/Chat/Settings | System sans | 14px / 20px | 400 | 8px vertical + horizontal row padding; 36px total height; 20px icon; 12px icon/text gap; dark text neutral-100 |
| Sidebar chat titles | System sans | 14px / 20px | 400 | 8px padding; 36px total row height; dark text neutral-400; no selected-row weight change |
| Sidebar Today/This week/Older | System sans | 12px / 16px | 500 | 4px vertical padding, 8px horizontal; dark neutral-500; title case, not uppercase |
| Toolbar Settings title | Rounded stack below | 16px / 24px inherited | 500 | 52px bar; 24px left padding when sidebar is open |
| Composer text/placeholder | System sans | 16px / 24px | 400 | 20px horizontal inset; placeholder neutral-500 in dark; text white |
| Closed model pill | System sans | 16px / 24px | 400 | 14px horizontal + 6px vertical padding =36px height; 12px chevron; 6px gap; neutral-100 text |
| Model popup rows/search | System sans | 15px / 22.5px inherited | 400 | Row padding12px horizontal/8px vertical; popup width256px; 16px radius |
| Normal settings name (“Cloud”, etc.) | System sans | 14px / 24px | **400** | Dark white; does not inherit the Field component's direct-child medium rule |
| Settings description | System sans | 14px / 24px | **400** | Dark zinc-400 (approximately #a1a1aa); ordinary sentence text |
| Signed-in account name | System sans | 14px / 24px | 500 | Explicit exception in Settings.tsx |
| Standard settings action button | System sans | 14px / 24px | 500 | Default desktop padding11px horizontal/5px vertical plus1px border =36px high; 8px radius |
| Settings text input | System sans | 14px / 24px | 400 | Same default36px height; 8px radius |
| Assistant prose | System sans | 16px / 28px | 400 | Typography plugin base; bold/strong and markdown headings overridden to600 |
| User bubble | System sans | 16px / 24px | 400 | “leading-normal”1.5; 16px horizontal/8px vertical padding |

Sidebar sources: [AppSidebar.tsx:7–31](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/AppSidebar.tsx#L7-L31), [ChatSidebar.tsx:255–267](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ChatSidebar.tsx#L255-L267), [311–317](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ChatSidebar.tsx#L311-L317).
Title: [layout.tsx:74–85](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/layout/layout.tsx#L74-L85). Composer: [ChatForm.tsx:858–885](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ChatForm.tsx#L858-L885). Model picker: [ModelPicker.tsx:163–196](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ModelPicker.tsx#L163-L196), [313–321](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ModelPicker.tsx#L313-L321).
Settings typography: [fieldset.tsx:49–99](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ui/fieldset.tsx#L49-L99), [Settings.tsx:509–518](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/Settings.tsx#L509-L518), [604–617](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/Settings.tsx#L604-L617).
Buttons/input: [button.tsx:7–17](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ui/button.tsx#L7-L17), [input.tsx:84–91](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ui/input.tsx#L84-L91).
Transcript: [StreamingMarkdownContent.tsx:144–160](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/StreamingMarkdownContent.tsx#L144-L160), [Message.tsx:812–815](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/Message.tsx#L812-L815), [Typography base styles](https://github.com/tailwindlabs/tailwindcss-typography/blob/v0.5.16/src/styles.js#L225-L233).

## Exact family and reset behavior

[index.css:13–17](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/index.css#L13-L17):

```css
--font-sans: ui-sans-serif, system-ui, "Segoe UI", sans-serif;
--font-rounded: "SF Pro Rounded", ui-sans-serif, system-ui, "Segoe UI", sans-serif;
```

Use the sans stack for the entire interface; use rounded only for the small toolbar section title. Do not select Inter, Arial, Helvetica Neue, SF Pro Display, or a rounded family globally. Let macOS resolve its system UI font.

The inspected global stylesheet, HTML body, route root, and native webview initialization add **no antialiasing, font-smoothing, text-rendering, or custom letter-spacing override**. Leave browser smoothing at its default; do not automatically add `-webkit-font-smoothing: antialiased` when matching this source.

Ollama imports Tailwind preflight. Its relevant behavior: headings inherit size/weight, form controls inherit font and tracking, margins/padding reset, border-box sizing, HTML line-height1.5, normal font features/variations. A static prototype without those resets can silently introduce bold default headings and browser-default button fonts. [Tailwind4.1.11 preflight](https://github.com/tailwindlabs/tailwindcss/blob/v4.1.11/packages/tailwindcss/preflight.css#L25-L42), [heading reset](https://github.com/tailwindlabs/tailwindcss/blob/v4.1.11/packages/tailwindcss/preflight.css#L68-L76), [control inheritance](https://github.com/tailwindlabs/tailwindcss/blob/v4.1.11/packages/tailwindcss/preflight.css#L225-L238).

Source uses `text-md` on the toolbar title, but Tailwind's pinned default theme and Ollama theme do not define that token. Treat it as inherited16px rather than inventing an18px “medium” size. The source does explicitly set title weight500 and rounded family. [Tailwind text/weight tokens](https://github.com/tailwindlabs/tailwindcss/blob/v4.1.11/packages/tailwindcss/theme.css#L273-L307).

## Why settings names stay regular

Exact Label class:

```
text-base/6 text-zinc-950 select-none data-disabled:opacity-50 sm:text-sm/6 dark:text-white
```

Exact Description class:

```
text-base/6 text-zinc-500 data-disabled:opacity-50 sm:text-sm/6 dark:text-zinc-400
```

Neither has a font-weight class. Field adds `*:data-[slot=label]:font-medium`, which only targets a direct child label. In the actual Settings tree, the label is nested under layout wrappers:

```text
Field → flex row → icon/text wrapper → text div → Label + Description
```

Thus ordinary row labels remain400. The same nesting means Field's direct-sibling label→description margin rule does not add a4px gap here: title and single-line description naturally occupy two24px lines,48px total. Avoid inserting a large gap or bold title.

## Settings slabs and composer baseline

- Settings outer content:24px padding; centered max-width896px; groups16px apart; group radius12px; interior padding16px; rows16px apart.
- Standard icon/name/description row:20px icon, top margin4px,12px gap to text; controls align to row top via flex; two lines occupy48px. Three such rows in one group produce about208px total height (144px rows +32px gaps +32px padding).
- Composer: max768px;24px radius; top padding16px, bottom8px. Input is24px minimum, controls36px plus8px top padding. Minimum class88px, but normal rendered content can occupy roughly92–98px including the inline textarea baseline. Use the supplied screenshot's98px target without enlarging its type.
- Keep the reference palette: canvas#171717, sidebar approximately#111111, slabs/composer#262626, tool circles#404040. Source sidebar is neutral-950 at40% opacity over the surrounding surface, not a mandatory flat hex. Use the root's screenshot measurement for a faithful static match.

Settings geometry: [Settings.tsx:488–497](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/Settings.tsx#L488-L497), [602–617](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/Settings.tsx#L602-L617). Composer: [ChatForm.tsx:726](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ChatForm.tsx#L726), [858–885](https://github.com/ollama/ollama/blob/5632619189ab493c70bd3f9435214b8788cc1c80/app/ui/app/src/components/ChatForm.tsx#L858-L885).

## Rendering recommendation

Render at a real Mac CSS viewport (about1450×780 from supplied references) using the system fonts and default zoom, then capture at2× device scale for sharp text. Review at100% logical size. Set typography explicitly per the table; do not globally scale the canvas, enlarge fonts to fill whitespace, or rely on image generation for letters. Keep all normal labels400; use500 sparingly as documented. This preserves the original feature requirements while matching Ollama's visible hierarchy.

