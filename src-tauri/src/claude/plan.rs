//! Plan types and validation. Claude's output is untrusted: every suggestion is
//! checked against the scan before it can reach the review UI.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::scanner::types::{FileItem, ItemId};

/// Exactly what the `propose_plan` tool returns, before validation.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct RawPlan {
    #[serde(default)]
    pub folders: Vec<RawFolder>,
    #[serde(default)]
    pub assignments: Vec<RawAssignment>,
    #[serde(default)]
    pub deletion_candidates: Vec<RawDeletion>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawFolder {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawAssignment {
    pub item_id: i64,
    pub folder: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawDeletion {
    pub item_id: i64,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    #[default]
    Low,
    Medium,
    High,
}

impl RawPlan {
    /// Concatenate plans from several batches; validation dedupes afterwards.
    pub fn merge(self, other: RawPlan) -> RawPlan {
        RawPlan {
            folders: [self.folders, other.folders].concat(),
            assignments: [self.assignments, other.assignments].concat(),
            deletion_candidates: [self.deletion_candidates, other.deletion_candidates].concat(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Folder {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Move {
    pub item_id: ItemId,
    pub folder: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Deletion {
    pub item_id: ItemId,
    pub reason: String,
    pub confidence: Confidence,
}

/// A suggestion we dropped, kept so the UI can show what was ignored and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rejection {
    pub suggestion: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    pub folders: Vec<Folder>,
    pub moves: Vec<Move>,
    pub deletions: Vec<Deletion>,
    pub rejected: Vec<Rejection>,
}

pub const MAX_FOLDER_NAME_LEN: usize = 64;
const MAX_REASON_LEN: usize = 300;

/// Check a folder name is a single, safe path component.
pub fn validate_folder_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("folder name is empty".into());
    }
    if trimmed.chars().count() > MAX_FOLDER_NAME_LEN {
        return Err(format!(
            "folder name is longer than {MAX_FOLDER_NAME_LEN} characters"
        ));
    }
    if trimmed.starts_with('.') {
        return Err("folder name can't start with a dot".into());
    }
    if trimmed
        .chars()
        .any(|c| c == '/' || c == ':' || c.is_control())
    {
        return Err("folder name contains '/', ':' or control characters".into());
    }
    Ok(trimmed.to_string())
}

fn clip(text: &str) -> String {
    text.trim().chars().take(MAX_REASON_LEN).collect()
}

fn reject(rejected: &mut Vec<Rejection>, suggestion: String, reason: impl Into<String>) {
    rejected.push(Rejection {
        suggestion,
        reason: reason.into(),
    });
}

/// Turn Claude's raw output into a plan that only references real scan items
/// and safe folder names. Invalid suggestions are moved to `rejected`.
pub fn validate_plan(raw: RawPlan, items: &[FileItem]) -> Plan {
    let mut rejected = Vec::new();
    let folders = validate_folders(raw.folders, items, &mut rejected);
    let find_item = |id: i64| -> Option<&FileItem> {
        ItemId::try_from(id)
            .ok()
            .and_then(|id| items.iter().find(|i| i.id == id))
    };
    let canonical_folder = |name: &str| -> Option<&Folder> {
        let key = name.trim().to_lowercase();
        folders.iter().find(|f| f.name.to_lowercase() == key)
    };
    let is_target = |item: &FileItem| canonical_folder(&item.name).is_some();

    let mut moved: HashSet<ItemId> = HashSet::new();
    let mut moves = Vec::new();
    for a in raw.assignments {
        let label = format!("move item {} to {:?}", a.item_id, a.folder);
        let Some(item) = find_item(a.item_id) else {
            reject(&mut rejected, label, "no such item in the scan");
            continue;
        };
        let Some(folder) = canonical_folder(&a.folder) else {
            reject(
                &mut rejected,
                label,
                "target folder wasn't proposed or isn't valid",
            );
            continue;
        };
        if is_target(item) {
            reject(&mut rejected, label, "item is itself a target folder");
            continue;
        }
        if !moved.insert(item.id) {
            reject(&mut rejected, label, "item was already assigned");
            continue;
        }
        moves.push(Move {
            item_id: item.id,
            folder: folder.name.clone(),
            reason: clip(&a.reason),
        });
    }

    let mut flagged: HashSet<ItemId> = HashSet::new();
    let mut deletions = Vec::new();
    for d in raw.deletion_candidates {
        let label = format!("delete item {}", d.item_id);
        let Some(item) = find_item(d.item_id) else {
            reject(&mut rejected, label, "no such item in the scan");
            continue;
        };
        if is_target(item) {
            reject(&mut rejected, label, "item is a target folder");
            continue;
        }
        if item.sensitive {
            reject(
                &mut rejected,
                label,
                "looks like a credential file; review it yourself",
            );
            continue;
        }
        if flagged.insert(item.id) {
            deletions.push(Deletion {
                item_id: item.id,
                reason: clip(&d.reason),
                confidence: d.confidence,
            });
        }
    }

    let deletions = keep_one_copy(deletions, items, &mut rejected);
    Plan {
        folders,
        moves,
        deletions,
        rejected,
    }
}

/// True for names like "report (1).pdf" or "report (12)".
fn has_copy_suffix(name: &str) -> bool {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    stem.strip_suffix(')')
        .and_then(|rest| rest.rsplit_once(" ("))
        .is_some_and(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// If every copy in a verified duplicate group is marked for deletion, keep
/// the one with the cleanest name, so a file is never lost entirely.
fn keep_one_copy(
    deletions: Vec<Deletion>,
    items: &[FileItem],
    rejected: &mut Vec<Rejection>,
) -> Vec<Deletion> {
    let flagged: HashSet<ItemId> = deletions.iter().map(|d| d.item_id).collect();
    let mut groups: BTreeMap<u32, Vec<&FileItem>> = BTreeMap::new();
    for item in items {
        if let Some(group) = item.duplicate_group {
            groups.entry(group).or_default().push(item);
        }
    }
    let keepers: HashSet<ItemId> = groups
        .values()
        .filter(|members| members.iter().all(|m| flagged.contains(&m.id)))
        .filter_map(|members| {
            members
                .iter()
                .min_by_key(|m| (has_copy_suffix(&m.name), m.name.len(), m.id))
                .map(|m| m.id)
        })
        .collect();
    for id in &keepers {
        reject(
            rejected,
            format!("delete item {id}"),
            "kept one copy of identical files",
        );
    }
    deletions
        .into_iter()
        .filter(|d| !keepers.contains(&d.item_id))
        .collect()
}

/// Valid, case-insensitively unique folders that don't collide with a file.
fn validate_folders(
    raw: Vec<RawFolder>,
    items: &[FileItem],
    rejected: &mut Vec<Rejection>,
) -> Vec<Folder> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut folders = Vec::new();
    for f in raw {
        let label = format!("folder {:?}", f.name);
        let name = match validate_folder_name(&f.name) {
            Ok(name) => name,
            Err(why) => {
                reject(rejected, label, why);
                continue;
            }
        };
        let key = name.to_lowercase();
        let collides_with_file = items
            .iter()
            .any(|i| !i.is_dir && i.name.to_lowercase() == key);
        if collides_with_file {
            reject(rejected, label, "a file with this name already exists");
            continue;
        }
        if seen.insert(key) {
            folders.push(Folder {
                name,
                description: clip(&f.description),
            });
        }
    }
    folders
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::types::Category;

    fn item(id: ItemId, name: &str, is_dir: bool) -> FileItem {
        FileItem {
            id,
            name: name.into(),
            extension: String::new(),
            category: if is_dir {
                Category::Folder
            } else {
                Category::Other
            },
            is_dir,
            size_bytes: 1,
            file_count: 1,
            modified_at: 0,
            accessed_at: 0,
            duplicate_group: None,
            sensitive: false,
        }
    }

    fn folder(name: &str) -> RawFolder {
        RawFolder {
            name: name.into(),
            description: String::new(),
        }
    }

    fn assign(item_id: i64, folder: &str) -> RawAssignment {
        RawAssignment {
            item_id,
            folder: folder.into(),
            reason: "r".into(),
        }
    }

    fn delete(item_id: i64) -> RawDeletion {
        RawDeletion {
            item_id,
            reason: "old".into(),
            confidence: Confidence::High,
        }
    }

    #[test]
    fn accepts_plain_folder_names_and_trims() {
        assert_eq!(validate_folder_name("  Invoices "), Ok("Invoices".into()));
        assert_eq!(
            validate_folder_name("Tax 2025 (US)"),
            Ok("Tax 2025 (US)".into())
        );
    }

    #[test]
    fn rejects_unsafe_folder_names() {
        for bad in [
            "",
            "   ",
            ".",
            "..",
            ".hidden",
            "a/b",
            "../x",
            "a:b",
            "tab\there",
        ] {
            assert!(
                validate_folder_name(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
        assert!(validate_folder_name(&"x".repeat(MAX_FOLDER_NAME_LEN + 1)).is_err());
    }

    #[test]
    fn keeps_valid_moves_and_deletions() {
        let items = vec![item(0, "a.pdf", false), item(1, "b.dmg", false)];
        let raw = RawPlan {
            folders: vec![folder("Docs")],
            assignments: vec![assign(0, "Docs")],
            deletion_candidates: vec![delete(1)],
        };

        let plan = validate_plan(raw, &items);

        assert_eq!(plan.folders.len(), 1);
        assert_eq!(
            plan.moves,
            vec![Move {
                item_id: 0,
                folder: "Docs".into(),
                reason: "r".into()
            }]
        );
        assert_eq!(plan.deletions.len(), 1);
        assert!(plan.rejected.is_empty());
    }

    #[test]
    fn rejects_unknown_item_ids() {
        let items = vec![item(0, "a.pdf", false)];
        let raw = RawPlan {
            folders: vec![folder("Docs")],
            assignments: vec![assign(7, "Docs"), assign(-1, "Docs")],
            deletion_candidates: vec![delete(99)],
        };

        let plan = validate_plan(raw, &items);

        assert!(plan.moves.is_empty());
        assert!(plan.deletions.is_empty());
        assert_eq!(plan.rejected.len(), 3);
    }

    #[test]
    fn rejects_moves_to_undeclared_or_unsafe_folders() {
        let items = vec![item(0, "a.pdf", false), item(1, "b.pdf", false)];
        let raw = RawPlan {
            folders: vec![folder("../Escape")],
            assignments: vec![assign(0, "../Escape"), assign(1, "Never Declared")],
            deletion_candidates: vec![],
        };

        let plan = validate_plan(raw, &items);

        assert!(plan.folders.is_empty());
        assert!(plan.moves.is_empty());
        assert_eq!(plan.rejected.len(), 3);
    }

    #[test]
    fn dedupes_folders_case_insensitively_and_matches_moves_to_canonical_name() {
        let items = vec![item(0, "a.pdf", false)];
        let raw = RawPlan {
            folders: vec![folder("Docs"), folder("docs")],
            assignments: vec![assign(0, "DOCS")],
            deletion_candidates: vec![],
        };

        let plan = validate_plan(raw, &items);

        assert_eq!(plan.folders.len(), 1);
        assert_eq!(plan.moves[0].folder, "Docs");
    }

    #[test]
    fn first_assignment_wins_for_each_item() {
        let items = vec![item(0, "a.pdf", false)];
        let raw = RawPlan {
            folders: vec![folder("A"), folder("B")],
            assignments: vec![assign(0, "A"), assign(0, "B")],
            deletion_candidates: vec![delete(0), delete(0)],
        };

        let plan = validate_plan(raw, &items);

        assert_eq!(plan.moves.len(), 1);
        assert_eq!(plan.moves[0].folder, "A");
        assert_eq!(plan.deletions.len(), 1);
    }

    #[test]
    fn rejects_folder_that_collides_with_an_existing_file() {
        let items = vec![item(0, "Notes", false), item(1, "x.txt", false)];
        let raw = RawPlan {
            folders: vec![folder("notes")],
            assignments: vec![assign(1, "notes")],
            deletion_candidates: vec![],
        };

        let plan = validate_plan(raw, &items);

        assert!(plan.folders.is_empty());
        assert!(plan.moves.is_empty());
    }

    #[test]
    fn allows_existing_folder_as_target_but_never_moves_it_into_itself() {
        let items = vec![item(0, "Invoices", true), item(1, "inv.pdf", false)];
        let raw = RawPlan {
            folders: vec![folder("Invoices")],
            assignments: vec![assign(0, "Invoices"), assign(1, "Invoices")],
            deletion_candidates: vec![],
        };

        let plan = validate_plan(raw, &items);

        assert_eq!(plan.moves.len(), 1);
        assert_eq!(plan.moves[0].item_id, 1);
        assert_eq!(plan.rejected.len(), 1);
    }

    #[test]
    fn target_folders_are_not_suggested_for_deletion() {
        let items = vec![item(0, "Invoices", true), item(1, "inv.pdf", false)];
        let raw = RawPlan {
            folders: vec![folder("Invoices")],
            assignments: vec![assign(1, "Invoices")],
            deletion_candidates: vec![delete(0)],
        };

        let plan = validate_plan(raw, &items);

        assert!(plan.deletions.is_empty());
    }

    #[test]
    fn truncates_overlong_reasons() {
        let items = vec![item(0, "a.pdf", false)];
        let raw = RawPlan {
            folders: vec![folder("Docs")],
            assignments: vec![RawAssignment {
                item_id: 0,
                folder: "Docs".into(),
                reason: "é".repeat(MAX_REASON_LEN * 2),
            }],
            deletion_candidates: vec![],
        };

        let plan = validate_plan(raw, &items);

        assert_eq!(plan.moves[0].reason.chars().count(), MAX_REASON_LEN);
    }

    fn dup(id: ItemId, name: &str, group: u32) -> FileItem {
        FileItem {
            duplicate_group: Some(group),
            ..item(id, name, false)
        }
    }

    #[test]
    fn keeps_the_cleanest_named_copy_when_every_duplicate_is_flagged() {
        let items = vec![
            dup(0, "INV_2 (1).pdf", 0),
            dup(1, "INV_2 (2).pdf", 0),
            dup(2, "INV_2.pdf", 0),
        ];
        let raw = RawPlan {
            deletion_candidates: vec![delete(0), delete(1), delete(2)],
            ..RawPlan::default()
        };

        let plan = validate_plan(raw, &items);

        let deleted: Vec<ItemId> = plan.deletions.iter().map(|d| d.item_id).collect();
        assert_eq!(deleted, vec![0, 1]);
        assert!(plan.rejected.iter().any(|r| r.reason.contains("one copy")));
    }

    #[test]
    fn keeps_a_copy_even_when_only_suffixed_names_exist() {
        let items = vec![dup(0, "a (1).pdf", 0), dup(1, "a (2).pdf", 0)];
        let raw = RawPlan {
            deletion_candidates: vec![delete(1), delete(0)],
            ..RawPlan::default()
        };

        let plan = validate_plan(raw, &items);

        let deleted: Vec<ItemId> = plan.deletions.iter().map(|d| d.item_id).collect();
        assert_eq!(deleted, vec![1]);
    }

    #[test]
    fn deleting_some_copies_of_a_group_is_allowed() {
        let items = vec![dup(0, "a.pdf", 0), dup(1, "a (1).pdf", 0)];
        let raw = RawPlan {
            deletion_candidates: vec![delete(1)],
            ..RawPlan::default()
        };

        let plan = validate_plan(raw, &items);

        assert_eq!(plan.deletions.len(), 1);
        assert!(plan.rejected.is_empty());
    }

    #[test]
    fn never_suggests_deleting_credential_files() {
        let items = vec![FileItem {
            sensitive: true,
            ..item(0, "client_secret.json", false)
        }];
        let raw = RawPlan {
            deletion_candidates: vec![delete(0)],
            ..RawPlan::default()
        };

        let plan = validate_plan(raw, &items);

        assert!(plan.deletions.is_empty());
        assert!(plan.rejected[0].reason.contains("credential"));
    }
}
