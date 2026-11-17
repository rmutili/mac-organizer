//! Moving items to the macOS Trash. Behind a trait so tests never touch the
//! real Trash.

use std::path::{Path, PathBuf};

pub trait Trasher {
    /// Move `path` to the Trash and return where it ended up (for undo).
    fn trash(&self, path: &Path) -> Result<PathBuf, String>;
}

/// The real Trash, via `NSFileManager.trashItem(at:resultingItemURL:)`. Unlike
/// Finder scripting, this needs no Automation permission and reports the
/// item's new location, so Undo can put it back.
pub struct MacTrash;

impl Trasher for MacTrash {
    fn trash(&self, path: &Path) -> Result<PathBuf, String> {
        use objc2::rc::Retained;
        use objc2_foundation::{NSFileManager, NSString, NSURL};

        let path_str = path
            .to_str()
            .ok_or_else(|| format!("{} has a name macOS can't trash", path.display()))?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path_str));
        let manager = NSFileManager::defaultManager();
        let mut resulting: Option<Retained<NSURL>> = None;
        manager
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
            .map_err(|e| e.localizedDescription().to_string())?;
        resulting
            .and_then(|u| u.path())
            .map(|p| PathBuf::from(p.to_string()))
            .ok_or_else(|| "macOS didn't report where the item went in the Trash".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses the real Trash with a throwaway file, then puts it back.
    /// Run with: cargo test --lib live_trash -- --ignored
    #[test]
    #[ignore]
    fn live_trash_reports_location_and_can_be_restored() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("mac-organizer-trash-test.txt");
        std::fs::write(&file, "test").unwrap();

        let trashed = MacTrash.trash(&file).unwrap();

        assert!(!file.exists());
        assert!(trashed.is_file(), "trashed to {}", trashed.display());
        std::fs::rename(&trashed, &file).unwrap();
        assert!(file.is_file());
    }
}
