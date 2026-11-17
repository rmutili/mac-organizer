use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::{AppState, lock, unix_now};
use crate::claude::classify::{ALLOWED_MODELS, ClassifyProgress, DEFAULT_MODEL, classify};
use crate::claude::client::{http_client, send_message};
use crate::claude::plan::Plan;
use crate::claude::processes::CANCELLED;
use crate::claude::response::parse_plan;
use crate::claude::{cli, keychain};

/// How the app reaches Claude: the local Claude Code CLI (the person's Claude
/// login) or the Messages API with a key stored in Keychain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    ClaudeCode,
    ApiKey,
}

#[derive(Serialize)]
pub struct BackendStatus {
    pub claude_code: bool,
    pub api_key: bool,
}

#[tauri::command]
pub fn claude_backends() -> Result<BackendStatus, String> {
    Ok(BackendStatus {
        claude_code: cli::locate().is_some(),
        api_key: keychain::load_api_key()?.is_some(),
    })
}

#[tauri::command]
pub fn set_api_key(key: String) -> Result<(), String> {
    keychain::save_api_key(&keychain::normalize_api_key(&key)?)
}

#[tauri::command]
pub fn clear_api_key() -> Result<(), String> {
    keychain::delete_api_key()
}

fn resolve_model(model: Option<&str>) -> Result<&'static str, String> {
    match model {
        None => Ok(DEFAULT_MODEL),
        Some(m) => ALLOWED_MODELS
            .into_iter()
            .find(|allowed| *allowed == m)
            .ok_or_else(|| format!("Unsupported model: {m}")),
    }
}

/// Event the UI listens to while a classification runs.
pub const PROGRESS_EVENT: &str = "classify-progress";

/// Stop the running Organize: kills Claude Code processes and makes the run
/// return "Cancelled." instead of a plan.
#[tauri::command]
pub fn cancel_classify(state: State<'_, AppState>) {
    state.processes.cancel();
}

#[tauri::command]
pub async fn classify_downloads(
    model: Option<String>,
    backend: Option<Backend>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Plan, String> {
    let model = resolve_model(model.as_deref())?;
    let processes = state.processes.clone();
    processes.reset();
    let (items, scanned_at) = {
        let scan = lock(&state.scan)?;
        let scan = scan.as_ref().ok_or("Scan your Downloads folder first.")?;
        (scan.items.clone(), scan.scanned_at)
    };
    let now = unix_now();
    let cli_binary = cli::locate();
    // Progress is best-effort UI feedback; a failed emit must not abort the run.
    let on_progress = |progress: &ClassifyProgress| {
        let _ = app.emit(PROGRESS_EVENT, progress);
    };
    let backend = backend.unwrap_or(if cli_binary.is_some() {
        Backend::ClaudeCode
    } else {
        Backend::ApiKey
    });

    let plan = match backend {
        Backend::ClaudeCode => {
            let binary = cli_binary.ok_or(
                "Claude Code isn't installed. Install it, or add an Anthropic API key instead.",
            )?;
            classify(
                &items,
                model,
                now,
                |body| {
                    let (binary, processes) = (binary.clone(), processes.clone());
                    async move {
                        tauri::async_runtime::spawn_blocking(move || {
                            cli::run_batch(&binary, &body, &processes)
                        })
                        .await
                        .map_err(|e| format!("Claude Code batch was interrupted: {e}"))?
                    }
                },
                on_progress,
            )
            .await?
        }
        Backend::ApiKey => {
            let api_key = keychain::load_api_key()?
                .ok_or("Add your Anthropic API key before asking Claude to organize.")?;
            let http = http_client()?;
            classify(
                &items,
                model,
                now,
                |body| {
                    let (http, api_key) = (http.clone(), api_key.clone());
                    let processes = processes.clone();
                    async move {
                        // HTTP requests already in flight finish; no new ones start.
                        if processes.is_cancelled() {
                            return Err(CANCELLED.to_string());
                        }
                        let response = send_message(&http, &api_key, &body)
                            .await
                            .map_err(|e| e.to_string())?;
                        parse_plan(&response).map_err(|e| e.to_string())
                    }
                },
                on_progress,
            )
            .await?
        }
    };

    // Only keep the plan if no new scan happened while Claude was working.
    let still_current = lock(&state.scan)?
        .as_ref()
        .is_some_and(|s| s.scanned_at == scanned_at);
    if !still_current {
        return Err("Downloads was rescanned while Claude was working. Run organize again.".into());
    }
    *lock(&state.plan)? = Some(plan.clone());
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_defaults_and_allowlist() {
        assert_eq!(resolve_model(None), Ok(DEFAULT_MODEL));
        assert_eq!(
            resolve_model(Some("claude-haiku-5-5")),
            Ok("claude-haiku-5-5")
        );
        assert!(resolve_model(Some("gpt-5")).is_err());
    }
}
