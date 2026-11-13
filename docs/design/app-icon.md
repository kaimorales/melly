# Melly conversation icon

The default is two bold, offset conversation shapes in black on white, selected by the user from the monochrome variants. No lettering, provider branding, silver frame or inset tile. The previous M monogram was discarded at the user's request.

Generated with OpenAI image generation on 2026-11-13. The approved conversation mark was retained in subsequent edits, removing the inset tile and making both monochrome colorways. `public/melly-icon.png` is now a copy of `public/branding/melly-black-on-white.png`; the inverse remains available as a separate logo asset. See [prompts and variants](monochrome-logos.md).

The source is deliberately opaque: on macOS 26.6.2, the original transparent margins caused Finder to shrink the artwork onto a silver system tile. An opaque full-bleed source lets the system supply its rounded mask without that frame. Changing only the bundle's icon name or using Apple's `iconutil` with the original art did not resolve it.

Source: `public/melly-icon.png`. Generate with `node scripts/tauri.mjs icon public/melly-icon.png --output artifacts/icon-final`, then copy `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png` and `icon.icns` into `src-tauri/icons/`. Only Mac bundle assets are retained. Rebuild the app to seal the new icon into its signature; do not edit a signed bundle in place.

The white default was inspected in Finder Quick Look on macOS 26.6.2 after the release rebuild: black conversation shapes, white rounded background and no inset silver frame. The bundled ICNS matched the source asset byte for byte, and strict bundle signature verification passed. TypeScript, formatting, Clippy, 109 Rust tests and 25 UI tests passed.

The final bundled appearance must be inspected in Finder, not just in a PNG preview. macOS 12–15 icon masking has not been tested; the opaque source can appear square on those older systems. An asset-catalog icon with a separately tested legacy fallback remains future distribution work. No system-wide icon settings or caches are changed.
