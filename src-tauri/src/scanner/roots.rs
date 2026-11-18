//! Which folders the app may organize. Apply moves every top-level item of the
//! scanned folder, so organizing the wrong place (the home folder, a system
//! folder, a code project) would break things. This is the gate for every
//! folder, including ones picked in the folder dialog.

use std::path::{Component, Path, PathBuf};

/// Folders under the home folder that belong to macOS or apps.
const HOME_PROTECTED: &[&str] = &["Library", "Applications"];

/// Marker files or folders inside a root that mean it's a code project, where
/// moving the top-level items breaks it.
const PROJECT_MARKERS: &[&str] = &[".git", ".hg", ".svn", "package.json", "Cargo.toml"];

/// macOS packages: folders that Finder shows as one file (apps, photo
/// libraries). Their insides must stay as they are.
const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "photoslibrary",
    "musiclibrary",
    "tvlibrary",
    "bundle",
    "framework",
    "pkg",
    "fcpbundle",
    "logicx",
    "band",
    "xcodeproj",
    "xcworkspace",
];

/// Check `candidate` and return its canonical form, or a message saying why
/// it can't be organized. `home` is the person's home folder.
pub fn check_root(candidate: &Path, home: &Path) -> Result<PathBuf, String> {
    // Resolve symlinks first, so a link can't smuggle in a protected folder.
    let root = candidate
        .canonicalize()
        .map_err(|_| "That folder doesn't exist or can't be opened.".to_string())?;
    if !root.is_dir() {
        return Err("That's a file, not a folder.".into());
    }
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());

    let inside = if let Ok(rest) = root.strip_prefix(&home) {
        let first = rest.components().next().ok_or(
            "Your home folder holds Desktop, Documents and Library, so it can't be organized. \
             Choose a folder inside it.",
        )?;
        if HOME_PROTECTED
            .iter()
            .any(|p| first == Component::Normal(p.as_ref()))
        {
            return Err("That folder belongs to macOS and apps, so it can't be organized.".into());
        }
        rest
    } else if let Ok(rest) = root.strip_prefix("/Volumes") {
        // An external drive: /Volumes/<drive> or a folder on it.
        if rest.components().next().is_none() {
            return Err("Choose a drive or a folder on it, not /Volumes itself.".into());
        }
        rest
    } else {
        return Err(
            "Choose a folder in your home folder or on an external drive. System folders can't be \
             organized."
                .into(),
        );
    };

    if inside
        .components()
        .any(|c| matches!(c, Component::Normal(n) if n.to_string_lossy().starts_with('.')))
    {
        return Err("Hidden folders can't be organized.".into());
    }
    if inside.components().any(|c| {
        Path::new(c.as_os_str())
            .extension()
            .is_some_and(|e| PACKAGE_EXTENSIONS.iter().any(|p| e.eq_ignore_ascii_case(p)))
    }) {
        return Err(
            "That's inside an app or library package, which must stay as it is.".to_string(),
        );
    }
    if let Some(marker) = PROJECT_MARKERS.iter().find(|m| root.join(m).exists()) {
        return Err(format!(
            "That folder looks like a code project (it has {marker}). Moving its files would \
             break it, so it can't be organized."
        ));
    }
    Ok(root)
}

/// What the UI shows for a root: its last path component.
pub fn display_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture {
        _tmp: tempfile::TempDir,
        home: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        // Canonical, because macOS temp dirs live behind the /var -> /private/var symlink.
        let home = tmp.path().canonicalize().unwrap().join("home");
        for dir in [
            "Downloads",
            "Documents/Taxes",
            "Library/Caches",
            ".config",
            "Code/app/.git",
        ] {
            fs::create_dir_all(home.join(dir)).unwrap();
        }
        fs::write(home.join("notes.txt"), "").unwrap();
        Fixture { _tmp: tmp, home }
    }

    #[test]
    fn accepts_ordinary_folders_inside_home() {
        let f = fixture();
        for dir in ["Downloads", "Documents", "Documents/Taxes"] {
            assert_eq!(check_root(&f.home.join(dir), &f.home), Ok(f.home.join(dir)));
        }
    }

    #[test]
    fn refuses_the_home_folder_and_anything_above_it() {
        let f = fixture();
        assert!(check_root(&f.home, &f.home).is_err());
        assert!(check_root(f.home.parent().unwrap(), &f.home).is_err());
        assert!(check_root(Path::new("/"), &f.home).is_err());
    }

    #[test]
    fn refuses_library_and_hidden_folders() {
        let f = fixture();
        assert!(check_root(&f.home.join("Library/Caches"), &f.home).is_err());
        assert!(check_root(&f.home.join(".config"), &f.home).is_err());
    }

    #[test]
    fn refuses_system_folders_outside_home() {
        let f = fixture();
        for dir in ["/System", "/Applications", "/usr", "/private/etc"] {
            assert!(check_root(Path::new(dir), &f.home).is_err(), "{dir}");
        }
    }

    #[test]
    fn refuses_code_projects() {
        let f = fixture();
        let err = check_root(&f.home.join("Code/app"), &f.home).unwrap_err();
        assert!(err.contains("project"), "{err}");
    }

    #[test]
    fn refuses_app_and_library_packages() {
        let f = fixture();
        let library = f
            .home
            .join("Pictures/Photos Library.photoslibrary/originals");
        fs::create_dir_all(&library).unwrap();
        assert!(check_root(library.parent().unwrap(), &f.home).is_err());
        assert!(check_root(&library, &f.home).is_err());
    }

    #[test]
    fn refuses_files_and_missing_folders() {
        let f = fixture();
        assert!(check_root(&f.home.join("notes.txt"), &f.home).is_err());
        assert!(check_root(&f.home.join("Nope"), &f.home).is_err());
    }

    #[test]
    fn resolves_symlinks_before_checking() {
        let f = fixture();
        let link = f.home.join("Downloads/sneaky");
        std::os::unix::fs::symlink(&f.home, &link).unwrap();
        assert!(check_root(&link, &f.home).is_err());
    }

    #[test]
    fn display_name_is_the_last_component() {
        assert_eq!(display_name(Path::new("/Users/x/Downloads")), "Downloads");
    }
}
