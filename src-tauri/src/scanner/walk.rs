use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::category::categorize;
use super::sensitive::is_sensitive;
use super::types::{FileItem, ItemId, Scan};

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("cannot read folder {path}: {source}")]
    ReadRoot {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a folder")]
    NotADirectory { path: String },
}

/// Scan the top-level entries of `root`. Hidden entries and symlinks are
/// skipped; entries that can't be read are listed in `Scan::skipped`.
/// `on_progress(done, total, name)` is called just before each entry is
/// examined (large subfolders take the longest).
pub fn scan_root_with_progress(
    root: &Path,
    now: i64,
    on_progress: impl Fn(usize, usize, &str),
) -> Result<Scan, ScanError> {
    let display = root.display().to_string();
    let root_meta = fs::metadata(root).map_err(|source| ScanError::ReadRoot {
        path: display.clone(),
        source,
    })?;
    if !root_meta.is_dir() {
        return Err(ScanError::NotADirectory { path: display });
    }
    let entries = fs::read_dir(root).map_err(|source| ScanError::ReadRoot {
        path: display,
        source,
    })?;

    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    let total = names.len();

    let (items, skipped) = names.into_iter().enumerate().fold(
        (Vec::new(), Vec::new()),
        |(mut items, mut skipped), (index, name)| {
            on_progress(index, total, &name);
            match describe_entry(&root.join(&name), &name) {
                Ok(Some(partial)) => items.push(FileItem {
                    id: items.len() as ItemId,
                    ..partial
                }),
                Ok(None) => {}
                Err(_) => skipped.push(name),
            }
            (items, skipped)
        },
    );

    Ok(Scan {
        root: root.to_path_buf(),
        scanned_at: now,
        items,
        skipped,
    })
}

/// Build an item (with a placeholder id) or `None` for entries we ignore.
fn describe_entry(path: &Path, name: &str) -> std::io::Result<Option<FileItem>> {
    let meta = fs::symlink_metadata(path)?;
    let file_type = meta.file_type();
    if file_type.is_symlink() || !(file_type.is_file() || file_type.is_dir()) {
        return Ok(None);
    }
    let is_dir = file_type.is_dir();
    let (size_bytes, file_count) = if is_dir {
        dir_totals(path)?
    } else {
        (meta.len(), 1)
    };
    let extension = if is_dir {
        String::new()
    } else {
        Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default()
    };
    Ok(Some(FileItem {
        id: 0,
        name: name.to_string(),
        category: categorize(&extension, is_dir),
        is_dir,
        size_bytes,
        file_count,
        modified_at: meta.mtime(),
        accessed_at: meta.atime(),
        duplicate_group: None,
        sensitive: is_sensitive(name, &extension, is_dir),
        extension,
    }))
}

/// Recursive (size, regular-file count) of a folder, not following symlinks.
/// Iterative so deeply nested folders can't overflow the stack.
fn dir_totals(dir: &Path) -> std::io::Result<(u64, u64)> {
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    let mut totals = (0u64, 0u64);
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                totals = (totals.0 + meta.len(), totals.1 + 1);
            }
        }
    }
    Ok(totals)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_root(root: &Path, now: i64) -> Result<Scan, ScanError> {
        scan_root_with_progress(root, now, |_, _, _| {})
    }
    use crate::scanner::types::Category;

    fn write(path: &Path, bytes: usize) {
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn item<'a>(scan: &'a Scan, name: &str) -> &'a FileItem {
        scan.items.iter().find(|i| i.name == name).unwrap()
    }

    #[test]
    fn lists_top_level_files_with_metadata() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Report.PDF"), 1_000);
        let mtime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        fs::File::options()
            .write(true)
            .open(dir.path().join("Report.PDF"))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(mtime))
            .unwrap();

        let scan = scan_root(dir.path(), 1_800_000_000).unwrap();

        let report = item(&scan, "Report.PDF");
        assert_eq!(report.extension, "pdf");
        assert_eq!(report.category, Category::Document);
        assert_eq!(report.size_bytes, 1_000);
        assert_eq!(report.file_count, 1);
        assert_eq!(report.modified_at, 1_700_000_000);
        assert!(!report.is_dir);
        assert_eq!(scan.scanned_at, 1_800_000_000);
    }

    #[test]
    fn treats_subfolder_as_one_item_with_recursive_totals() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        fs::create_dir_all(project.join("nested")).unwrap();
        write(&project.join("a.txt"), 100);
        write(&project.join("nested/b.txt"), 250);

        let scan = scan_root(dir.path(), 0).unwrap();

        assert_eq!(scan.items.len(), 1);
        let folder = item(&scan, "project");
        assert!(folder.is_dir);
        assert_eq!(folder.category, Category::Folder);
        assert_eq!(folder.size_bytes, 350);
        assert_eq!(folder.file_count, 2);
        assert_eq!(folder.extension, "");
    }

    #[test]
    fn skips_hidden_files_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join(".DS_Store"), 10);
        write(&dir.path().join("real.zip"), 10);
        std::os::unix::fs::symlink(dir.path().join("real.zip"), dir.path().join("link.zip"))
            .unwrap();

        let scan = scan_root(dir.path(), 0).unwrap();

        let names: Vec<&str> = scan.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["real.zip"]);
    }

    #[test]
    fn assigns_unique_ids_in_name_order() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("b.png"), 1);
        write(&dir.path().join("a.png"), 1);

        let scan = scan_root(dir.path(), 0).unwrap();

        let ids: Vec<(u32, &str)> = scan.items.iter().map(|i| (i.id, i.name.as_str())).collect();
        assert_eq!(ids, vec![(0, "a.png"), (1, "b.png")]);
    }

    #[test]
    fn reports_each_entry_before_describing_it() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("b.png"), 1);
        write(&dir.path().join("a.png"), 1);
        write(&dir.path().join(".hidden"), 1);
        let seen = std::cell::RefCell::new(Vec::new());

        scan_root_with_progress(dir.path(), 0, |done, total, name| {
            seen.borrow_mut().push((done, total, name.to_string()));
        })
        .unwrap();

        assert_eq!(
            seen.into_inner(),
            vec![(0, 2, "a.png".to_string()), (1, 2, "b.png".to_string())]
        );
    }

    #[test]
    fn rejects_missing_root_and_files() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("f.txt"), 1);

        assert!(matches!(
            scan_root(&dir.path().join("missing"), 0),
            Err(ScanError::ReadRoot { .. })
        ));
        assert!(matches!(
            scan_root(&dir.path().join("f.txt"), 0),
            Err(ScanError::NotADirectory { .. })
        ));
    }
}
