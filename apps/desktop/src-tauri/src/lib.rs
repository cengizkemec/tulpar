use std::{sync::Mutex, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Runtime, State, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Mutex as AsyncMutex;
use url::Url;

const KEYCHAIN_SERVICE: &str = "Tulpar";

#[derive(Debug, Serialize)]
struct CommandError {
    code: String,
    message: String,
}

impl CommandError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

struct DesktopState {
    http: reqwest::Client,
    refresh: AsyncMutex<()>,
    ptt: Mutex<Option<Shortcut>>,
    pending_deep_links: Mutex<Vec<String>>,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .user_agent("Tulpar-Desktop/0.1.0")
                .build()
                .expect("desktop HTTP client"),
            refresh: AsyncMutex::new(()),
            ptt: Mutex::new(None),
            pending_deep_links: Mutex::new(Vec::new()),
        }
    }
}

fn normalize_instance(input: &str) -> Result<String, CommandError> {
    let trimmed = input.trim().trim_end_matches('/');
    let url = Url::parse(trimmed)
        .map_err(|_| CommandError::new("invalid_instance", "Enter a valid server URL."))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(CommandError::new(
            "invalid_instance",
            "The server URL must use http or https.",
        ));
    }
    Ok(trimmed.to_string())
}

fn keychain_entry(instance: &str) -> Result<keyring::Entry, CommandError> {
    keyring::Entry::new(KEYCHAIN_SERVICE, instance).map_err(|error| {
        CommandError::new(
            "keychain_unavailable",
            format!("The operating system keychain is unavailable: {error}"),
        )
    })
}

fn network_error(error: reqwest::Error) -> CommandError {
    CommandError::new(
        "network_error",
        format!("The server could not be reached: {error}"),
    )
}

fn parse_error(error: reqwest::Error) -> CommandError {
    CommandError::new(
        "invalid_response",
        format!("The server returned an invalid response: {error}"),
    )
}

#[derive(Debug, Deserialize)]
struct ProblemDetails {
    code: Option<String>,
    detail: Option<String>,
    title: Option<String>,
}

async fn problem(response: reqwest::Response) -> CommandError {
    let status = response.status();
    let fallback = format!("The server returned HTTP {status}.");
    match response.json::<ProblemDetails>().await {
        Ok(problem) => CommandError::new(
            problem.code.unwrap_or_else(|| "request_failed".to_string()),
            problem.detail.or(problem.title).unwrap_or(fallback),
        ),
        Err(_) => CommandError::new("request_failed", fallback),
    }
}

