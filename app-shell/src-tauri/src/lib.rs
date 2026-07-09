mod agent;
mod models;
mod ollama;
mod settings;
mod vision_models;

use agent::{AgentProcess, AgentStatus};
use models::{is_ollama_model, MODEL_CHOICES};
use serde::Serialize;
use settings::LocalState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use vision_models::{is_ollama_backed, VISION_CHOICES};

#[derive(Serialize)]
struct ModelInfo {
    name: String,
    size_human: String,
    description: String,
    selected: bool,
    downloaded: Option<bool>,
}

#[derive(Serialize)]
struct ScreenWatchSettings {
    enabled: bool,
    interval_seconds: f64,
}

#[derive(Serialize)]
struct TokenSettings {
    token: String,
    api_url: String,
}

fn ollama_host() -> String {
    settings::read_env_values(&settings::state_dir(), &["OLLAMA_HOST"])
        .get("OLLAMA_HOST")
        .cloned()
        .unwrap_or_else(|| "http://localhost:11434".to_string())
}

fn apple_helper(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .resource_dir()
        .map(|d| agent::apple_ai::helper_path(&d))
        .map_err(|e| e.to_string())
}

#[derive(Serialize, serde::Deserialize)]
struct VerifiedAccount {
    username: Option<String>,
    name: Option<String>,
}

fn api_url() -> String {
    settings::read_env_values(&settings::state_dir(), &["LIFE_UPDATE_API_URL"])
        .get("LIFE_UPDATE_API_URL")
        .cloned()
        .unwrap_or_else(|| "https://www.life-update.com".to_string())
}

async fn verify_token_inner(api: &str, token: &str) -> Result<VerifiedAccount, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(format!("{}/api/device", api.trim_end_matches('/')))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| "Couldn't reach life-update.com - check your connection and try again.".to_string())?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("That token isn't valid. Generate a fresh one at life-update.com → Settings → Devices.".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("life-update.com returned an error ({})", resp.status()));
    }
    resp.json::<VerifiedAccount>()
        .await
        .map_err(|_| "Unexpected response from life-update.com".to_string())
}

/// Validates a pasted token against life-update.com BEFORE anything is
/// saved - onboarding refuses to proceed on an invalid token instead of
/// silently failing at first sync.
#[tauri::command]
async fn verify_token(token: String) -> Result<VerifiedAccount, String> {
    verify_token_inner(&api_url(), token.trim()).await
}

#[tauri::command]
async fn get_connected_account() -> Result<VerifiedAccount, String> {
    let token = settings::read_env_values(&settings::state_dir(), &["LIFE_UPDATE_TOKEN"])
        .get("LIFE_UPDATE_TOKEN")
        .cloned()
        .unwrap_or_default();
    if token.is_empty() {
        return Err("not connected".to_string());
    }
    verify_token_inner(&api_url(), &token).await
}

#[tauri::command]
fn disconnect_device(state: State<AgentProcess>) -> Result<(), String> {
    let _ = agent::stop(&state);
    settings::write_env_values(&settings::state_dir(), &[("LIFE_UPDATE_TOKEN", "")])
}

#[tauri::command]
fn get_token_settings() -> TokenSettings {
    let values = settings::read_env_values(&settings::state_dir(), &["LIFE_UPDATE_TOKEN", "LIFE_UPDATE_API_URL"]);
    TokenSettings {
        token: values.get("LIFE_UPDATE_TOKEN").cloned().unwrap_or_default(),
        api_url: values
            .get("LIFE_UPDATE_API_URL")
            .cloned()
            .unwrap_or_else(|| "https://www.life-update.com".to_string()),
    }
}

#[tauri::command]
fn save_token_settings(token: String, api_url: String) -> Result<(), String> {
    settings::write_env_values(
        &settings::state_dir(),
        &[("LIFE_UPDATE_TOKEN", &token), ("LIFE_UPDATE_API_URL", &api_url)],
    )
}

#[tauri::command]
fn get_exclude_list() -> LocalState {
    settings::read_state()
}

#[tauri::command]
fn add_exclude_app(app: String) -> Result<(), String> {
    let mut state = settings::read_state();
    state.apps.push(app);
    settings::write_state(&state)
}

#[tauri::command]
fn remove_exclude_app(app: String) -> Result<(), String> {
    let mut state = settings::read_state();
    state.apps.retain(|a| a != &app);
    settings::write_state(&state)
}

#[tauri::command]
fn add_exclude_title_pattern(pattern: String) -> Result<(), String> {
    let mut state = settings::read_state();
    state.title_patterns.push(pattern);
    settings::write_state(&state)
}

