//! Reads/writes the same two files the Python daemon reads:
//! `agent/.env` (token, API URL - python-dotenv format) and
//! `~/.life-update-agent/config.json` (exclude-list + selected model -
//! see `agent/src/life_update_agent/config.py`). Both sides treat these
//! files as the shared source of truth; nothing here duplicates state.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::DEFAULT_MODEL;

pub fn state_path() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".life-update-agent")
        .join("config.json")
}

// Mirrors config.py's DEFAULT_EXCLUDE_LIST.
fn default_apps() -> Vec<String> {
    ["1Password", "Bitwarden", "KeePassXC", "LastPass", "Keychain Access"]
        .iter().map(|s| s.to_string()).collect()
}

fn default_title_patterns() -> Vec<String> {
    [
        r"(?i)\bbank\b", r"(?i)\bpaypal\b", r"(?i)\bwise\b", r"(?i)\bcoinbase\b",
        r"(?i)\brobinhood\b", r"(?i)\bvenmo\b", r"(?i)\bchase\b", r"(?i)\bwells fargo\b",
        r"(?i)\bpassword\b", r"(?i)\bincognito\b", r"(?i)\bprivate browsing\b",
    ].iter().map(|s| s.to_string()).collect()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalState {
    #[serde(default = "default_apps")]
    pub apps: Vec<String>,
    #[serde(default = "default_title_patterns")]
    pub title_patterns: Vec<String>,
    #[serde(default = "default_model_owned")]
    pub ollama_model: String,
}

fn default_model_owned() -> String {
    DEFAULT_MODEL.to_string()
}

impl Default for LocalState {
    fn default() -> Self {
        LocalState {
            apps: default_apps(),
            title_patterns: default_title_patterns(),
            ollama_model: DEFAULT_MODEL.to_string(),
        }
    }
}

pub fn read_state() -> LocalState {
    let path = state_path();
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => LocalState::default(),
    }
}

pub fn write_state(state: &LocalState) -> Result<(), String> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let contents = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(&path, contents).map_err(|e| e.to_string())
}

/// `.env` is a flat KEY=value file (python-dotenv format). We parse it into
/// an order-preserving map, patch in the given keys, and write it back -
/// any keys/comments we don't touch are left alone.
fn parse_env_file(path: &Path) -> (Vec<String>, BTreeMap<String, usize>) {
    let mut lines: Vec<String> = Vec::new();
    let mut key_lines: BTreeMap<String, usize> = BTreeMap::new();

    if let Ok(contents) = fs::read_to_string(path) {
        for line in contents.lines() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('#') {
                if let Some((key, _)) = trimmed.split_once('=') {
                    key_lines.insert(key.trim().to_string(), lines.len());
                }
            }
            lines.push(line.to_string());
        }
    }

    (lines, key_lines)
}

pub fn write_env_values(agent_dir: &Path, values: &[(&str, &str)]) -> Result<(), String> {
    let env_path = agent_dir.join(".env");
    let (mut lines, mut key_lines) = parse_env_file(&env_path);

    for (key, value) in values {
        let formatted = format!("{key}={value}");
        if let Some(&idx) = key_lines.get(*key) {
            lines[idx] = formatted;
        } else {
            key_lines.insert(key.to_string(), lines.len());
            lines.push(formatted);
        }
    }

    fs::write(&env_path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

pub fn read_env_values(agent_dir: &Path, keys: &[&str]) -> BTreeMap<String, String> {
    let env_path = agent_dir.join(".env");
    let mut result = BTreeMap::new();
    if let Ok(contents) = fs::read_to_string(&env_path) {
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                if keys.contains(&key) {
                    result.insert(key.to_string(), value.trim().to_string());
                }
            }
        }
    }
    result
}
