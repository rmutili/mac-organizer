//! Runs a batch through the local Claude Code CLI (`claude -p`), so the app can
//! use the person's Claude login instead of an API key.
//!
//! The CLI runs locked down: no tools, no MCP servers, no settings or project
//! CLAUDE.md, no saved session. File metadata goes in over stdin, never as
//! command-line arguments.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use super::processes::{CANCELLED, ProcessRegistry};

use super::plan::RawPlan;

/// GUI apps launched from Finder don't inherit the shell PATH, so check the
/// usual install locations before falling back to PATH.
const CANDIDATE_DIRS: [&str; 4] = [
    ".local/bin",
    ".claude/local",
    "/opt/homebrew/bin",
    "/usr/local/bin",
];

pub fn find_claude_binary(home: Option<&Path>, path_env: Option<&str>) -> Option<PathBuf> {
    let candidates = CANDIDATE_DIRS.iter().filter_map(|dir| {
        if dir.starts_with('/') {
            Some(PathBuf::from(dir))
        } else {
            home.map(|h| h.join(dir))
        }
    });
    let from_path = path_env
        .into_iter()
        .flat_map(|p| p.split(':'))
        .filter(|p| !p.is_empty())
        .map(PathBuf::from);
    candidates
        .chain(from_path)
        .map(|dir| dir.join("claude"))
        .find(|bin| bin.is_file())
}

pub fn locate() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = std::env::var("PATH").ok();
    find_claude_binary(home.as_deref(), path.as_deref())
}

/// CLI arguments and stdin for one Messages-API-shaped request body.
pub fn cli_invocation(body: &Value) -> Result<(Vec<String>, String), String> {
    let field = |value: &Value, what: &str| -> Result<String, String> {
        value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("request is missing {what}"))
    };
    let model = field(&body["model"], "model")?;
    let system = field(&body["system"], "system prompt")?;
    let stdin = field(&body["messages"][0]["content"], "user message")?;
    let schema = &body["tools"][0]["input_schema"];
    if !schema.is_object() {
        return Err("request is missing the plan schema".into());
    }
    let args = [
        "-p",
        "--model",
        &model,
        "--output-format",
        "json",
        "--json-schema",
        &schema.to_string(),
        "--system-prompt",
        &system,
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--no-session-persistence",
        "--disable-slash-commands",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    Ok((args, stdin))
}

pub fn parse_cli_output(stdout: &str) -> Result<RawPlan, String> {
    let output: Value = serde_json::from_str(stdout).map_err(|_| {
        "Claude Code returned output that isn't JSON. Is it up to date?".to_string()
    })?;
    if output["is_error"] == true {
        let detail = output["result"].as_str().unwrap_or("unknown error");
        return Err(format!("Claude Code reported an error: {detail}"));
    }
    let plan = output
        .get("structured_output")
        .filter(|v| v.is_object())
        .ok_or("Claude Code didn't return a plan.")?;
    serde_json::from_value(plan.clone())
        .map_err(|e| format!("Claude Code returned a plan in an unexpected format: {e}"))
}

/// Blocking: run on a blocking thread. The process is tracked in `processes`
/// so Cancel (or quitting the app) can stop it.
pub fn run_batch(
    binary: &Path,
    body: &Value,
    processes: &ProcessRegistry,
) -> Result<RawPlan, String> {
    if processes.is_cancelled() {
        return Err(CANCELLED.into());
    }
    let (args, stdin) = cli_invocation(body)?;
    let child = Command::new(binary)
        .args(&args)
        // A neutral directory so no project CLAUDE.md is picked up.
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start Claude Code: {e}"))?;
    let pid = child.id();
    processes.register(pid);
    if processes.is_cancelled() {
        // Cancel landed between the check above and the spawn.
        processes.kill_all();
    }
    let output = feed_and_wait(child, &stdin);
    processes.unregister(pid);
    if processes.is_cancelled() {
        return Err(CANCELLED.into());
    }
    let output = output?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() && stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let hint = stderr.lines().last().unwrap_or("no details");
        return Err(format!(
            "Claude Code failed ({}). If you're not logged in, run `claude` in Terminal once. Details: {hint}",
            output.status
        ));
    }
    parse_cli_output(&stdout)
}