#[derive(Debug, Deserialize)]
struct ProbeResponse {
    name: String,
    version: String,
    public_url: String,
    registration_enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceInfo {
    url: String,
    name: String,
    version: String,
    registration_enabled: bool,
}

#[tauri::command]
async fn desktop_probe_instance(
    state: State<'_, DesktopState>,
    url: String,
) -> Result<InstanceInfo, CommandError> {
    let base = normalize_instance(&url)?;
    let response = state
        .http
        .get(format!("{base}/api/v1/sync/instance"))
        .send()
        .await
        .map_err(network_error)?;
    if !response.status().is_success() {
        return Err(problem(response).await);
    }
    let payload: ProbeResponse = response.json().await.map_err(parse_error)?;
    Ok(InstanceInfo {
        url: payload.public_url.trim_end_matches('/').to_string(),
        name: payload.name,
        version: payload.version,
        registration_enabled: payload.registration_enabled,
    })
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceStart {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
}

#[tauri::command]
async fn desktop_device_start(
    state: State<'_, DesktopState>,
    instance: String,
    client_name: String,
) -> Result<DeviceStart, CommandError> {
    let base = normalize_instance(&instance)?;
    let response = state
        .http
        .post(format!("{base}/api/v1/auth/device/code"))
        .json(&serde_json::json!({
            "client_name": client_name,
            "platform": "desktop",
        }))
        .send()
        .await
        .map_err(network_error)?;
    if !response.status().is_success() {
        return Err(problem(response).await);
    }
    let payload: DeviceCodeResponse = response.json().await.map_err(parse_error)?;
    Ok(DeviceStart {
        device_code: payload.device_code,
        user_code: payload.user_code,
        verification_uri: payload.verification_uri,
        expires_in: payload.expires_in,
        interval: payload.interval,
    })
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    session_id: String,
    user: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DevicePoll {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[tauri::command]
async fn desktop_device_poll(
    state: State<'_, DesktopState>,
    instance: String,
    device_code: String,
) -> Result<DevicePoll, CommandError> {
    let base = normalize_instance(&instance)?;
    let response = state
        .http
        .post(format!("{base}/api/v1/auth/device/token"))
        .json(&serde_json::json!({ "device_code": device_code }))
        .send()
        .await
        .map_err(network_error)?;
    if response.status().is_success() {
        let payload: DeviceTokenResponse = response.json().await.map_err(parse_error)?;
        let entry = keychain_entry(&base)?;
        entry
            .set_password(&payload.refresh_token)
            .map_err(|error| {
                CommandError::new(
                    "keychain_unavailable",
                    format!("The refresh token could not be stored: {error}"),
                )
            })?;
        return Ok(DevicePoll {
            status: "approved".to_string(),
            access_token: Some(payload.access_token),
            expires_in: Some(payload.expires_in),
            session_id: Some(payload.session_id),
            user: Some(payload.user),
            message: None,
        });
    }

    let error = problem(response).await;
    let status = match error.code.as_str() {
        "authorization_pending" => "pending",
        "access_denied" => "denied",
        "expired_token" => "expired",
        _ => return Err(error),
    };
    Ok(DevicePoll {
        status: status.to_string(),
        access_token: None,
        expires_in: None,
        session_id: None,
        user: None,
        message: Some(error.message),
    })
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RefreshResult {
    access_token: String,
    expires_in: i64,
}

#[tauri::command]
async fn desktop_refresh(
    state: State<'_, DesktopState>,
    instance: String,
) -> Result<RefreshResult, CommandError> {
    let base = normalize_instance(&instance)?;
    let _guard = state.refresh.lock().await;
    let entry = keychain_entry(&base)?;
    let refresh_token = match entry.get_password() {
        Ok(token) => token,
        Err(keyring::Error::NoEntry) => {
            return Err(CommandError::new(
                "no_session",
                "No stored session exists for this server.",
            ));
        }
        Err(error) => {
            return Err(CommandError::new(
                "keychain_unavailable",
                format!("The refresh token could not be read: {error}"),
            ));
        }
    };

    let response = state
        .http
        .post(format!("{base}/api/v1/auth/refresh"))
        .json(&serde_json::json!({
            "refresh_token": refresh_token,
            "client": "desktop",
        }))
        .send()
        .await
        .map_err(network_error)?;
    if !response.status().is_success() {
        let error = problem(response).await;
        if matches!(
            error.code.as_str(),
            "invalid_token" | "session_expired" | "session_revoked"
        ) {
            let _ = entry.delete_credential();
        }
        return Err(error);
    }

    let payload: RefreshResponse = response.json().await.map_err(parse_error)?;
    if let Some(rotated) = payload.refresh_token {
        entry.set_password(&rotated).map_err(|error| {
            CommandError::new(
                "keychain_unavailable",
                format!("The rotated refresh token could not be stored: {error}"),
            )
        })?;
    }
    Ok(RefreshResult {
        access_token: payload.access_token,
        expires_in: payload.expires_in,
    })
}

#[tauri::command]
async fn desktop_logout(
    state: State<'_, DesktopState>,
    instance: String,
) -> Result<(), CommandError> {
    let base = normalize_instance(&instance)?;
    let entry = keychain_entry(&base)?;
    if let Ok(refresh_token) = entry.get_password() {
        let _ = state
            .http
            .post(format!("{base}/api/v1/auth/logout"))
            .json(&serde_json::json!({ "refresh_token": refresh_token }))
            .send()
            .await;
    }
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(CommandError::new(
            "keychain_unavailable",
            format!("The stored session could not be removed: {error}"),
        )),
    }
}

#[tauri::command]
fn desktop_keychain_has(instance: String) -> Result<bool, CommandError> {
    let base = normalize_instance(&instance)?;
    let entry = keychain_entry(&base)?;
    match entry.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(CommandError::new(
            "keychain_unavailable",
            format!("The keychain could not be read: {error}"),
        )),
    }
}

#[tauri::command]
fn desktop_notify(app: AppHandle, title: String, body: String) -> Result<(), CommandError> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| {
            CommandError::new(
                "notification_failed",
                format!("The notification could not be shown: {error}"),
            )
        })
}

#[tauri::command]
fn desktop_open_external(app: AppHandle, url: String) -> Result<(), CommandError> {
    let parsed =
        Url::parse(&url).map_err(|_| CommandError::new("invalid_url", "Enter a valid URL."))?;
    if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
        return Err(CommandError::new(
            "invalid_url",
            "Only http, https, and mailto URLs can be opened.",
        ));
    }
    app.opener().open_url(url, None::<&str>).map_err(|error| {
        CommandError::new(
            "open_failed",
            format!("The URL could not be opened: {error}"),
        )
    })
}

