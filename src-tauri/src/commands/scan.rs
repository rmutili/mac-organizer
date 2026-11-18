use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{AppState, lock, unix_now};
use crate::scanner::duplicates::{find_duplicate_groups, with_duplicate_groups};
use crate::scanner::heatmap::{Heatmap, build_heatmap};
use crate::scanner::roots::{check_root, display_name};
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

/// Which folder to scan. The webview names a location, never a path; a
/// `Picked` folder is the one chosen in the native folder dialog.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    Downloads,
    Desktop,
    Documents,
    Picked,
}

#[derive(Serialize)]
pub struct PickedFolder {
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct ScanResponse {
    /// Shown in the UI ("Downloads"); the full path is for display only.
    pub name: String,
    pub root: String,
    pub items: Vec<FileItem>,
    pub skipped: Vec<String>,
    pub heatmap: Heatmap,
}

fn home_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .home_dir()
        .map_err(|e| format!("Couldn't locate your home folder: {e}"))
}

fn resolve_root(app: &AppHandle, state: &AppState, location: Location) -> Result<PathBuf, String> {
    let paths = app.path();
    let (path, label) = match location {
        Location::Downloads => (paths.download_dir(), "Downloads"),
        Location::Desktop => (paths.desktop_dir(), "Desktop"),
        Location::Documents => (paths.document_dir(), "Documents"),
        Location::Picked => {
            return lock(&state.picked)?
                .clone()
                .ok_or_else(|| "Choose a folder first.".to_string());
        }
    };
    let path = path.map_err(|e| format!("Couldn't locate your {label} folder: {e}"))?;
    check_root(&path, &home_dir(app)?)
}

/// Opens the native folder dialog. Returns `None` if the person cancels, and
/// an error explaining why if the folder can't be organized.
#[tauri::command]
pub async fn pick_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<PickedFolder>, String> {
    let (tx, rx) = futures::channel::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose a folder to organize")
        .pick_folder(move |picked| {
            let _ = tx.send(picked);
        });
    let Some(picked) = rx
        .await
        .map_err(|_| "The folder dialog closed unexpectedly.")?
    else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|_| "That location isn't a folder on this Mac.".to_string())?;
    let root = check_root(&path, &home_dir(&app)?)?;
    let folder = PickedFolder {
        name: display_name(&root),
        path: root.display().to_string(),
    };
    *lock(&state.picked)? = Some(root);
    Ok(Some(folder))
}

#[tauri::command]
pub async fn scan_folder(
    location: Location,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanResponse, String> {
    let root = resolve_root(&app, &state, location)?;
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
        name: display_name(&scan.root),
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