fn feed_and_wait(
    mut child: std::process::Child,
    stdin: &str,
) -> Result<std::process::Output, String> {
    let written = child
        .stdin
        .take()
        .ok_or_else(|| "Couldn't send data to Claude Code.".to_string())
        .and_then(|mut pipe| {
            pipe.write_all(stdin.as_bytes())
                .map_err(|e| format!("Couldn't send data to Claude Code: {e}"))
        });
    // Always reap the child, even if writing failed, so it isn't left running.
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Claude Code didn't finish: {e}"));
    written.and(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::request::{SYSTEM_PROMPT, build_folders_request, build_request};
    use serde_json::json;

    #[test]
    fn cancelled_run_never_starts_the_cli() {
        let registry = ProcessRegistry::default();
        registry.cancel();
        // A binary that doesn't exist: reaching spawn would give a different error.
        let result = run_batch(Path::new("/nonexistent/claude"), &json!({}), &registry);
        assert_eq!(result, Err(CANCELLED.to_string()));
    }

    #[test]
    fn finds_binary_in_home_install_dir_before_path() {
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        for dir in [home.path().join(".local/bin"), other.path().to_path_buf()] {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("claude"), "").unwrap();
        }

        let found = find_claude_binary(Some(home.path()), other.path().to_str());

        assert_eq!(found, Some(home.path().join(".local/bin/claude")));
    }

    #[test]
    fn falls_back_to_path_and_returns_none_when_missing() {
        let home = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        std::fs::write(bin.path().join("claude"), "").unwrap();
        let path = format!("/nonexistent:{}", bin.path().display());

        assert_eq!(
            find_claude_binary(Some(home.path()), Some(&path)),
            Some(bin.path().join("claude"))
        );
        let empty = tempfile::tempdir().unwrap();
        // The absolute candidate dirs may hold a real install on this machine,
        // so only assert "none" when they don't.
        if CANDIDATE_DIRS
            .iter()
            .all(|d| !d.starts_with('/') || !Path::new(d).join("claude").is_file())
        {
            assert_eq!(find_claude_binary(Some(empty.path()), Some("")), None);
        }
    }

    #[test]
    fn invocation_disables_tools_and_sends_items_on_stdin() {
        let body = build_request(
            "claude-opus-5-5",
            "Downloads",
            &[],
            &["Docs".to_string()],
            0,
        );

        let (args, stdin) = cli_invocation(&body).unwrap();

        let value_after = |flag: &str| {
            let at = args.iter().position(|a| a == flag).unwrap();
            args[at + 1].clone()
        };
        assert_eq!(value_after("--model"), "claude-opus-5-5");
        assert_eq!(value_after("--tools"), "");
        assert_eq!(value_after("--setting-sources"), "");
        assert_eq!(value_after("--system-prompt"), SYSTEM_PROMPT);
        let schema: Value = serde_json::from_str(&value_after("--json-schema")).unwrap();
        assert_eq!(schema, body["tools"][0]["input_schema"]);
        assert!(args.contains(&"--no-session-persistence".to_string()));
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        assert_eq!(stdin, body["messages"][0]["content"].as_str().unwrap());
    }

    #[test]
    fn parses_structured_output() {
        let stdout = json!({
            "type": "result", "is_error": false,
            "structured_output": {
                "folders": [{ "name": "Installers", "description": "" }],
                "assignments": [{ "item_id": 0, "folder": "Installers", "reason": "dmg" }],
                "deletion_candidates": []
            }
        })
        .to_string();

        let plan = parse_cli_output(&stdout).unwrap();

        assert_eq!(plan.folders[0].name, "Installers");
        assert_eq!(plan.assignments.len(), 1);
    }

    #[test]
    fn reports_cli_errors_and_missing_plans() {
        let error = json!({ "is_error": true, "result": "Not logged in" }).to_string();
        assert!(
            parse_cli_output(&error)
                .unwrap_err()
                .contains("Not logged in")
        );

        let no_plan = json!({ "is_error": false, "result": "hi" }).to_string();
        assert_eq!(
            parse_cli_output(&no_plan).unwrap_err(),
            "Claude Code didn't return a plan."
        );

        assert!(parse_cli_output("not json").is_err());
    }

    /// Calls the real Claude Code CLI (uses the logged-in account).
    /// Run with: cargo test --lib live_claude_code -- --ignored
    #[test]
    #[ignore]
    fn live_claude_code_returns_a_plan() {
        use crate::scanner::types::{Category, FileItem};
        let item = |id, name: &str, category| FileItem {
            id,
            name: name.into(),
            extension: String::new(),
            category,
            is_dir: false,
            size_bytes: 1_000_000,
            file_count: 1,
            modified_at: 0,
            accessed_at: 0,
            duplicate_group: None,
            sensitive: false,
        };
        let items = [
            item(0, "GoogleChrome.dmg", Category::Installer),
            item(1, "invoice-march.pdf", Category::Document),
        ];
        let binary = locate().expect("claude CLI not found");

        let proposed = run_batch(
            &binary,
            &build_folders_request("claude-haiku-5-5", "Downloads", &items),
            &ProcessRegistry::default(),
        )
        .unwrap();
        assert!(!proposed.folders.is_empty());

        let folders = vec!["Installers".to_string(), "Invoices".to_string()];
        let body = build_request(
            "claude-haiku-5-5",
            "Downloads",
            &items,
            &folders,
            1_800_000_000,
        );
        let plan = run_batch(&binary, &body, &ProcessRegistry::default()).unwrap();

        assert_eq!(plan.assignments.len(), 2);
        assert!(plan.assignments.iter().all(|a| folders.contains(&a.folder)));
    }
}