#[tauri::command]
fn desktop_register_ptt(
    app: AppHandle,
    state: State<'_, DesktopState>,
    shortcut: String,
) -> Result<bool, CommandError> {
    let mut current = state.ptt.lock().expect("ptt lock");
    if let Some(previous) = current.take() {
        let _ = app.global_shortcut().unregister(previous);
    }
    if shortcut.trim().is_empty() {
        return Ok(false);
    }
    let parsed: Shortcut = shortcut.parse().map_err(|error| {
        CommandError::new(
            "invalid_shortcut",
            format!("The shortcut could not be parsed: {error}"),
        )
    })?;
    app.global_shortcut()
        .on_shortcut(parsed, move |app, _shortcut, event| {
            let payload = if event.state == ShortcutState::Pressed {
                "down"
            } else {
                "up"
            };
            let _ = app.emit("ptt", payload);
        })
        .map_err(|error| {
            CommandError::new(
                "shortcut_conflict",
                format!("The shortcut could not be registered: {error}"),
            )
        })?;
    *current = Some(parsed);
    Ok(true)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    platform: String,
    keychain: bool,
    global_shortcut: bool,
    tray: bool,
}

#[tauri::command]
fn desktop_capabilities() -> Capabilities {
    let wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false);
    let keychain = keyring::Entry::new(KEYCHAIN_SERVICE, "capability-probe").is_ok();
    Capabilities {
        platform: std::env::consts::OS.to_string(),
        keychain,
        global_shortcut: cfg!(any(target_os = "windows", target_os = "linux")) && !wayland,
        tray: cfg!(any(target_os = "windows", target_os = "linux")),
    }
}

#[tauri::command]
fn desktop_set_unread(app: AppHandle, count: i64) -> Result<(), CommandError> {
    if let Some(tray) = app.tray_by_id("main") {
        let tooltip = if count > 0 {
            format!("Tulpar — {count} unread")
        } else {
            "Tulpar".to_string()
        };
        tray.set_tooltip(Some(tooltip)).map_err(|error| {
            CommandError::new(
                "tray_failed",
                format!("The tray tooltip could not be updated: {error}"),
            )
        })?;
    }
    Ok(())
}

#[tauri::command]
fn desktop_take_deep_links(state: State<'_, DesktopState>) -> Vec<String> {
    std::mem::take(&mut *state.pending_deep_links.lock().expect("deep link lock"))
}

