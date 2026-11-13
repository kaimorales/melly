# Melly conversation icon

The previous M monogram was discarded at the user's request. The replacement is two bold, offset conversation shapes in warm white on graphite. No lettering, provider branding, silver frame or inset tile.

Generated with OpenAI image generation on 2026-11-13. The approved conversation mark was retained in a subsequent edit, extending the graphite background to all four edges and removing the inset tile. The source is deliberately opaque: on macOS 26.6.2, the original transparent margins caused Finder to shrink the artwork onto a silver system tile. An opaque full-bleed source lets the system supply its rounded mask without that frame. Changing only the bundle's icon name or using Apple's `iconutil` with the original art did not resolve it.

Source: `public/melly-icon.png`. Generate with `node scripts/tauri.mjs icon public/melly-icon.png --output artifacts/icon-final`, then copy `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png` and `icon.icns` into `src-tauri/icons/`. Only Mac bundle assets are retained. Rebuild the app to seal the new icon into its signature; do not edit a signed bundle in place.

The final bundled appearance must be inspected in Finder, not just in a PNG preview. macOS 12–15 icon masking has not been tested; the opaque source can appear square on those older systems. An asset-catalog icon with a separately tested legacy fallback remains future distribution work. No system-wide icon settings or caches are changed.
