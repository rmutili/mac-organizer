use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::{AppState, lock, unix_now};
use crate::scanner::duplicates::{find_duplicate_groups, with_duplicate_groups};
use crate::scanner::heatmap::{Heatmap, build_heatmap};
use crate::scanner::types::{FileItem, Scan};
use crate::scanner::walk::{ScanError, scan_root_with_progress};

pub const SCAN_PROGRESS_EVENT: &str = "scan-progress";

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ScanPhase {
    Listing,
    Duplicates,
}

#[derive(Clone, Serialize)]
struct ScanProgress {
    phase: ScanPhase,
    done: usize,
    total: usize,
    name: String,
}

#[derive(Serialize)]
pub struct ScanResponse {
    pub root: String,
    pub items: Vec<FileItem>,
    pub skipped: Vec<String>,
    pub heatmap: Heatmap,
}

#[tauri::command]
pub async fn scan_downloads(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanResponse, String> {
    let root = app
        .path()
        .download_dir()
        .map_err(|e| format!("Couldn't locate your Downloads folder: {e}"))?;
    let now = unix_now();

    let emitter = app.clone();
    let scan = tauri::async_runtime::spawn_blocking(move || {
        // Best-effort UI feedback; a failed emit must not abort the scan.
        let report = |phase| {
            let emitter = emitter.clone();
            move |done, total, name: &str| {
                let progress = ScanProgress {
                    phase,
                    done,
                    total,
                    name: name.to_string(),
                };
                let _ = emitter.emit(SCAN_PROGRESS_EVENT, progress);
            }
        };
        let listed = scan_root_with_progress(&root, now, report(ScanPhase::Listing))?;
        let groups =
            find_duplicate_groups(&listed.root, &listed.items, report(ScanPhase::Duplicates));
        Ok::<_, ScanError>(Scan {
            items: with_duplicate_groups(&listed.items, &groups),
            ..listed
        })
    })
    .await
    .map_err(|e| format!("Scan was interrupted: {e}"))?
    .map_err(|e| e.to_string())?;

    let response = ScanResponse {
        root: scan.root.display().to_string(),
        heatmap: build_heatmap(&scan.items, now),
        items: scan.items.clone(),
        skipped: scan.skipped.clone(),
    };
    *lock(&state.plan)? = None;
    let mut latest = lock(&state.scan)?;
    *latest = Some(scan);
    Ok(response)
}
