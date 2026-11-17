//! Previews through macOS Quick Look. The webview never reads files: it asks
//! for a thumbnail by item ID and gets back a PNG data URL.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const QLMANAGE: &str = "/usr/bin/qlmanage";
const THUMBNAIL_TIMEOUT: Duration = Duration::from_secs(8);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Where Quick Look writes the thumbnail for `source` inside `out_dir`.
pub fn thumbnail_output_path(out_dir: &Path, source: &Path) -> PathBuf {
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    out_dir.join(format!("{name}.png"))
}

pub fn png_data_url(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
}

/// Render a thumbnail (cached in `cache_dir`). `Ok(None)` when Quick Look has
/// no preview for this kind of file.
pub fn thumbnail(source: &Path, cache_dir: &Path, size: u32) -> Result<Option<String>, String> {
    fs::create_dir_all(cache_dir).map_err(|e| format!("couldn't create the preview cache: {e}"))?;
    let output = thumbnail_output_path(cache_dir, source);
    if !output.is_file() {
        let mut child = Command::new(QLMANAGE)
            .args(["-t", "-s", &size.to_string(), "-o"])
            .arg(cache_dir)
            .arg(source)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("couldn't start Quick Look: {e}"))?;
        let started = Instant::now();
        while child.try_wait().map_err(|e| e.to_string())?.is_none() {
            if started.elapsed() > THUMBNAIL_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
    match fs::read(&output) {
        Ok(bytes) => Ok(Some(png_data_url(&bytes))),
        Err(_) => Ok(None),
    }
}

/// Open the full Quick Look panel, like pressing Space in Finder.
pub fn quick_look(path: &Path) -> Result<(), String> {
    Command::new(QLMANAGE)
        .arg("-p")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't open Quick Look: {e}"))
}

/// Select the item in a Finder window.
pub fn reveal_in_finder(path: &Path) -> Result<(), String> {
    Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|e| format!("couldn't open Finder: {e}"))
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err("Finder couldn't show that item.".into())
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_path_appends_png_to_the_full_name() {
        assert_eq!(
            thumbnail_output_path(Path::new("/cache"), Path::new("/d/report.pdf")),
            PathBuf::from("/cache/report.pdf.png")
        );
    }

    #[test]
    fn encodes_png_as_data_url() {
        assert_eq!(png_data_url(b"abc"), "data:image/png;base64,YWJj");
    }

    #[test]
    fn renders_a_real_thumbnail_for_a_text_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("note.txt");
        fs::write(&file, "hello").unwrap();

        let url = thumbnail(&file, &tmp.path().join("cache"), 128).unwrap();

        assert!(url.is_some_and(|u| u.starts_with("data:image/png;base64,")));
    }
}
