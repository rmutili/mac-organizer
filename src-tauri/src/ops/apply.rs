//! Validating the person's reviewed decisions and executing them.
//!
//! The UI sends item IDs and folder names only. Every entry is checked against
//! the scan and the plan's folders; one bad entry rejects the whole request.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::log::{LogEntry, OperationKind, OperationLog};
use super::trash::Trasher;
use crate::claude::plan::Folder;
use crate::scanner::types::{FileItem, ItemId};

#[derive(Debug, Clone, Deserialize)]
pub struct MoveDecision {
    pub item_id: ItemId,
    pub folder: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Decisions {
    pub moves: Vec<MoveDecision>,
    pub trash: Vec<ItemId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    Move { item: FileItem, folder: String },
    Trash { item: FileItem },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failure {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApplyReport {
    pub moved: usize,
    pub trashed: usize,
    pub freed_bytes: u64,
    pub failures: Vec<Failure>,
}

/// Check the decisions against the scan and the plan's folders.
pub fn validate_decisions(
    decisions: &Decisions,
    items: &[FileItem],
    folders: &[Folder],
) -> Result<Vec<Operation>, String> {
    let find = |id: ItemId| {
        items
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| format!("Item {id} isn't in the current scan; scan again."))
    };
    let canonical = |name: &str| {
        folders
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name.trim()))
            .map(|f| f.name.clone())
            .ok_or_else(|| format!("\"{name}\" isn't one of the plan's folders."))
    };

    let mut seen: HashSet<ItemId> = HashSet::new();
    let mut targets: HashSet<String> = HashSet::new();
    let mut operations = Vec::new();
    for decision in &decisions.moves {
        let item = find(decision.item_id)?;
        let folder = canonical(&decision.folder)?;
        if item.name.eq_ignore_ascii_case(&folder) {
            return Err(format!("\"{}\" can't be moved into itself.", item.name));
        }
        if !seen.insert(item.id) {
            return Err(format!("\"{}\" has more than one decision.", item.name));
        }
        targets.insert(folder.to_lowercase());
        operations.push(Operation::Move {
            item: item.clone(),
            folder,
        });
    }
    for &id in &decisions.trash {
        let item = find(id)?;
        if !seen.insert(item.id) {
            return Err(format!("\"{}\" has more than one decision.", item.name));
        }
        if targets.contains(&item.name.to_lowercase()) {
            return Err(format!(
                "\"{}\" is a destination folder, so it can't go to the Trash.",
                item.name
            ));
        }
        operations.push(Operation::Trash { item: item.clone() });
    }
    Ok(operations)
}

