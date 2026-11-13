# Native Mac walkthrough

These are captures of the built Tauri app, running the Rust backend on macOS. The initial walkthrough uses synthetic conversations and web results with **Demo · Mock**. The final ChatGPT section uses the real plan route with synthetic test content. Browser preview tests are separate. The purple screen-control badge and highlighted pointer are macOS automation overlays, not Melly interface elements.

## Folder context and exact sources

Created **Job search demo**, then **My experience** with “My transit project improved forecast accuracy by 18%.” Created **Interview prep** in that folder and explicitly asked to look in the other Job search chats. Rust returned one relevant snippet. Clicking its **My experience** source opened the original message.

![Native folder citation in dark mode](screenshots/native-folder-citation-dark.png)

![Native folder citation in light mode](screenshots/native-folder-citation-light.png)

## Streaming and Stop

The deliberately paced **Demonstrate streaming and Stop** fixture was stopped during a partial reply. Retry preserved one user message. Stop ended the response and retained the partial text.

![Native stopped reply](screenshots/native-stop-dark.png)

## Web references

Enabled Search the web and asked for transit planning ideas. Demo returned a clearly marked synthetic source. Clicking it opened Example Domain in the system browser; no live search was claimed.

![Native synthetic web source](screenshots/native-web-source-dark.png)

## Providers and appearance

Provider rows show actual configured state and billing route, with official artwork including DeepSeek, QwenCloud and Kimi. Qwen Standard API and Token Plan have separate setup paths and fixed endpoints. The native API-key dialog was opened and cancelled empty; no real key was used.

![Native provider settings in light mode](screenshots/native-providers-light.png)

![Native appearance and conversation settings](screenshots/native-settings-light.png)

See [verification](verification.md) for commands, measurements, regression coverage and the live tests that still require an account or API key.

The initial ChatGPT browser-handoff check reached OpenAI's account chooser without a Mac password prompt, verified cancellation, and stopped before account authorization.

## Live ChatGPT plan check

After correcting a missing response-header incompatibility, the native app completed official authorization and model discovery, answered the original failed `hi`, preserved a synthetic test word across turns, streamed a long reply, and stopped while preserving partial output. A subsequent reply completed, and another completed after ordinary Quit and relaunch without another sign-in.

A fresh chat explicitly requested the transit result from other chats in **Job search demo**. The real ChatGPT model called Melly's read-only folder tool, received three local snippets and cited the 18% result. The **My experience** citation opened the exact synthetic source message. The response below is live inference, not the mock adapter; all conversation content is synthetic.

![Live ChatGPT plan reply with a folder citation](screenshots/native-chatgpt-live-folder.png)
