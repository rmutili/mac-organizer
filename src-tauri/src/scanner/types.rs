use serde::Serialize;

/// Stable identifier for an item within one scan. The UI and Claude refer to
/// items by this ID, never by path; the Rust side resolves IDs back to paths.
pub type ItemId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Folder,
    Image,
    Video,
    Audio,
    Document,
    Spreadsheet,
    Presentation,
    Archive,
    Installer,
    Code,
    Other,
}

impl Category {
    pub const ALL: [Category; 11] = [
        Category::Folder,
        Category::Image,
        Category::Video,
        Category::Audio,
        Category::Document,
        Category::Spreadsheet,
        Category::Presentation,
        Category::Archive,
        Category::Installer,
        Category::Code,
        Category::Other,
    ];
}

/// One top-level entry of the scanned root. A folder counts as a single item
/// (it is moved as a unit) with its size and file count summed recursively.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileItem {
    pub id: ItemId,
    /// File or folder name, relative to the scanned root (no separators).
    pub name: String,
    /// Lowercased extension without the dot; empty for folders and extensionless files.
    pub extension: String,
    pub category: Category,
    pub is_dir: bool,
    pub size_bytes: u64,
    /// 1 for a file; recursive regular-file count for a folder.
    pub file_count: u64,
    /// Seconds since the Unix epoch.
    pub modified_at: i64,
    /// Seconds since the Unix epoch; approximates "last opened".
    pub accessed_at: i64,
    /// Items sharing a group number are byte-identical files (verified by hash).
    pub duplicate_group: Option<u32>,
    /// Looks like a credential or key file; never auto-suggested for deletion.
    pub sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scan {
    pub root: std::path::PathBuf,
    pub scanned_at: i64,
    pub items: Vec<FileItem>,
    /// Names of top-level entries that could not be read (e.g. permission denied).
    pub skipped: Vec<String>,
}
