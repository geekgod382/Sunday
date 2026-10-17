#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod key_store;
mod setup;
mod speech;

use std::{fs, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use setup::{Provider, ProviderInfo};
use tauri::{AppHandle, Manager, State};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Settings {
    provider: Provider,
    model: String,
}

#[derive(Default)]
struct AppState {
    settings: Mutex<Option<Settings>>,
}

#[derive(Serialize)]
struct Status {
    configured: bool,
    provider: Option<String>,
    model: Option<String>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("Could not find Sunday configuration folder: {error}"))?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create Sunday configuration folder: {error}"))?;
    Ok(directory.join("settings.json"))
}

fn read_settings(app: &AppHandle) -> Result<Option<Settings>, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)
        .map_err(|error| format!("Could not read Sunday settings: {error}"))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|error| format!("Sunday settings are invalid: {error}"))
}

fn write_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    let contents = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("Could not serialize Sunday settings: {error}"))?;
    fs::write(path, contents).map_err(|error| format!("Could not save Sunday settings: {error}"))
}

#[tauri::command]
fn detect_provider(api_key: String) -> Result<ProviderInfo, String> {
    setup::detect_provider(&api_key)
}

#[tauri::command]
fn save_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    api_key: String,
    model: Option<String>,
) -> Result<ProviderInfo, String> {
    let info = setup::detect_provider(&api_key)?;
    let selected_model = model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| info.default_model.to_string());
    let settings = Settings {
        provider: info.provider,
        model: selected_model,
    };

    key_store::save(api_key.trim())?;
    if key_store::load()?.as_deref() != Some(api_key.trim()) {
        return Err(
            "The saved API key could not be verified. Please try saving it again.".to_string(),
        );
    }
    write_settings(&app, &settings)?;
    *state
        .settings
        .lock()
        .map_err(|_| "Sunday settings lock is unavailable".to_string())? = Some(settings);
    Ok(info)
}

#[tauri::command]
fn save_model(app: AppHandle, state: State<'_, AppState>, model: String) -> Result<(), String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("Model name cannot be empty.".to_string());
    }
    let mut current = state
        .settings
        .lock()
        .map_err(|_| "Sunday settings lock is unavailable".to_string())?;
    if current.is_none() {
        *current = read_settings(&app)?;
    }
    let settings = current
        .as_ref()
        .ok_or_else(|| "Add an API key before changing the model.".to_string())?;
    if key_store::load()?.is_none() {
        return Err("The API key is missing. Add it again in Settings.".to_string());
    }
    let updated = Settings {
        provider: settings.provider,
        model: model.to_string(),
    };
    write_settings(&app, &updated)?;
    *current = Some(updated);
    Ok(())
}

#[tauri::command]
fn get_status(app: AppHandle, state: State<'_, AppState>) -> Result<Status, String> {
    let key = key_store::load()?;
    let mut current = state
        .settings
        .lock()
        .map_err(|_| "Sunday settings lock is unavailable".to_string())?;
    if current.is_none() {
        *current = read_settings(&app)?;
    }

    let configured = key.is_some() && current.is_some();
    let settings = current.as_ref();
    Ok(Status {
        configured,
        provider: settings.map(|value| value.provider.name().to_string()),
        model: settings.map(|value| value.model.clone()),
    })
}

#[tauri::command]
async fn ask(state: State<'_, AppState>, question: String) -> Result<AskResult, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("Please enter a question.".to_string());
    }
    let settings = state
        .settings
        .lock()
        .map_err(|_| "Sunday settings lock is unavailable".to_string())?
        .clone()
        .ok_or_else(|| "Add an API key in Settings before asking Sunday.".to_string())?;
    let key = key_store::load()?
        .ok_or_else(|| "The API key is missing. Add it again in Settings.".to_string())?;
    let result = api::ask(settings.provider, &key, &settings.model, question).await?;
    Ok(AskResult {
        answer: result.text,
        links: result.links,
    })
}

#[derive(Serialize)]
struct AskResult {
    answer: String,
    links: Vec<String>,
}

#[tauri::command]
async fn speak(text: String) -> Result<(), String> {
    speech::speak(text).await
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|error| format!("Invalid link: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only HTTP and HTTPS links can be opened.".to_string());
    }
    open::that(parsed.as_str()).map_err(|error| format!("Could not open the link: {error}"))
}

#[tauri::command]
fn set_hotkey_active(active: bool) {
    win_key::set_active(active);
}

#[tauri::command]
fn hide_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Sunday's window is unavailable.".to_string())?;
    window
        .hide()
        .map_err(|error| format!("Could not hide Sunday's window: {error}"))
}

#[tauri::command]
fn show_window(app: AppHandle, compact: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Sunday's window is unavailable.".to_string())?;
    let (width, height, right_margin, bottom_margin) = if compact {
        (260, 90, 28, 48)
    } else {
        (400, 300, 32, 48)
    };
    window
        .set_size(tauri::PhysicalSize::new(width, height))
        .map_err(|error| format!("Could not size Sunday's window: {error}"))?;
    if let Some(monitor) = window
        .current_monitor()
        .map_err(|error| format!("Could not locate Sunday's display: {error}"))?
    {
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let x = monitor_position.x + monitor_size.width as i32 - width as i32 - right_margin;
        let y = monitor_position.y + monitor_size.height as i32 - height as i32 - bottom_margin;
        window
            .set_position(tauri::PhysicalPosition::new(x, y))
            .map_err(|error| format!("Could not position Sunday's window: {error}"))?;
    }
    window
        .show()
        .map_err(|error| format!("Could not show Sunday's window: {error}"))?;
    window
        .set_focus()
        .map_err(|error| format!("Could not focus Sunday's window: {error}"))
}

