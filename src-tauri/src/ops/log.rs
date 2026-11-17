//! The record of an Apply, persisted so Undo works even after a restart.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::apply::Failure;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Moved,
    Trashed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub kind: OperationKind,
    pub from: PathBuf,
    pub to: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationLog {
    pub applied_at: i64,
    pub entries: Vec<LogEntry>,
    /// Folders this Apply created; Undo removes them if they end up empty.
    pub created_folders: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UndoReport {
    pub restored: usize,
    pub failures: Vec<Failure>,
}

/// Summary of the most recent Apply that can still be undone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UndoAvailable {
    pub applied_at: i64,
    pub moved: usize,
    pub trashed: usize,
}

const ACTIVE_SUFFIX: &str = ".json";
const UNDONE_SUFFIX: &str = ".undone.json";

pub fn save_log(dir: &Path, log: &OperationLog) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("couldn't create the undo folder: {e}"))?;
    let path = dir.join(format!("{}{ACTIVE_SUFFIX}", log.applied_at));
    let json = serde_json::to_vec_pretty(log).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("couldn't save the undo log: {e}"))?;
    Ok(path)
}

/// The newest log that hasn't been undone yet.
pub fn latest_log(dir: &Path) -> Result<Option<(PathBuf, OperationLog)>, String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(None);
    };
    let newest = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.ends_with(ACTIVE_SUFFIX) && !name.ends_with(UNDONE_SUFFIX)
        })
        .max_by_key(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(i64::MIN)
        });
    let Some(path) = newest else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|e| format!("couldn't read the undo log: {e}"))?;
    let log =
        serde_json::from_slice(&bytes).map_err(|e| format!("the undo log is damaged: {e}"))?;
    Ok(Some((path, log)))
}

pub fn summarize(log: &OperationLog) -> UndoAvailable {
    let count = |kind| log.entries.iter().filter(|e| e.kind == kind).count();
    UndoAvailable {
        applied_at: log.applied_at,
        moved: count(OperationKind::Moved),
        trashed: count(OperationKind::Trashed),
    }
}

/// Reverse every entry (newest first). Never overwrites: if something now
/// occupies the original location, that entry is reported as a failure.
pub fn undo(log: &OperationLog) -> UndoReport {
    let mut restored = 0;
    let mut failures = Vec::new();
    for entry in log.entries.iter().rev() {
        let name = entry
            .from
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let result = if fs::symlink_metadata(&entry.from).is_ok() {
            Err("something else now has its original name".to_string())
        } else if fs::symlink_metadata(&entry.to).is_err() {
            Err("it's no longer where it was moved to".to_string())
        } else {
            fs::rename(&entry.to, &entry.from).map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => restored += 1,
            Err(error) => failures.push(Failure { name, error }),
        }
    }
    for folder in log.created_folders.iter().rev() {
        // remove_dir only succeeds on an empty folder, so nothing else is lost.
        let _ = fs::remove_dir(folder);
    }
    UndoReport { restored, failures }
}

/// Mark a log as undone (kept for the record, no longer offered for Undo).
pub fn mark_undone(path: &Path) -> Result<(), String> {
    let undone = path.with_extension("undone.json");
    fs::rename(path, undone).map_err(|e| format!("couldn't update the undo log: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::plan::Folder;
    use crate::ops::apply::tests::{FakeTrash, id, setup};
    use crate::ops::apply::{Decisions, MoveDecision, execute, validate_decisions};

    fn apply_sample() -> (tempfile::TempDir, PathBuf, OperationLog) {
        let (tmp, root, items) = setup(&["a.pdf", "old.dmg"]);
        let trash = FakeTrash::new(&tmp.path().join("FakeTrash"));
        let decisions = Decisions {
            moves: vec![MoveDecision {
                item_id: id(&items, "a.pdf"),
                folder: "Docs".into(),
            }],
            trash: vec![id(&items, "old.dmg")],
        };
        let folders = vec![Folder {
            name: "Docs".into(),
            description: String::new(),
        }];
        let ops = validate_decisions(&decisions, &items, &folders).unwrap();
        let (_, log) = execute(&root, &ops, &trash, 100);
        (tmp, root, log)
    }

    #[test]
    fn undo_restores_moves_and_trash_and_removes_empty_created_folders() {
        let (_tmp, root, log) = apply_sample();

        let report = undo(&log);

        assert_eq!(report.restored, 2);
        assert!(report.failures.is_empty());
        assert!(root.join("a.pdf").is_file());
        assert!(root.join("old.dmg").is_file());
        assert!(!root.join("Docs").exists());
    }

    #[test]
    fn undo_never_overwrites_a_new_file_with_the_same_name() {
        let (_tmp, root, log) = apply_sample();
        fs::write(root.join("a.pdf"), b"new download").unwrap();

        let report = undo(&log);

        assert_eq!(report.restored, 1);
        assert_eq!(report.failures[0].name, "a.pdf");
        assert_eq!(fs::read(root.join("a.pdf")).unwrap(), b"new download");
        assert!(
            root.join("Docs/a.pdf").is_file(),
            "folder kept since it isn't empty"
        );
    }

    #[test]
    fn saves_finds_newest_and_marks_undone() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("undo");
        let log = |applied_at| OperationLog {
            applied_at,
            entries: vec![],
            created_folders: vec![],
        };
        save_log(&dir, &log(5)).unwrap();
        let newest = save_log(&dir, &log(9)).unwrap();

        let (path, found) = latest_log(&dir).unwrap().unwrap();
        assert_eq!((path.clone(), found.applied_at), (newest, 9));

        mark_undone(&path).unwrap();
        assert_eq!(latest_log(&dir).unwrap().unwrap().1.applied_at, 5);
        assert!(latest_log(&tmp.path().join("missing")).unwrap().is_none());
    }
}
