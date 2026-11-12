# Melly terminal design

## Scene and visual direction

A developer codes late in a dim room, reads long conversations and occasionally checks delegated work. Keep OpenCode's familiar chat rhythm: shaded user messages, plain assistant text, an anchored composer and a compact sidebar. Replies have no repeated identity footer. The composer shows the full model name and reasoning effort; agent identity belongs in navigation. Melly adds an original lowercase pixel wordmark and a small activity pulse. No decorative dashboard or permanent animated background.

## Shared palette

`src/visual.rs` owns terminal RGB tokens. The near-black neutral ramp is slightly cool: canvas 12/13/15, panel 23/25/28, sidebar 17/19/21, selection 33/37/42, rule 44/48/54. Warm ink is 233/232/228; secondary text 149/152/159. Faint 93/98/106 is reserved for decorative marks and secondary elapsed timing, not essential instructions. The user's accent is used for focus and identity. Mint means positive status; amber marks thinking and attention. Preserve saved accent choices.

## Geometry and reading rhythm

- A two-column text gutter aligns user messages, replies, code, wrapped continuations, composer text and metadata. Use widget padding, not spaces that disappear on wrapping.
- One blank row separates conversation groups. User blocks have a one-cell accent edge and one row of vertical padding. Bright text remains readable against the shaded surface.
- Wide layouts have a one-column gap before the 34-column sidebar. Narrow layouts switch panes. Sidebar content starts one row below its top edge.
- The composer grows with wrapped drafts up to six lines, bounded by available height. A quiet row separates text from model metadata. Its activity row is reserved even when idle, preventing a turn completion from moving the transcript.
- A separate, muted shortcut row aligns to the chat's right edge, with one blank row above it except in very short terminals. Keys use secondary text, labels use faint text; omit whole hints that do not fit instead of clipping them. Terminal font size remains user-controlled.
- A thin, neutral scrollbar occupies the existing right gutter only when loaded content overflows. Track and thumb never change text wrapping; no arrows or decorative indicators. Preserve the top visible message while streaming or prepending history.
- Menus use neutral rules, a clear title, one selected row and readable key hints. Accent belongs to the selection rather than every border.
- Context shows measured last-call occupancy, capacity and an approximate percentage, with a thin, quantized meter. An unknown value leaves the meter unfilled. Account quota stays separate.

## Wordmark and motion

The original lowercase wordmark uses two vertical pixels per terminal cell and a one-pixel shadow. The first letter takes the selected accent. It scales to narrower terminals, with plain text as the smallest fallback. A 600 ms reveal plays once when a new home screen opens; typing works immediately.

One monotonic clock drives small busy indicators across the header, agent list and review view. The composer uses a three-note pulse with gently changing color. This signals work, not percent complete. No per-token reveal or artificial streaming delay.

Busy indicators and the bounded home reveal use a 20 fps timer. Wheel scrolling briefly uses the existing 16 ms render tick with a 100 ms cubic ease-out, quantized to whole terminal rows. Continuing gestures retarget the remaining motion; reversing cancels it. Keyboard jumps stay immediate. Only the selected view schedules scroll frames, and motion stops at transcript boundaries. Ratatui still draws changed cells, and transcript formatting stays cached. Idle views stop scheduling animation frames. Turning off Animations in `/settings` renders the complete logo, static indicators and immediate scrolling. No extra assets, font downloads, graphics protocol or model calls are required.

## Verification

Inspect real Ratatui test-backend captures for home, chat, commands, agents and handoff views. Check widths 24, 40, 80 and 130; verify wrapped text alignment, settings, input during startup and bounded animation. Run the existing pseudo-terminal flows for session restore, delegation, history and approvals. Render timing measures local terminal work only, never model latency.
