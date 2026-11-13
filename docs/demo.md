# Native Mac walkthrough

These are captures of the built Tauri app, running the Rust backend on macOS. Every conversation and web result shown is synthetic and the model selector says **Demo · Mock**. Browser preview tests are separate. The purple screen-control badge and highlighted pointer are macOS automation overlays, not Melly interface elements.

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

The final native ChatGPT check opened OpenAI's account chooser for Melly without a Mac password prompt. Closing and reopening the provider panel preserved Cancel sign-in; cancelling returned the app to Not connected. No account selection, consent, token exchange or live model reply was completed.
