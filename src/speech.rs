pub async fn speak(text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut speech =
            tts::Tts::default().map_err(|error| format!("Speech output failed: {error}"))?;
        speech
            .speak(&text, true)
            .map(|_| ())
            .map_err(|error| format!("Speech output failed: {error}"))
    })
    .await
    .map_err(|error| format!("Speech output task failed: {error}"))?
}
