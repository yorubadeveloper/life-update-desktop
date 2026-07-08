mod agent;
mod models;
mod ollama;
mod ollama_process;
mod settings;

use agent::{AgentProcess, AgentStatus};
use models::MODEL_CHOICES;
use ollama_process::OllamaProcess;
use serde::Serialize;
use settings::LocalState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
struct ModelInfo {
    name: String,
    size_human: String,
    description: String,
    selected: bool,
    downloaded: Option<bool>,
}

#[derive(Serialize)]
struct TokenSettings {
    token: String,
    api_url: String,
}

fn ollama_host() -> String {
    settings::read_env_values(&agent::agent_dir(), &["OLLAMA_HOST"])
        .get("OLLAMA_HOST")
        .cloned()
        .unwrap_or_else(|| "http://localhost:11434".to_string())
}

#[tauri::command]
fn get_token_settings() -> TokenSettings {
    let values = settings::read_env_values(&agent::agent_dir(), &["LIFE_UPDATE_TOKEN", "LIFE_UPDATE_API_URL"]);
    TokenSettings {
        token: values.get("LIFE_UPDATE_TOKEN").cloned().unwrap_or_default(),
        api_url: values
            .get("LIFE_UPDATE_API_URL")
            .cloned()
            .unwrap_or_else(|| "https://life-update.com".to_string()),
    }
}

#[tauri::command]
fn save_token_settings(token: String, api_url: String) -> Result<(), String> {
    settings::write_env_values(
        &agent::agent_dir(),
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
async fn list_models() -> Result<Vec<ModelInfo>, String> {
    let host = ollama_host();
    let selected = settings::read_state().ollama_model;
    let local_models = ollama::list_local_models(&host).await.ok();

    Ok(MODEL_CHOICES
        .iter()
        .map(|m| ModelInfo {
            name: m.name.to_string(),
            size_human: m.size_human.to_string(),
            description: m.description.to_string(),
            selected: m.name == selected,
            downloaded: local_models.as_ref().map(|set| set.contains(m.name)),
        })
        .collect())
}

#[tauri::command]
async fn choose_model(app: AppHandle, name: String) -> Result<(), String> {
    if !MODEL_CHOICES.iter().any(|m| m.name == name) {
        return Err(format!("unknown model {name}"));
    }

    let host = ollama_host();
    let already_local = ollama::list_local_models(&host).await.unwrap_or_default();
    if !already_local.contains(&name) {
        ollama::pull_model(&app, &host, &name).await?;
    }

    let mut state = settings::read_state();
    state.ollama_model = name;
    settings::write_state(&state)
}

#[tauri::command]
fn agent_status(app: AppHandle) -> Result<AgentStatus, String> {
    agent::fetch_status(&app)
}

#[tauri::command]
async fn start_agent(
    app: AppHandle,
    agent_state: State<'_, AgentProcess>,
    ollama_state: State<'_, OllamaProcess>,
) -> Result<(), String> {
    ollama_process::ensure_server_running(&app, &ollama_host(), &ollama_state).await?;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AgentProcess(std::sync::Mutex::new(None)))
        .manage(OllamaProcess::default())
        .invoke_handler(tauri::generate_handler![
            get_token_settings,
            save_token_settings,
            get_exclude_list,
            add_exclude_app,
            remove_exclude_app,
            add_exclude_title_pattern,
            remove_exclude_title_pattern,
            list_models,
            choose_model,
            agent_status,
            start_agent,
            stop_agent,
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

            // Only auto-resume on launch if a token was already saved from a
            // previous session - otherwise this would spawn the daemon
            // before onboarding writes LIFE_UPDATE_TOKEN to .env, and since
            // start_agent() is a no-op once already running, the token
            // would never get picked up until the user manually restarted it.
            let already_configured = settings::read_env_values(&agent::agent_dir(), &["LIFE_UPDATE_TOKEN"])
                .get("LIFE_UPDATE_TOKEN")
                .is_some_and(|t| !t.is_empty());
            if already_configured {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let ollama_state: State<OllamaProcess> = handle.state();
                    let agent_state: State<AgentProcess> = handle.state();
                    if ollama_process::ensure_server_running(&handle, &ollama_host(), &ollama_state)
                        .await
                        .is_ok()
                    {
                        agent::start(&handle, &agent_state).ok();
                    }
                });
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
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let ollama_state: State<OllamaProcess> = handle.state();
                            let agent_state: State<AgentProcess> = handle.state();
                            if ollama_process::ensure_server_running(&handle, &ollama_host(), &ollama_state)
                                .await
                                .is_ok()
                            {
                                agent::start(&handle, &agent_state).ok();
                            }
                            let _ = handle.emit("agent-state-changed", true);
                        });
                    }
                    "stop" => {
                        let state: State<AgentProcess> = app.state();
                        agent::stop(&state).ok();
                        let _ = app.emit("agent-state-changed", false);
                    }
                    "quit" => {
                        let agent_state: State<AgentProcess> = app.state();
                        let ollama_state: State<OllamaProcess> = app.state();
                        agent::stop(&agent_state).ok();
                        ollama_process::stop(&ollama_state);
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