#[cfg(windows)]
mod win_key {
    use std::{
        ptr,
        sync::{
            atomic::{AtomicBool, AtomicU8, Ordering},
            mpsc::{sync_channel, SyncSender},
            OnceLock,
        },
        thread,
        time::Duration,
    };

    use tauri::{AppHandle, Emitter};
    use windows_sys::Win32::{
        Foundation::{GetLastError, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
            UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
            WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    };

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static HELD_KEYS: AtomicU8 = AtomicU8::new(0);
    static HOTKEY_ACTIVE: AtomicBool = AtomicBool::new(false);
    static SUPPRESS_KEY_SEQUENCE: AtomicBool = AtomicBool::new(false);
    static CHORD_TRIGGERED: AtomicBool = AtomicBool::new(false);

    const WIN_KEYS: u8 = 0b0011;
    const CTRL_KEYS: u8 = 0b1100;

    pub fn set_active(active: bool) {
        HOTKEY_ACTIVE.store(active, Ordering::SeqCst);
    }

    pub fn install(app: AppHandle) -> Result<(), String> {
        APP.set(app)
            .map_err(|_| "Windows key hook is already installed".to_string())?;
        let (ready_sender, ready_receiver) = sync_channel(1);
        thread::Builder::new()
            .name("sunday-win-key-hook".to_string())
            .spawn(move || hook_loop(ready_sender))
            .map_err(|error| format!("Could not start Windows key listener: {error}"))?;
        match ready_receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(result) => result,
            Err(error) => Err(format!("Windows key listener did not start: {error}")),
        }
    }

    fn hook_loop(ready_sender: SyncSender<Result<(), String>>) {
        unsafe {
            let module = GetModuleHandleW(ptr::null());
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_callback), module, 0);
            if hook.is_null() {
                let _ = ready_sender.send(Err(format!(
                    "Could not install the Windows key listener (Windows error {}).",
                    GetLastError()
                )));
                return;
            }
            let _ = ready_sender.send(Ok(()));
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            UnhookWindowsHookEx(hook);
        }
    }

    unsafe extern "system" fn keyboard_callback(
        code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if code >= 0 {
            let key = &*(l_param as *const KBDLLHOOKSTRUCT);
            let key_bit = match key.vkCode {
                0x5B => 1,
                0x5C => 2,
                0xA2 => 4,
                0xA3 => 8,
                _ => 0,
            };
            if key_bit != 0 {
                let mut suppress = SUPPRESS_KEY_SEQUENCE.load(Ordering::SeqCst);
                match w_param as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => {
                        let previous = HELD_KEYS.fetch_or(key_bit, Ordering::SeqCst);
                        let held = previous | key_bit;
                        if held & WIN_KEYS != 0
                            && held & CTRL_KEYS != 0
                            && HOTKEY_ACTIVE.load(Ordering::SeqCst)
                            && !CHORD_TRIGGERED.swap(true, Ordering::SeqCst)
                        {
                            SUPPRESS_KEY_SEQUENCE.store(true, Ordering::SeqCst);
                            emit("voice-start");
                        }
                        suppress =
                            key_bit & WIN_KEYS != 0 && SUPPRESS_KEY_SEQUENCE.load(Ordering::SeqCst);
                    }
                    WM_KEYUP | WM_SYSKEYUP => {
                        let previous = HELD_KEYS.fetch_and(!key_bit, Ordering::SeqCst);
                        let held = previous & !key_bit;
                        if held & (WIN_KEYS | CTRL_KEYS) == 0 {
                            CHORD_TRIGGERED.store(false, Ordering::SeqCst);
                        }
                        if key_bit & WIN_KEYS != 0 {
                            suppress = SUPPRESS_KEY_SEQUENCE.load(Ordering::SeqCst);
                            if held & WIN_KEYS == 0 {
                                SUPPRESS_KEY_SEQUENCE.store(false, Ordering::SeqCst);
                            }
                        }
                    }
                    _ => {}
                }
                if suppress {
                    return 1;
                }
            }
        }
        CallNextHookEx(ptr::null_mut(), code, w_param, l_param)
    }

    fn emit(event: &'static str) {
        if let Some(app) = APP.get() {
            let _ = app.emit(event, ());
        }
    }
}

#[cfg(not(windows))]
compile_error!("Sunday currently supports Windows only.");

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            ask,
            detect_provider,
            get_status,
            open_url,
            save_api_key,
            save_model,
            set_hotkey_active,
            hide_window,
            show_window,
            speak
        ])
        .setup(|app| {
            #[cfg(windows)]
            win_key::install(app.handle().clone()).map_err(std::io::Error::other)?;
            if let Some(window) = app.get_webview_window("main") {
                if let (Ok(Some(monitor)), Ok(window_size)) =
                    (window.current_monitor(), window.outer_size())
                {
                    let monitor_position = monitor.position();
                    let monitor_size = monitor.size();
                    let x = monitor_position.x + monitor_size.width as i32
                        - window_size.width as i32
                        - 32;
                    let y = monitor_position.y + monitor_size.height as i32
                        - window_size.height as i32
                        - 48;
                    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Sunday");
}
