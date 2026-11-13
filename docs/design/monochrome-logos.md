# Melly monochrome logo variants

Created on 2026-11-13 with the built-in OpenAI image-generation tool, editing the approved conversation mark. These are separate opaque logo assets; they do not add a theme-dependent Dock icon or change the installed application bundle.

- White on black: `public/branding/melly-white-on-black.png`
- Black on white: `public/branding/melly-black-on-white.png`

Both outputs were inspected visually. The two opposing speech bubbles and borderless composition are retained. No provider branding, text or silver frame was introduced. These are raster image edits, not mathematically identical vector inversions.

## White-on-black edit prompt

Reference: `public/melly-icon.png`.

> Use case: precise-object-edit. Asset: Melly Mac application logo, white-on-black colorway. Edit the provided existing logo, not a redesign. Keep the EXACT two opposing speech-bubble silhouettes, their contours, placement, scale, gap, and square canvas composition unchanged. Change only the colors: the two existing warm-white shapes become solid pure white (#FFFFFF); the entire existing dark background becomes solid pure black (#000000), edge to edge. Flat two-color artwork with smooth antialiased edges only. Remove the subtle texture, warm tint, gradients, and highlights. Fully opaque square image, no transparent margins. No border, no silver frame, no outer tile, no rounded-square inset, no added shadow, no text. Preserve the existing logo identity precisely.

## Black-on-white edit prompt

Reference: the generated white-on-black variant above.

> Use case: precise-object-edit. Asset: Melly application logo, black-on-white colorway. Perform only an exact monochrome color inversion of the reference. The reference's two WHITE opposing speech-bubble silhouettes must become solid PURE BLACK (#000000). The reference's entire BLACK background must become solid PURE WHITE (#FFFFFF), edge to edge. Preserve the two silhouettes EXACTLY: identical contours, placement, scale, negative-space gap, and square composition. Do not redesign, redraw, shift, enlarge or add anything. Flat two-color artwork, smooth antialiased edges, no texture, no gradients, no highlights. Fully opaque square. No border, no silver frame, no inset tile, no extra shadow, no text.
