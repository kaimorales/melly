# Melly desktop design brief

The user approved the third visual study before implementation, after supplying current Ollama dark-mode screenshots and correcting the earlier typography and missing provider imagery. The final app uses that approved restraint in both light and dark themes.

The [source study](../research/ollama.md) inspected the real Go desktop host, React components, native Mac chrome, and CSS at a pinned official Ollama commit, plus official screenshots. No Ollama source or branding was copied. See [typography evidence](../research/typography.md).

- A native Mac window, hidden title and unified 52px drag area. No dashboard or extra header band.
- A collapsible 192px sidebar with New Chat, Settings, search, folders and recent chats. Folder actions appear on hover or keyboard focus; chat actions live in one menu.
- System typography: regular 14px interface text, 16px transcript, restrained heading weights. Neutral surfaces create hierarchy without heavy dividers.
- One reading column and composer, at most 768px wide. Empty chats center the composer; active conversations keep it at the bottom.
- The provider logo opens configured provider families. The adjacent model name opens models for that family. Billing remains visible beside it. Model discovery is lazy and cached per endpoint.
- Web search and folder context are small composer controls. Folder retrieval is explicit for each turn; source chips open exact local messages or public HTTPS pages. Search activity takes one quiet transcript line.
- Settings groups provider connections, appearance, conversation defaults and optional external search. API keys are entered in a native secure field; ChatGPT begins with a single browser-sign-in action.
- Light mode preserves the same geometry, contrast and typography. Official light/dark provider asset variants are used where supplied.
- The rejected M icon was replaced with a bold, letter-free conversation mark. [Icon direction](app-icon.md).

Provider images are integration identifiers, not endorsements; [asset provenance and terms](../research/provider-artwork.md) record the remaining distribution limitations. No product release is authorized by the Git pushes.
