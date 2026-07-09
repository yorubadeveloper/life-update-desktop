//! Layer 1 - screen content watching (opt-in, off by default).
//!
//! Hybrid cadence: a periodic timer, reset immediately whenever the active
//! window changes. Two processing paths by vision engine:
//! - "native" (default): Apple Vision OCR, fast enough to run inline.
//! - An Ollama vision model: multi-second calls, so frames go to the
//!   bounded in-memory FrameQueue for the idle-gated worker instead.

use super::frame_queue::{FrameQueue, PendingFrame};
use super::window_tracker::read_active_window;
use super::{db, idle, is_excluded, redaction::scan, AgentConfig};
use crate::vision_models::is_ollama_backed;
use image::ImageEncoder;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Pure cadence decision: capture when the active window changed since the
/// last capture, or the configured interval has elapsed.
pub fn should_capture(
    window_key: &str,
    last_window_key: Option<&str>,
    now: f64,
    last_capture_at: Option<f64>,
    interval_seconds: f64,
) -> bool {
    match (last_window_key, last_capture_at) {
        (None, _) | (_, None) => true,
        (Some(last_key), Some(last_at)) => {
            window_key != last_key || now - last_at >= interval_seconds
        }
    }
}

fn capture_primary_png() -> Option<Vec<u8>> {
    let monitors = xcap::Monitor::all().ok()?;
    let monitor = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())?;
    let img = monitor.capture_image().ok()?;
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(buf)
}

pub fn run(cfg: Arc<AgentConfig>, frames: Arc<FrameQueue>, stop: Arc<AtomicBool>) {
    if !idle::ensure_screen_recording_permission() {
        log::warn!("screen recording permission not granted; screen watching disabled this session");
        return;
    }

    let ollama_vision = is_ollama_backed(&cfg.vision_engine);
    let mut last_window_key: Option<String> = None;
    let mut last_capture_at: Option<f64> = None;
    let start = std::time::Instant::now();

    while !stop.load(Ordering::Relaxed) {
        let (app_name, title) = read_active_window();
        let window_key = format!("{}|{}", app_name.as_deref().unwrap_or(""), title.as_deref().unwrap_or(""));
        let now = start.elapsed().as_secs_f64();

        let capture = should_capture(
            &window_key,
            last_window_key.as_deref(),
            now,
            last_capture_at,
            cfg.screen_interval_seconds,
        ) && !is_excluded(&cfg, app_name.as_deref(), title.as_deref());

        if capture {
            last_window_key = Some(window_key);
            last_capture_at = Some(now);

            if let Some(png) = capture_primary_png() {
                if ollama_vision {
                    frames.push(PendingFrame {
                        png_bytes: png,
                        app_name: app_name.clone(),
                        title: title.clone(),
                        ts: db::now_iso(),
                    });
                } else {
                    match super::vision_ocr::ocr_png(&png) {
                        Ok(text) if !text.trim().is_empty() => {
                            db::insert_raw(
                                &cfg.db_path,
                                &db::now_iso(),
                                "screen_text",
                                scan(app_name.as_deref()).as_deref(),
                                scan(title.as_deref()).as_deref(),
                                None,
                                scan(Some(&text)).as_deref(),
                            );
                        }
                        Ok(_) => {}
                        Err(e) => log::warn!("native OCR failed: {e}"),
                    }
                }
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::should_capture;

    #[test]
    fn first_call_always_captures() {
        assert!(should_capture("a|b", None, 0.0, None, 120.0));
    }

    #[test]
    fn window_change_captures_immediately() {
        assert!(should_capture("new|win", Some("old|win"), 5.0, Some(4.0), 120.0));
    }

    #[test]
    fn same_window_waits_for_interval() {
        assert!(!should_capture("a|b", Some("a|b"), 100.0, Some(50.0), 120.0));
        assert!(should_capture("a|b", Some("a|b"), 171.0, Some(50.0), 120.0));
    }
}