#[tauri::command]
fn remove_exclude_title_pattern(pattern: String) -> Result<(), String> {
    let mut state = settings::read_state();
    state.title_patterns.retain(|p| p != &pattern);
    settings::write_state(&state)
}

#[tauri::command]
async fn list_models(app: AppHandle) -> Result<Vec<ModelInfo>, String> {
    let selected = settings::read_state().ollama_model;
    let local_models = ollama::list_local_models(&ollama_host()).await.ok();
    let apple_available = apple_helper(&app)
        .map(|h| agent::apple_ai::availability(&h).is_ok())
        .unwrap_or(false);

    Ok(MODEL_CHOICES
        .iter()
        .map(|m| ModelInfo {
            name: m.name.to_string(),
            size_human: m.size_human.to_string(),
            description: m.description.to_string(),
            selected: m.name == selected,
            downloaded: if is_ollama_model(m.name) {
                local_models.as_ref().map(|set| set.contains(m.name))
            } else {
                Some(apple_available)
            },
        })
        .collect())
}

#[tauri::command]
async fn choose_model(app: AppHandle, name: String) -> Result<(), String> {
    if !MODEL_CHOICES.iter().any(|m| m.name == name) {
        return Err(format!("unknown model {name}"));
    }

    if is_ollama_model(&name) {
        let host = ollama_host();
        let already_local = ollama::list_local_models(&host).await.map_err(|_| {
            "Ollama isn't running. Install and open the Ollama app first, or pick Apple Intelligence instead.".to_string()
        })?;
        if !already_local.contains(&name) {
            ollama::pull_model(&app, &host, &name).await?;
        }
    } else {
        let helper = apple_helper(&app)?;
        agent::apple_ai::availability(&helper)?;
    }

    let mut state = settings::read_state();
    state.ollama_model = name;
    settings::write_state(&state)
}

#[tauri::command]
async fn list_vision_engines() -> Result<Vec<ModelInfo>, String> {
    let selected = settings::read_state().vision_engine;
    let selected = if selected == "tesseract" { vision_models::NATIVE_ENGINE.to_string() } else { selected };
    let local_models = ollama::list_local_models(&ollama_host()).await.ok();

    Ok(VISION_CHOICES
        .iter()
        .map(|v| ModelInfo {
            name: v.name.to_string(),
            size_human: v.size_human.to_string(),
            description: v.description.to_string(),
            selected: v.name == selected,
            downloaded: if !is_ollama_backed(v.name) {
                Some(true)
            } else {
                local_models.as_ref().map(|set| set.contains(v.name))
            },
        })
        .collect())
}

#[tauri::command]
async fn choose_vision_engine(app: AppHandle, name: String) -> Result<(), String> {
    if !VISION_CHOICES.iter().any(|v| v.name == name) {
        return Err(format!("unknown vision engine {name}"));
    }

    if is_ollama_backed(&name) {
        let host = ollama_host();
        let already_local = ollama::list_local_models(&host).await.map_err(|_| {
            "Ollama isn't running. Install and open the Ollama app first, or keep the built-in engine.".to_string()
        })?;
        if !already_local.contains(&name) {
            ollama::pull_model(&app, &host, &name).await?;
        }
    }

    let mut state = settings::read_state();
    state.vision_engine = name;
    settings::write_state(&state)
}

#[tauri::command]
fn get_screen_watch_settings() -> ScreenWatchSettings {
    let state = settings::read_state();
    ScreenWatchSettings {
        enabled: state.screen_watch_enabled,
        interval_seconds: state.screen_capture_interval_seconds,
    }
}

#[tauri::command]
fn set_screen_watch_enabled(enabled: bool) -> Result<(), String> {
    let mut state = settings::read_state();
    state.screen_watch_enabled = enabled;
    settings::write_state(&state)
}

#[tauri::command]
fn set_screen_capture_interval(seconds: f64) -> Result<(), String> {
    if seconds <= 0.0 {
        return Err("interval must be a positive number of seconds".to_string());
    }
    let mut state = settings::read_state();
    state.screen_capture_interval_seconds = seconds;
    settings::write_state(&state)
}

#[tauri::command]
fn agent_status() -> Result<AgentStatus, String> {
    agent::fetch_status()
}

#[tauri::command]
fn recent_events(limit: u32) -> Result<Vec<agent::RawEventView>, String> {
    agent::recent_events(limit.min(500))
}

