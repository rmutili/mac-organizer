//! Finds byte-identical top-level files. Candidates are grouped by size first,
//! so only same-size files are ever hashed.
//!
//! A file that can't be read is treated as unique: failing to verify a
//! duplicate is the safe direction, since duplicates feed deletion advice.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::types::{FileItem, ItemId};

const HASH_BUFFER: usize = 1 << 16;

/// Groups of identical files (each sorted by id, groups sorted by first id).
/// `on_progress(done, total, name)` fires before each file is hashed.
pub fn find_duplicate_groups(
    root: &Path,
    items: &[FileItem],
    on_progress: impl Fn(usize, usize, &str),
) -> Vec<Vec<ItemId>> {
    let by_size = items.iter().filter(|i| !i.is_dir && i.size_bytes > 0).fold(
        HashMap::<u64, Vec<&FileItem>>::new(),
        |mut acc, item| {
            acc.entry(item.size_bytes).or_default().push(item);
            acc
        },
    );
    let mut candidates: Vec<&FileItem> = by_size
        .into_values()
        .filter(|same_size| same_size.len() > 1)
        .flatten()
        .collect();
    candidates.sort_by_key(|i| i.id);

    let total = candidates.len();
    let by_hash = candidates.iter().enumerate().fold(
        HashMap::<(u64, [u8; 32]), Vec<ItemId>>::new(),
        |mut acc, (done, item)| {
            on_progress(done, total, &item.name);
            if let Ok(digest) = hash_file(&root.join(&item.name)) {
                acc.entry((item.size_bytes, digest))
                    .or_default()
                    .push(item.id);
            }
            acc
        },
    );

    let mut groups: Vec<Vec<ItemId>> = by_hash
        .into_values()
        .filter(|ids| ids.len() > 1)
        .map(|mut ids| {
            ids.sort();
            ids
        })
        .collect();
    groups.sort();
    groups
}

fn hash_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

/// New items with `duplicate_group` set from `groups` (group n -> Some(n)).
pub fn with_duplicate_groups(items: &[FileItem], groups: &[Vec<ItemId>]) -> Vec<FileItem> {
    items
        .iter()
        .map(|item| FileItem {
            duplicate_group: groups
                .iter()
                .position(|g| g.contains(&item.id))
                .map(|n| n as u32),
            ..item.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::walk::scan_root_with_progress;
    use std::fs;

    fn scan(dir: &Path) -> Vec<FileItem> {
        scan_root_with_progress(dir, 0, |_, _, _| {}).unwrap().items
    }

    fn ids_by_name(items: &[FileItem], names: &[&str]) -> Vec<ItemId> {
        names
            .iter()
            .map(|n| items.iter().find(|i| i.name == *n).unwrap().id)
            .collect()
    }

    #[test]
    fn groups_identical_files_and_ignores_same_size_different_content() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.pdf"), b"same bytes").unwrap();
        fs::write(dir.path().join("a (1).pdf"), b"same bytes").unwrap();
        fs::write(dir.path().join("b.pdf"), b"diff bytes").unwrap();
        fs::write(dir.path().join("c.txt"), b"unique").unwrap();
        let items = scan(dir.path());

        let groups = find_duplicate_groups(dir.path(), &items, |_, _, _| {});

        let mut expected = ids_by_name(&items, &["a.pdf", "a (1).pdf"]);
        expected.sort();
        assert_eq!(groups, vec![expected]);
    }

    #[test]
    fn skips_folders_and_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("x")).unwrap();
        fs::create_dir(dir.path().join("y")).unwrap();
        fs::write(dir.path().join("empty1"), b"").unwrap();
        fs::write(dir.path().join("empty2"), b"").unwrap();
        let items = scan(dir.path());

        assert!(find_duplicate_groups(dir.path(), &items, |_, _, _| {}).is_empty());
    }

    #[test]
    fn only_hashes_same_size_candidates() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), b"12").unwrap();
        fs::write(dir.path().join("b"), b"12").unwrap();
        fs::write(dir.path().join("c"), b"123").unwrap();
        let items = scan(dir.path());
        let hashed = std::cell::RefCell::new(Vec::new());

        find_duplicate_groups(dir.path(), &items, |_, total, name| {
            hashed.borrow_mut().push((total, name.to_string()));
        });

        assert_eq!(
            hashed.into_inner(),
            vec![(2, "a".to_string()), (2, "b".to_string())]
        );
    }

    #[test]
    fn marks_items_with_their_group_number() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), b"x").unwrap();
        fs::write(dir.path().join("b"), b"x").unwrap();
        fs::write(dir.path().join("c"), b"yy").unwrap();
        let items = scan(dir.path());

        let marked = with_duplicate_groups(&items, &[ids_by_name(&items, &["a", "b"])]);

        let group = |n: &str| marked.iter().find(|i| i.name == n).unwrap().duplicate_group;
        assert_eq!(
            (group("a"), group("b"), group("c")),
            (Some(0), Some(0), None)
        );
    }
}