#[tauri::command]
fn desktop_export_diagnostics(app: AppHandle) -> Result<String, CommandError> {
    let directory = app.path().app_data_dir().map_err(|error| {
        CommandError::new(
            "diagnostics_failed",
            format!("The app data directory is unavailable: {error}"),
        )
    })?;
    std::fs::create_dir_all(&directory).map_err(|error| {
        CommandError::new(
            "diagnostics_failed",
            format!("The diagnostics directory could not be created: {error}"),
        )
    })?;
    let path = directory.join("diagnostics.txt");
    let capabilities = desktop_capabilities();
    let content = format!(
        "Tulpar diagnostics\n\
         version: {}\n\
         platform: {}\n\
         architecture: {}\n\
         keychain: {}\n\
         global_shortcut: {}\n\
         tray: {}\n\
         executable: {:?}\n\
         generated_at: {}\n",
        env!("CARGO_PKG_VERSION"),
        capabilities.platform,
        std::env::consts::ARCH,
        capabilities.keychain,
        capabilities.global_shortcut,
        capabilities.tray,
        std::env::current_exe().ok(),
        chrono::Utc::now().to_rfc3339(),
    );
    std::fs::write(&path, content).map_err(|error| {
        CommandError::new(
            "diagnostics_failed",
            format!("Diagnostics could not be written: {error}"),
        )
    })?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn desktop_show_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn desktop_autostart_get(app: AppHandle) -> Result<bool, CommandError> {
    app.state::<tauri_plugin_autostart::AutoLaunchManager>()
        .is_enabled()
        .map_err(|error| {
            CommandError::new(
                "autostart_failed",
                format!("Autostart state could not be read: {error}"),
            )
        })
}

#[tauri::command]
fn desktop_autostart_set(app: AppHandle, enabled: bool) -> Result<(), CommandError> {
    let manager = app.state::<tauri_plugin_autostart::AutoLaunchManager>();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|error| {
        CommandError::new(
            "autostart_failed",
            format!("Autostart could not be updated: {error}"),
        )
    })
}

fn setup_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Tulpar", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Tulpar", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("Tulpar")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn emit_deep_link<R: Runtime>(app: &tauri::AppHandle<R>, url: &str) {
    if let Ok(parsed) = Url::parse(url) {
        if parsed.scheme() == "tulpar" {
            let _ = app.emit("deep-link", url.to_string());
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            for arg in args {
                emit_deep_link(app, &arg);
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            desktop_probe_instance,
            desktop_device_start,
            desktop_device_poll,
            desktop_refresh,
            desktop_logout,
            desktop_keychain_has,
            desktop_notify,
            desktop_open_external,
            desktop_register_ptt,
            desktop_capabilities,
            desktop_set_unread,
            desktop_show_window,
            desktop_take_deep_links,
            desktop_export_diagnostics,
            desktop_autostart_get,
            desktop_autostart_set,
        ])
        .setup(|app| {
            setup_tray(app)?;
            let panic_handle = app.handle().clone();
            std::panic::set_hook(Box::new(move |info| {
                if let Ok(directory) = panic_handle.path().app_data_dir() {
                    let _ = std::fs::create_dir_all(&directory);
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(directory.join("crash.log"))
                    {
                        use std::io::Write;
                        let _ = writeln!(file, "{} panic: {info}", chrono::Utc::now().to_rfc3339());
                    }
                }
                eprintln!("{info}");
            }));
            let initial_links: Vec<String> = std::env::args()
                .filter(|argument| argument.starts_with("tulpar://"))
                .collect();
            if !initial_links.is_empty() {
                *app.state::<DesktopState>()
                    .pending_deep_links
                    .lock()
                    .expect("deep link lock") = initial_links;
            }
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    emit_deep_link(&handle, url.as_str());
                }
            });
            #[cfg(any(windows, target_os = "linux"))]
            {
                let _ = app.deep_link().register_all();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Tulpar desktop");
}
