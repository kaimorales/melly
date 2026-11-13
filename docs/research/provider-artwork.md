# Provider artwork provenance

Verified 2026-11-13. The local desktop app includes unchanged official artwork for ChatGPT, Meta, Claude, Gemini, Grok, OpenRouter, DeepSeek, QwenCloud, and Kimi in `public/providers/`, as requested by the user. Both QwenCloud billing routes share the Qwen icon. Claude, Meta, and Gemini were copied byte-for-byte from the approved private `design-v3/assets/` study; Qwen and Kimi came directly from the official sources below. Files were renamed locally for convenience. No marks were traced, generated, recolored, or taken from an icon pack. No provider permission request was made.

**Sign in with ChatGPT has specific official integration branding and assets.** The other sources have different scopes, recorded below. Including an official file does not establish a general trademark or redistribution license, provider approval, or a partnership. The user's local-app artwork preference does not change those recorded terms. “Unresolved” means this research did not establish permission for the integration context; it is not a claim that the use is unlawful.

## Ready for the relevant integration context

| Provider | Local image files | Source and permitted context | Limits |
| --- | --- | --- | --- |
| ChatGPT | `chatgpt-white.svg`, `chatgpt-black.svg` | [Official SIWC UI/UX guidance](https://developers.openai.com/siwc/ui-ux-guidelines) depicts the logo in plan connection settings, first-use confirmation, and a plan-use indicator. [Website guide](https://developers.openai.com/siwc/website) supplies approved black/white sign-in formats. | Appropriate for a design of the actual supported ChatGPT connection/plan flow. Use **Continue with ChatGPT** for the sign-in action. Do not imply partner approval, claim current availability, or use it as a generic logo for unrelated API providers. |
| Grok | `grok-white.svg`, `grok-black.svg`, `grok-white.png` | [Official guidelines](https://x.ai/legal/brand-guidelines) permit accurate references subject to terms; exact files extracted from the linked [official ZIP](https://data.x.ai/logos/SpaceXAI_Grok_Assets.zip). | Use unchanged, with truthful identification and no endorsement or merged mark. White/black variants are provided by the owner. Permission is revocable. |
| OpenRouter | `openrouter-cloud.svg`, `openrouter-grape.svg` | [Official brand page](https://openrouter.ai/brand/) distributes glyphs expressly for small-size contexts. | Preserve color and geometry; no stretch or remix. Cloud is intended for dark backgrounds; Grape for light. Page provides intended use rather than an unrestricted trademark license. |
| DeepSeek | `public/providers/deepseek.svg` | The [official integration repository's asset guidance](https://github.com/deepseek-ai/awesome-deepseek-integration/blob/main/docs/_logo%20svg/guidelines.md) supports descriptive and nominative identification when DeepSeek models or APIs are actually integrated. Melly implements the documented DeepSeek API adapter. | Keep the supplied blue icon unchanged, next to the DeepSeek provider name. No partnership, official-client, or live-verification claim. The current guideline expressly disclaims a general IP license; this is the specific identification context it describes, not a blanket trademark permission. |

### Direct file URLs

- [ChatGPT white](https://developers.openai.com/assets/siwc/sign-in-buttons/chatgpt-logo-white.svg)
- [ChatGPT black](https://developers.openai.com/assets/siwc/sign-in-buttons/chatgpt-logo-black.svg)
- Grok ZIP members: `SpaceXAI_Grok_Assets/Grok_Logomark_Light.svg`, `Grok_Logomark_Dark.svg`, `Grok_Logomark_Light.png`.
- [OpenRouter Cloud](https://openrouter.ai/brand/logos/transparent/glyph/svg/glyph-cloud.svg)
- [OpenRouter Grape](https://openrouter.ai/brand/logos/transparent/glyph/svg/glyph-grape.svg)
- [DeepSeek ICON.svg](https://raw.githubusercontent.com/deepseek-ai/awesome-deepseek-integration/main/docs/_logo%20svg/ICON.svg), from the [official integration asset directory](https://github.com/deepseek-ai/awesome-deepseek-integration/tree/main/docs/_logo%20svg).

DeepSeek verification: 2,009 bytes, native `viewBox="0 0 71.69 52.76"`, SHA-256 `b15083e3cbb398bd35619b758c43adc0a632efccc7e2d69f2481a860862910fe`. The XML parses, and contains no scripts, event handlers, foreign objects, links, embedded images, remote references, or CSS imports. The direct repository guideline was read because search results retained an older, broader version.

The Grok ZIP initially returned 403; a normal browser User-Agent and Referer retrieved it successfully. Use the canonical extracted `grok-white`/`grok-black` files instead of the earlier favicon downloads.

## Additional official artwork and recorded terms

| Provider | Local image files | Official provenance | Remaining restriction |
| --- | --- | --- | --- |
| Claude | `claude-spark.svg` | [Anthropic press kit](https://www.anthropic.com/press-kit), resolved to [official CDN ZIP](https://www-cdn.anthropic.com/ae59ca4ca194dac9c9dc3bc78c5829468cb0e8af.zip). The selected file is Claude Spark–Clay. | The complete [two-page trademark guidelines](https://assets.claude.com/f7874b435aabdaee5f836e888b4439b9b56a934b.pdf) require specific permission and advance material approval. No applicable integration exception was established. |
| Meta / Muse Code | `meta-symbol.svg` | Standalone symbol served as the SVG favicon by [Meta's official home page](https://www.meta.com/): [exact SVG](https://static.xx.fbcdn.net/rsrc.php/yf/r/-7pQO6hUGK_.svg). It is not a crop of the company lockup. | The [company-brand page](https://www.meta.com/brand/resources/meta/company-brand/) requires approval for all Meta-logo use. No public Muse Code-specific logo kit or integration exception was found. This identifies Meta, not a distinct Muse logo; Muse remains unavailable. |
| Gemini | `gemini-color.png` | [Gemini's official application](https://gemini.google.com/) links this [512px multicolor icon](https://www.gstatic.com/lamda/images/gemini_sparkle_4g_512_lt_f94943af3be039176192d.png) as its icon and Apple touch icon. | [Compatibility guidance](https://partnermarketinghub.withgoogle.com/brands/google/use-cases/product-co-branding/) describes contextual icons in compatible-product grids. The [product-icon guidance](https://partnermarketinghub.withgoogle.com/brands/google/branding-guidelines/how-to-show-googles-brand/) requires requesting permission through the Partner Marketing Hub. No applicable Gemini API exception was established. |
| QwenCloud | `qwen.png` | The [official Qwen site](https://qwen.ai/) directly links its [80px icon](https://img.alicdn.com/imgextra/i4/O1CN01OXv3EM1FN8t9W4P79_!!6000000000474-2-tps-80-80.png). | [Qwen terms, section V](https://qwen.ai/termsservice) reserve logo rights and limit the service IP license to necessary service use. No specific public API-integration artwork permission was established. Qwen Code's code license does not establish that permission. |
| Kimi / Moonshot | `kimi-light.svg`, `kimi-dark.svg` | The [official Kimi Brand Book](https://www.kimi.com/en/resources/kimi-brand) provides this [logo ZIP](https://kimi-file.kimi.ai/prod-chat-kimi/kfs/4/1/2026-08-12/1d9u74p1l51jas5cp5oq0?response-content-disposition=attachment%3Bfilename%3Dkimi-logo-assets.zip). Selected members are `KIMI-logos/Logomark/SVG/Logomark_Light.svg` and `Logomark_Dark.svg`. | [Logo terms](https://www.kimi.ai/policies/logo-usage-terms) limit the asset license to editorial, media, and noncommercial promotional use, forbid alteration and implied endorsement, and allow revocation. An ordinary app provider picker is not clearly covered. No broader API branding exception was established. |

### Additional integration context examined

- [Claude Managed Agents reference](https://platform.claude.com/docs/en/managed-agents/reference#branding-guidelines) expressly permits particular product text such as **Claude Agent** for relevant integrations, while prohibiting presentation as Claude Code/Cowork. It does not expressly authorize the Claude icon. Do not broaden this into a logo license or apply Managed Agents naming rules to a different integration.
- [Sign in with ChatGPT terms](https://openai.com/policies/sign-in-with-chatgpt-terms/) require authorized branding and prohibit implied sponsorship. The specific SIWC docs provide a more relevant source than a generic brand-page snippet, but do not establish that Melly is already registered or approved.
- The generic Google documentation page's CC/Apache footer does **not** grant trademark rights. Neither does an open-source code license elsewhere.
- Rechecked Anthropic's current [trademark guidelines](https://assets.claude.com/f7874b435aabdaee5f836e888b4439b9b56a934b.pdf) and Meta's [company-brand guidance](https://www.meta.com/brand/resources/meta/company-brand/): both still require approval. No applicable chat API or Muse Code icon exception was found. Muse remains disabled in Melly; a Meta corporate mark would not establish Muse integration support.

### Direct Gemini asset URLs

- [Gemini app spark](https://storage.googleapis.com/gweb-gemini-cdn/gemini/uploads/c546a20243c57b297546630cf76e417a2483acb8.svg)
- [Workspace communications Gemini icon](https://knowledge.workspace.google.com/static/images/icons/product/gemini.svg)

## Desktop handoff

Preserve every asset's original geometry and color. Render as an image with proportional sizing; do not crop, trace, invert, recolor, or add effects. Keep the provider name beside its image. Custom and Mock have no third-party provider mark and use Melly's own identifiers.

| Provider ID | Image |
| --- | --- |
| `chatgpt` | `chatgpt-black.svg` on light; `chatgpt-white.svg` on dark |
| `muse` | `meta-symbol.svg`; retain the unavailable state |
| `claude` | `claude-spark.svg` |
| `gemini` | `gemini-color.png` |
| `grok` | `grok-black.svg` on light; `grok-white.svg` on dark |
| `openrouter` | `openrouter-grape.svg` on light; `openrouter-cloud.svg` on dark |
| `deepseek` | `deepseek.svg` |
| `qwen-standard`, `qwen-token-plan` | `qwen.png` |
| `moonshot` | `kimi-light.svg` on light; `kimi-dark.svg` on dark |

Kimi's supplied Light file is a black rounded tile; its Dark file is a white rounded tile. These 1.3KB official square logomarks are used instead of the kit's 1.1MB textured app-icon SVG. No asset optimization or color transformation was applied.

### File verification

All 13 bundled files were parsed or checked as PNGs; the new and restored icons were visually inspected. SVGs contain no scripts, event handlers, foreign objects, embedded images, external links, remote references, or CSS imports. Internal gradient/clip references are preserved. New and restored files have these SHA-256 checksums:

| File | Dimensions / viewBox | SHA-256 |
| --- | --- | --- |
| `claude-spark.svg` | `94 × 94` | `6d53db4be375e899c937c26cf16684a80d6e869b1928d72b37748bef2560e219` |
| `meta-symbol.svg` | `32 × 32` | `9c354dc7d40f7862df2dfb34df05ae08c3e956d55a450379803d6cac6878a63e` |
| `gemini-color.png` | `512 × 512` | `5e7cfecaa53f4f65a313fe89b0f389548126544a78fad8489510c70ae641a4a1` |
| `qwen.png` | `80 × 80` | `cd9390bc4209c319121d111f3a1f535b3c7dd909b8f860d41acc9744138d58df` |
| `kimi-light.svg` | `180 × 180` | `a543446e0c6827bae262ec45e6bbc67cc0c9e9f446ebc86be916c082459aed25` |
| `kimi-dark.svg` | `180 × 180` | `e47dc8cfa928317cba1ce1605545d5b92f349a465b031f956a56f2788ed17c6a` |

## Compact dark-layout alternatives

The initial Meta full lockup has a large artboard and is unsuitable for a 32px square slot. The initial `gemini-official.svg` is a black spark and loses contrast on a dark surface. These **additional unchanged official assets** address those rendering problems; existing files are retained:

- **`meta-symbol.svg`**: standalone blue gradient Meta infinity symbol with a native `32 × 32` viewport. Retrieved from the SVG favicon explicitly linked by [Meta's official home page](https://www.meta.com/). [Exact asset source](https://static.xx.fbcdn.net/rsrc.php/yf/r/-7pQO6hUGK_.svg). This is the owner's standalone asset, not a crop from the company lockup. Its provenance does not change Meta's unresolved distribution permission noted above.
- **`gemini-color.png`**: transparent `512 × 512` multicolor Gemini spark, visually inspected. Retrieved from the icon and Apple touch icon links served by [Gemini's official application](https://gemini.google.com/). [Exact asset source](https://www.gstatic.com/lamda/images/gemini_sparkle_4g_512_lt_f94943af3be039176192d.png). Its source colors remain unchanged; no inversion or CSS recoloring is needed. Its provenance does not change Gemini's unresolved distribution permission noted above.
