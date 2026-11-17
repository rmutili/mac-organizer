use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use super::{AppState, lock, unix_now};
use crate::ops::apply::{ApplyReport, Decisions, Failure, execute, validate_decisions};
use crate::ops::log::{
    UndoAvailable, UndoReport, latest_log, mark_undone, save_log, summarize, undo,
};
use crate::ops::preview;
use crate::ops::trash::MacTrash;
use crate::scanner::types::ItemId;

const THUMBNAIL_SIZE: u32 = 512;

fn undo_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join("undo"))
        .map_err(|e| format!("Couldn't find the app's data folder: {e}"))
}

/// Resolve an item ID from the current scan to its path on disk.
fn item_path(state: &AppState, item_id: ItemId) -> Result<(PathBuf, i64), String> {
    let scan = lock(&state.scan)?;
    let scan = scan.as_ref().ok_or("Scan your Downloads folder first.")?;
    let item = scan
        .items
        .iter()
        .find(|i| i.id == item_id)
        .ok_or("That item isn't in the current scan; scan again.")?;
    Ok((scan.root.join(&item.name), scan.scanned_at))
}

/// Execute the reviewed decisions. The scan and plan are cleared afterwards
/// because item IDs no longer match what's on disk; the UI rescans.
#[tauri::command]
pub async fn apply_plan(
    decisions: Decisions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyReport, String> {
    let (root, operations) = {
        let scan = lock(&state.scan)?;
        let scan = scan.as_ref().ok_or("Scan your Downloads folder first.")?;
        let plan = lock(&state.plan)?;
        let plan = plan.as_ref().ok_or("Ask Claude to organize first.")?;
        let operations = validate_decisions(&decisions, &scan.items, &plan.folders)?;
        (scan.root.clone(), operations)
    };
    if operations.is_empty() {
        return Err("Nothing is selected to apply.".into());
    }
    let log_dir = undo_dir(&app)?;

    let (report, log) = tauri::async_runtime::spawn_blocking(move || {
        execute(&root, &operations, &MacTrash, unix_now())
    })
    .await
    .map_err(|e| format!("Apply was interrupted: {e}"))?;

    *lock(&state.plan)? = None;
    *lock(&state.scan)? = None;
    if log.entries.is_empty() {
        return Ok(report);
    }
    match save_log(&log_dir, &log) {
        Ok(_) => Ok(report),
        Err(error) => Ok(ApplyReport {
            failures: [
                report.failures,
                vec![Failure {
                    name: "Undo log".into(),
                    error: format!("{error}. These changes can't be undone from the app."),
                }],
            ]
            .concat(),
            ..report
        }),
    }
}

#[tauri::command]
pub fn last_apply(app: AppHandle) -> Result<Option<UndoAvailable>, String> {
    Ok(latest_log(&undo_dir(&app)?)?.map(|(_, log)| summarize(&log)))
}

#[tauri::command]
pub async fn undo_last_apply(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UndoReport, String> {
    let (path, log) = latest_log(&undo_dir(&app)?)?.ok_or("There's nothing to undo.")?;
    let report = tauri::async_runtime::spawn_blocking(move || undo(&log))
        .await
        .map_err(|e| format!("Undo was interrupted: {e}"))?;
    mark_undone(&path)?;
    *lock(&state.plan)? = None;
    *lock(&state.scan)? = None;
    Ok(report)
}

#[tauri::command]
pub async fn thumbnail(
    item_id: ItemId,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let (path, scanned_at) = item_path(&state, item_id)?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Couldn't find the cache folder: {e}"))?
        .join("thumbnails")
        .join(scanned_at.to_string());
    tauri::async_runtime::spawn_blocking(move || preview::thumbnail(&path, &cache, THUMBNAIL_SIZE))
        .await
        .map_err(|e| format!("Preview was interrupted: {e}"))?
}

#[tauri::command]
pub fn quick_look(item_id: ItemId, state: State<'_, AppState>) -> Result<(), String> {
    preview::quick_look(&item_path(&state, item_id)?.0)
}

#[tauri::command]
pub fn reveal_in_finder(item_id: ItemId, state: State<'_, AppState>) -> Result<(), String> {
    preview::reveal_in_finder(&item_path(&state, item_id)?.0)
}
