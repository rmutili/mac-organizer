//! Tauri commands: the only surface the webview can call.

pub mod classify;
pub mod ops;
pub mod scan;

use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::claude::plan::Plan;
use crate::scanner::types::Scan;

/// The latest scan and the plan built from it. Later commands (classify,
/// apply) resolve item IDs against the scan, so the UI never sends paths.
/// A new scan clears the plan, because its item IDs no longer match.
#[derive(Default)]
pub struct AppState {
    pub scan: Mutex<Option<Scan>>,
    pub plan: Mutex<Option<Plan>>,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    mutex
        .lock()
        .map_err(|_| "Internal state is unavailable; restart the app.".to_string())
}

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