#[tauri::command]
fn recent_sessions(limit: u32) -> Result<Vec<agent::SessionView>, String> {
    agent::recent_sessions(limit.min(100))
}

#[tauri::command]
fn session_events(started_at: String, ended_at: String) -> Result<Vec<agent::RawEventView>, String> {
    agent::session_events(&started_at, &ended_at)
}

#[tauri::command]
async fn start_agent(app: AppHandle, agent_state: State<'_, AgentProcess>) -> Result<(), String> {
    // Never trigger a model download from here - onboarding/Settings are
    // where downloads happen, explicitly. Just verify the chosen engine is
    // actually usable and fail loudly if not.
    let model = settings::read_state().ollama_model;
    if is_ollama_model(&model) {
        let host = ollama_host();
        let local = ollama::list_local_models(&host).await.map_err(|_| {
            "Ollama isn't running. Open the Ollama app, or switch to Apple Intelligence in Settings.".to_string()
        })?;
        if !local.contains(&model) {
            return Err(format!("{model} is not downloaded yet - choose a model in Settings first"));
        }
    } else {
        let helper = apple_helper(&app)?;
        agent::apple_ai::availability(&helper)?;
    }

    agent::start(&app, &agent_state)
}

#[tauri::command]
fn stop_agent(state: State<AgentProcess>) -> Result<(), String> {
    agent::stop(&state)
}

#[tauri::command]
fn is_agent_running(state: State<AgentProcess>) -> bool {
    agent::is_running(&state)
}

/// "Launch at login" registers a launchd plist pointing at the app's
/// *current* path - warn before enabling from a DMG/translocated copy.
#[tauri::command]
fn is_running_from_applications() -> bool {
    std::env::current_exe()
        .map(|p| p.starts_with("/Applications"))
        .unwrap_or(false)
}

/// Removes everything Life-Update stores on this machine: the local
/// capture database + config (~/.life-update-agent) and the launch-at-login
/// entry. Ollama models (if any were used) live in Ollama's own folder and
/// are managed from the Ollama app.
#[tauri::command]
fn delete_local_data(state: State<AgentProcess>) -> Result<(), String> {
    let _ = agent::stop(&state);

    let dir = settings::state_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    if let Some(home) = dirs::home_dir() {
        let plist = home.join("Library/LaunchAgents/Life-Update.plist");
        if plist.exists() {
            let _ = std::fs::remove_file(plist);
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AgentProcess(std::sync::Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            get_token_settings,
            save_token_settings,
            verify_token,
            get_connected_account,
            disconnect_device,
            get_exclude_list,
            add_exclude_app,
            remove_exclude_app,
            add_exclude_title_pattern,
            remove_exclude_title_pattern,
            list_models,
            choose_model,
            list_vision_engines,
            choose_vision_engine,
            get_screen_watch_settings,
            set_screen_watch_enabled,
            set_screen_capture_interval,
            agent_status,
            recent_events,
            recent_sessions,
            session_events,
            start_agent,
            stop_agent,
            is_running_from_applications,
            delete_local_data,
            is_agent_running,
        ])
        .setup(|app| {
            let show_item = MenuItem::with_id(app, "show", "Open Settings", true, None::<&str>)?;
            let start_item = MenuItem::with_id(app, "start", "Start Agent", true, None::<&str>)?;
            let stop_item = MenuItem::with_id(app, "stop", "Pause Agent", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;

            let menu = Menu::with_items(
                app,
                &[&show_item, &separator, &start_item, &stop_item, &separator, &quit_item],
            )?;

            // Auto-resume only if a token was already saved from a previous
            // session - never before onboarding has completed.
            let already_configured = settings::read_env_values(&settings::state_dir(), &["LIFE_UPDATE_TOKEN"])
                .get("LIFE_UPDATE_TOKEN")
                .is_some_and(|t| !t.is_empty());
            if already_configured {
                let state: State<AgentProcess> = app.state();
                let _ = agent::start(app.handle(), &state);
            }

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "start" => {
                        let state: State<AgentProcess> = app.state();
                        let _ = agent::start(app, &state);
                        let _ = app.emit("agent-state-changed", true);
                    }
                    "stop" => {
                        let state: State<AgentProcess> = app.state();
                        let _ = agent::stop(&state);
                        let _ = app.emit("agent-state-changed", false);
                    }
                    "quit" => {
                        let state: State<AgentProcess> = app.state();
                        let _ = agent::stop(&state);
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the settings window hides it instead of quitting -
            // the agent keeps running in the tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().ok();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
