# Sunday - desktop assistant

Sunday is a small Windows desktop assistant. Press `Ctrl+Win` to open its
floating voice window, speak your question, and press `Enter` to submit it.
Sunday speaks the answer, then closes the window.

## Features

- Transparent, always-on-top floating window that skips the taskbar
- Bring Your Own Key setup with provider detection and an editable model name
- API keys stored in the Windows credential vault
- Concise responses from OpenAI, Anthropic, Google Gemini, Groq, or xAI
- Spoken answers without displaying the transcript or response text

Speech recognition uses the Web Speech API available in the WebView. Voice
recognition depends on Windows/WebView support and may require microphone
permission; its processing and privacy behavior are determined by the WebView
speech service. Sunday is voice-only and does not provide a text-entry fallback.

## Build

Requirements: Rust and the Tauri v2 Windows prerequisites (including Microsoft
C++ Build Tools and WebView2).

Run the desktop application in development mode:

```powershell
cargo install tauri-cli
cargo tauri dev
```

Build a release bundle:

```powershell
cargo tauri build
```