/// `dir/name`, or `dir/name (2).ext`, `(3)`… if that's taken. Never overwrites.
pub fn unique_destination(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let path = Path::new(name);
    let (stem, ext) = match (path.file_stem(), path.extension()) {
        (Some(stem), Some(ext)) if !name.starts_with('.') => (
            stem.to_string_lossy().into_owned(),
            format!(".{}", ext.to_string_lossy()),
        ),
        _ => (name.to_string(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or(first)
}

/// Create `root/folder` if needed. Refuses a symlink or a file in the way.
fn ensure_folder(root: &Path, folder: &str, created: &mut Vec<PathBuf>) -> Result<PathBuf, String> {
    let dir = root.join(folder);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.is_dir() => Ok(dir),
        Ok(_) => Err(format!("\"{folder}\" exists but isn't a regular folder")),
        Err(_) => {
            fs::create_dir(&dir).map_err(|e| format!("couldn't create \"{folder}\": {e}"))?;
            created.push(dir.clone());
            Ok(dir)
        }
    }
}

fn run_one(
    root: &Path,
    operation: &Operation,
    trasher: &dyn Trasher,
    created: &mut Vec<PathBuf>,
) -> Result<LogEntry, String> {
    let item = match operation {
        Operation::Move { item, .. } | Operation::Trash { item } => item,
    };
    let source = root.join(&item.name);
    fs::symlink_metadata(&source).map_err(|_| "it's no longer there".to_string())?;
    match operation {
        Operation::Move { folder, .. } => {
            let dir = ensure_folder(root, folder, created)?;
            let destination = unique_destination(&dir, &item.name);
            fs::rename(&source, &destination).map_err(|e| e.to_string())?;
            Ok(LogEntry {
                kind: OperationKind::Moved,
                from: source,
                to: destination,
            })
        }
        Operation::Trash { .. } => Ok(LogEntry {
            kind: OperationKind::Trashed,
            to: trasher.trash(&source)?,
            from: source,
        }),
    }
}

/// Execute every operation, continuing past individual failures. The log
/// records exactly what succeeded, so Undo can reverse it.
pub fn execute(
    root: &Path,
    operations: &[Operation],
    trasher: &dyn Trasher,
    applied_at: i64,
) -> (ApplyReport, OperationLog) {
    let mut created = Vec::new();
    let mut entries = Vec::new();
    let mut failures = Vec::new();
    let mut freed_bytes = 0;
    for operation in operations {
        let item = match operation {
            Operation::Move { item, .. } | Operation::Trash { item } => item,
        };
        match run_one(root, operation, trasher, &mut created) {
            Ok(entry) => {
                if entry.kind == OperationKind::Trashed {
                    freed_bytes += item.size_bytes;
                }
                entries.push(entry);
            }
            Err(error) => failures.push(Failure {
                name: item.name.clone(),
                error,
            }),
        }
    }
    let count = |kind| {
        entries
            .iter()
            .filter(|e: &&LogEntry| e.kind == kind)
            .count()
    };
    let report = ApplyReport {
        moved: count(OperationKind::Moved),
        trashed: count(OperationKind::Trashed),
        freed_bytes,
        failures,
    };
    let log = OperationLog {
        applied_at,
        entries,
        created_folders: created,
    };
    (report, log)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::scanner::walk::scan_root_with_progress;
    use std::cell::RefCell;

    /// Moves "trashed" files into a private folder instead of the real Trash.
    pub struct FakeTrash {
        pub dir: PathBuf,
        pub calls: RefCell<usize>,
    }

    impl FakeTrash {
        pub fn new(dir: &Path) -> Self {
            fs::create_dir_all(dir).unwrap();
            Self {
                dir: dir.to_path_buf(),
                calls: RefCell::new(0),
            }
        }
    }

    impl Trasher for FakeTrash {
        fn trash(&self, path: &Path) -> Result<PathBuf, String> {
            *self.calls.borrow_mut() += 1;
            let to = unique_destination(&self.dir, &path.file_name().unwrap().to_string_lossy());
            fs::rename(path, &to).map_err(|e| e.to_string())?;
            Ok(to)
        }
    }

    pub fn setup(names: &[&str]) -> (tempfile::TempDir, PathBuf, Vec<FileItem>) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Downloads");
        fs::create_dir(&root).unwrap();
        for name in names {
            fs::write(root.join(name), name.as_bytes()).unwrap();
        }
        let items = scan_root_with_progress(&root, 0, |_, _, _| {})
            .unwrap()
            .items;
        (tmp, root, items)
    }

    pub fn id(items: &[FileItem], name: &str) -> ItemId {
        items.iter().find(|i| i.name == name).unwrap().id
    }

    fn folders(names: &[&str]) -> Vec<Folder> {
        names
            .iter()
            .map(|n| Folder {
                name: n.to_string(),
                description: String::new(),
            })
            .collect()
    }

    fn mv(item_id: ItemId, folder: &str) -> MoveDecision {
        MoveDecision {
            item_id,
            folder: folder.into(),
        }
    }

    #[test]
    fn validates_ids_and_folders() {
        let (_tmp, _root, items) = setup(&["a.pdf"]);
        let plan_folders = folders(&["Docs"]);
        let decide = |moves, trash| Decisions { moves, trash };

        assert!(
            validate_decisions(&decide(vec![mv(99, "Docs")], vec![]), &items, &plan_folders)
                .is_err()
        );
        assert!(
            validate_decisions(
                &decide(vec![mv(0, "Elsewhere")], vec![]),
                &items,
                &plan_folders
            )
            .is_err()
        );
        assert!(
            validate_decisions(&decide(vec![mv(0, "docs")], vec![0]), &items, &plan_folders)
                .is_err()
        );
        let ok = validate_decisions(&decide(vec![mv(0, "docs")], vec![]), &items, &plan_folders)
            .unwrap();
        assert!(matches!(&ok[0], Operation::Move { folder, .. } if folder == "Docs"));
    }

    #[test]
    fn refuses_to_trash_a_destination_folder() {
        let (_tmp, root, _) = setup(&["a.pdf"]);
        fs::create_dir(root.join("Docs")).unwrap();
        let items = scan_root_with_progress(&root, 0, |_, _, _| {})
            .unwrap()
            .items;
        let decisions = Decisions {
            moves: vec![mv(id(&items, "a.pdf"), "Docs")],
            trash: vec![id(&items, "Docs")],
        };

        assert!(validate_decisions(&decisions, &items, &folders(&["Docs"])).is_err());
    }

    #[test]
    fn moves_into_new_folders_and_trashes_without_touching_the_real_trash() {
        let (tmp, root, items) = setup(&["a.pdf", "old.dmg"]);
        let trash = FakeTrash::new(&tmp.path().join("FakeTrash"));
        let ops = validate_decisions(
            &Decisions {
                moves: vec![mv(id(&items, "a.pdf"), "Docs")],
                trash: vec![id(&items, "old.dmg")],
            },
            &items,
            &folders(&["Docs"]),
        )
        .unwrap();

        let (report, log) = execute(&root, &ops, &trash, 42);

        assert!(root.join("Docs/a.pdf").is_file());
        assert!(!root.join("old.dmg").exists());
        assert!(trash.dir.join("old.dmg").is_file());
        assert_eq!(
            (report.moved, report.trashed, report.freed_bytes),
            (1, 1, 7)
        );
        assert!(report.failures.is_empty());
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.created_folders, vec![root.join("Docs")]);
        assert_eq!(log.applied_at, 42);
    }

    #[test]
    fn never_overwrites_an_existing_file_in_the_target_folder() {
        let (tmp, root, items) = setup(&["a.pdf"]);
        fs::create_dir(root.join("Docs")).unwrap();
        fs::write(root.join("Docs/a.pdf"), b"already here").unwrap();
        let trash = FakeTrash::new(&tmp.path().join("FakeTrash"));
        let ops = vec![Operation::Move {
            item: items[0].clone(),
            folder: "Docs".into(),
        }];

        execute(&root, &ops, &trash, 0);

        assert_eq!(fs::read(root.join("Docs/a.pdf")).unwrap(), b"already here");
        assert_eq!(fs::read(root.join("Docs/a (2).pdf")).unwrap(), b"a.pdf");
    }

    #[test]
    fn reports_items_that_disappeared_since_the_scan_and_continues() {
        let (tmp, root, items) = setup(&["gone.pdf", "here.pdf"]);
        fs::remove_file(root.join("gone.pdf")).unwrap();
        let trash = FakeTrash::new(&tmp.path().join("FakeTrash"));
        let ops: Vec<Operation> = items
            .iter()
            .map(|item| Operation::Move {
                item: item.clone(),
                folder: "Docs".into(),
            })
            .collect();

        let (report, log) = execute(&root, &ops, &trash, 0);

        assert_eq!(report.moved, 1);
        assert_eq!(report.failures[0].name, "gone.pdf");
        assert_eq!(log.entries.len(), 1);
    }

    #[test]
    fn refuses_a_file_or_symlink_in_place_of_the_target_folder() {
        let (tmp, root, items) = setup(&["a.pdf", "Docs"]);
        let trash = FakeTrash::new(&tmp.path().join("FakeTrash"));
        let ops = vec![Operation::Move {
            item: items.iter().find(|i| i.name == "a.pdf").unwrap().clone(),
            folder: "Docs".into(),
        }];

        let (report, _) = execute(&root, &ops, &trash, 0);

        assert_eq!(report.failures.len(), 1);
        assert!(root.join("a.pdf").is_file());
    }

    #[test]
    fn unique_destination_handles_names_without_extensions() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("README"), b"").unwrap();
        fs::write(tmp.path().join("README (2)"), b"").unwrap();

        assert_eq!(
            unique_destination(tmp.path(), "README"),
            tmp.path().join("README (3)")
        );
        assert_eq!(
            unique_destination(tmp.path(), "new.txt"),
            tmp.path().join("new.txt")
        );
    }
}
