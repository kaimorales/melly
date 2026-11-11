# Melly terminal design

## Scene and theme

A developer spends an evening coding in a dim room, reading long terminal conversations and glancing at tool activity. Preserve the approved graphite theme with warm foreground text and restrained lavender and mint accents.

## Palette

Graphite canvas RGB 20,23,30; raised surface 27,30,39; warm ink 235,232,228; secondary text 162,166,177; rules 62,67,81; lavender focus 210,183,240; mint status 164,233,193. Terminal output requires RGB colors. Use the accents for selection and live state.

## Layout

Compact identity and status header. Broad conversation column, narrower activity and usage inspector. A persistent composer anchors the conversation. At narrow widths, focus switches between conversation and inspector. Commands open above the composer, keeping draft text visible.

## Components

Commands and model/effort/skill pickers share a filtered keyboard list, visible selection and concise help. Thinking summaries are muted, expandable conversation blocks. Tool output has status and command labels. Usage meters identify their source and show unknown values explicitly. Approval dialogs take focus and require a separate visible confirmation.

## Typography and motion

Use the terminal's monospace font. Hierarchy comes from weight, spacing, case and restrained color. Animate only the active state indicator; redraw on changes and avoid work while idle.
