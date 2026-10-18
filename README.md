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

Speech-to-text runs locally with Whisper.cpp through `whisper-rs`; microphone
audio is not sent to an online transcription service. Sunday downloads the
English `tiny` model (about 75 MB) once into its app-data folder before voice
input can be used. The separate AI response request still requires an internet
connection. Sunday is voice-only and does not provide a text-entry fallback.

## Build

Requirements: Rust, CMake, LLVM/Clang (including `libclang`), and the Tauri v2
Windows prerequisites (including Microsoft C++ Build Tools and WebView2).
Whisper.cpp's native library is built as part of the Rust build. Make sure
CMake is on `PATH` and set `LIBCLANG_PATH` to LLVM's `bin` directory before
building if it is not already configured in your environment.

Run the desktop application in development mode:

```powershell
cargo install tauri-cli
cargo tauri dev
```

Build a release bundle:

```powershell
cargo tauri build
```
