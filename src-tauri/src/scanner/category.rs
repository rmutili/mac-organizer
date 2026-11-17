use super::types::Category;

/// Map a lowercased extension (no dot) to a category.
pub fn categorize(extension: &str, is_dir: bool) -> Category {
    if is_dir {
        return Category::Folder;
    }
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "heic" | "webp" | "svg" | "tiff" | "bmp" | "raw" => {
            Category::Image
        }
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" => Category::Video,
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" => Category::Audio,
        "pdf" | "doc" | "docx" | "txt" | "rtf" | "md" | "pages" | "odt" | "epub" => {
            Category::Document
        }
        "xls" | "xlsx" | "csv" | "tsv" | "numbers" | "ods" => Category::Spreadsheet,
        "ppt" | "pptx" | "key" | "odp" => Category::Presentation,
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" => Category::Archive,
        "dmg" | "pkg" | "mpkg" | "iso" | "app" => Category::Installer,
        "js" | "ts" | "py" | "rs" | "go" | "java" | "json" | "yaml" | "yml" | "html" | "css"
        | "sh" | "sql" | "ipynb" => Category::Code,
        _ => Category::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_are_folders_regardless_of_name() {
        assert_eq!(categorize("zip", true), Category::Folder);
    }

    #[test]
    fn maps_common_download_types() {
        assert_eq!(categorize("dmg", false), Category::Installer);
        assert_eq!(categorize("pdf", false), Category::Document);
        assert_eq!(categorize("heic", false), Category::Image);
        assert_eq!(categorize("zip", false), Category::Archive);
        assert_eq!(categorize("xlsx", false), Category::Spreadsheet);
    }

    #[test]
    fn unknown_and_empty_extensions_are_other() {
        assert_eq!(categorize("", false), Category::Other);
        assert_eq!(categorize("weird", false), Category::Other);
    }
}
