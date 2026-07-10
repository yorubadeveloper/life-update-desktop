//! Idle detection and CPU load - gates when the inference worker may run.
//!
//! Unlike the Python agent (which needed pynput keyboard/mouse listeners -
//! an Accessibility-permission-hungry, always-on event tap), this asks the
//! OS directly via CGEventSourceSecondsSinceLastEventType: zero listeners,
//! zero permissions, zero cost.

use std::time::Duration;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

const HID_SYSTEM_STATE: i32 = 1;
const ANY_INPUT_EVENT_TYPE: u32 = u32::MAX; // kCGAnyInputEventType

pub fn seconds_since_last_input() -> f64 {
    unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT_TYPE) }
}

pub fn is_user_idle(threshold_minutes: f64) -> bool {
    seconds_since_last_input() >= threshold_minutes * 60.0
}

pub fn is_load_low(ceiling_percent: f64) -> bool {
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_usage();
    std::thread::sleep(Duration::from_millis(500));
    sys.refresh_cpu_usage();
    f64::from(sys.global_cpu_usage()) <= ceiling_percent
}

pub fn is_safe_to_run_inference(idle_threshold_minutes: f64, cpu_load_ceiling_percent: f64) -> bool {
    is_user_idle(idle_threshold_minutes) && is_load_low(cpu_load_ceiling_percent)
}

/// Preflight only - no prompt. Screen Recording gates BOTH window titles
/// (CGWindowList omits kCGWindowName without it) and screen capture, so
/// without it the agent only ever sees bare app names - the root cause of
/// guesswork summaries.
pub fn has_screen_recording_permission() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Actively requests the permission (triggers the system prompt). macOS
/// requires an app relaunch after granting for it to take effect.
pub fn request_screen_recording_permission() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

/// True if macOS Screen Recording permission is granted; if not, actively
/// requests it (which triggers the system prompt) before returning false.
pub fn ensure_screen_recording_permission() -> bool {
    unsafe {
        if CGPreflightScreenCaptureAccess() {
            true
        } else {
            CGRequestScreenCaptureAccess()
        }
    }
}
