//! Layer 1 - file saves and git commits, via the `notify` crate.
//! A change to `<repo>/.git/logs/HEAD` is treated as a commit and resolved
//! to its subject line via `git log`, rather than logged as a plain write.

use super::{db, redaction::scan, AgentConfig};
use notify::{EventKind, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const IGNORED_DIR_NAMES: &[&str] = &[
    "node_modules", ".git", "__pycache__", ".venv", "venv", "dist", "build",
    ".next", ".idea", ".vscode", ".pytest_cache", "target", ".turbo",
];

fn is_ignored(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        IGNORED_DIR_NAMES.contains(&s.as_ref())
    })
}

fn is_git_head_log(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == "HEAD")
        && path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "logs")
        && path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == ".git")
}

fn latest_commit_subject(repo_root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["-C", &repo_root.to_string_lossy(), "log", "-1", "--pretty=%s"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let subject = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!subject.is_empty()).then_some(subject)
}

fn handle_path(cfg: &AgentConfig, path: &Path) {
    if is_git_head_log(path) {
        // <repo>/.git/logs/HEAD -> repo root is three levels up.
        if let Some(repo_root) = path.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
            if let Some(subject) = latest_commit_subject(repo_root) {
                db::insert_raw(
                    &cfg.db_path,
                    &db::now_iso(),
                    "git_commit",
                    None,
                    None,
                    scan(Some(&repo_root.to_string_lossy())).as_deref(),
                    scan(Some(&subject)).as_deref(),
                );
            }
        }
        return;
    }

    if is_ignored(path) || path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
        return;
    }

    db::insert_raw(
        &cfg.db_path,
        &db::now_iso(),
        "file",
        None,
        None,
        scan(Some(&path.to_string_lossy())).as_deref(),
        None,
    );
}

pub fn run(cfg: Arc<AgentConfig>, stop: Arc<AtomicBool>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("file watcher failed to start: {e}");
            return;
        }
    };

    let mut watched_any = false;
    for dir in &cfg.watch_dirs {
        if dir.is_dir() {
            if watcher.watch(dir, RecursiveMode::Recursive).is_ok() {
                watched_any = true;
            }
        } else {
            log::warn!("watch dir does not exist, skipping: {}", dir.display());
        }
    }
    if !watched_any {
        log::info!("no valid watch directories configured; file watcher idle");
        return;
    }

    while !stop.load(Ordering::Relaxed) {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => {
                if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    for path in &event.paths {
                        if path.is_file() {
                            handle_path(&cfg, path);
                        }
                    }
                }
            }
            Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_head_log_detection() {
        assert!(is_git_head_log(Path::new("/repo/.git/logs/HEAD")));
        assert!(!is_git_head_log(Path::new("/repo/.git/HEAD")));
        assert!(!is_git_head_log(Path::new("/repo/logs/HEAD")));
    }

    #[test]
    fn ignores_dependency_dirs() {
        assert!(is_ignored(Path::new("/p/node_modules/x/y.js")));
        assert!(is_ignored(Path::new("/p/target/debug/foo")));
        assert!(!is_ignored(Path::new("/p/src/main.rs")));
    }
}
