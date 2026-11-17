use serde::Serialize;

use super::types::{Category, FileItem};

/// Age buckets by last-modified time, youngest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgeBucket {
    Week,
    Month,
    Quarter,
    Year,
    Older,
}

impl AgeBucket {
    pub const ALL: [AgeBucket; 5] = [
        AgeBucket::Week,
        AgeBucket::Month,
        AgeBucket::Quarter,
        AgeBucket::Year,
        AgeBucket::Older,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeatmapCell {
    pub category: Category,
    pub age: AgeBucket,
    pub size_bytes: u64,
    pub file_count: u64,
}

/// Category x age grid. Always contains every combination (zeros included) so
/// the UI can render a fixed grid without filling gaps.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Heatmap {
    pub cells: Vec<HeatmapCell>,
    pub total_size_bytes: u64,
    pub total_file_count: u64,
}

const DAY: i64 = 86_400;

pub fn age_bucket(modified_at: i64, now: i64) -> AgeBucket {
    let age_days = (now - modified_at).max(0) / DAY;
    match age_days {
        0..7 => AgeBucket::Week,
        7..30 => AgeBucket::Month,
        30..90 => AgeBucket::Quarter,
        90..365 => AgeBucket::Year,
        _ => AgeBucket::Older,
    }
}

pub fn build_heatmap(items: &[FileItem], now: i64) -> Heatmap {
    let cells: Vec<HeatmapCell> = Category::ALL
        .iter()
        .flat_map(|&category| {
            AgeBucket::ALL.iter().map(move |&age| {
                let matching = items
                    .iter()
                    .filter(|i| i.category == category && age_bucket(i.modified_at, now) == age);
                let (size_bytes, file_count) =
                    matching.fold((0, 0), |(s, c), i| (s + i.size_bytes, c + i.file_count));
                HeatmapCell {
                    category,
                    age,
                    size_bytes,
                    file_count,
                }
            })
        })
        .collect();

    Heatmap {
        total_size_bytes: cells.iter().map(|c| c.size_bytes).sum(),
        total_file_count: cells.iter().map(|c| c.file_count).sum(),
        cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn file(category: Category, size: u64, count: u64, age_days: i64) -> FileItem {
        FileItem {
            id: 0,
            name: "x".into(),
            extension: String::new(),
            category,
            is_dir: category == Category::Folder,
            size_bytes: size,
            file_count: count,
            modified_at: NOW - age_days * DAY,
            accessed_at: NOW,
            duplicate_group: None,
            sensitive: false,
        }
    }

    fn cell(map: &Heatmap, category: Category, age: AgeBucket) -> &HeatmapCell {
        map.cells
            .iter()
            .find(|c| c.category == category && c.age == age)
            .unwrap()
    }

    #[test]
    fn buckets_by_age_boundaries() {
        assert_eq!(age_bucket(NOW, NOW), AgeBucket::Week);
        assert_eq!(age_bucket(NOW - 6 * DAY, NOW), AgeBucket::Week);
        assert_eq!(age_bucket(NOW - 7 * DAY, NOW), AgeBucket::Month);
        assert_eq!(age_bucket(NOW - 30 * DAY, NOW), AgeBucket::Quarter);
        assert_eq!(age_bucket(NOW - 90 * DAY, NOW), AgeBucket::Year);
        assert_eq!(age_bucket(NOW - 365 * DAY, NOW), AgeBucket::Older);
    }

    #[test]
    fn future_timestamps_count_as_newest() {
        assert_eq!(age_bucket(NOW + DAY, NOW), AgeBucket::Week);
    }

    #[test]
    fn sums_size_and_count_per_cell() {
        let items = vec![
            file(Category::Installer, 500, 1, 400),
            file(Category::Installer, 300, 1, 500),
            file(Category::Folder, 1_000, 12, 2),
        ];

        let map = build_heatmap(&items, NOW);

        let old_installers = cell(&map, Category::Installer, AgeBucket::Older);
        assert_eq!(
            (old_installers.size_bytes, old_installers.file_count),
            (800, 2)
        );
        let new_folders = cell(&map, Category::Folder, AgeBucket::Week);
        assert_eq!(
            (new_folders.size_bytes, new_folders.file_count),
            (1_000, 12)
        );
        assert_eq!(map.total_size_bytes, 1_800);
        assert_eq!(map.total_file_count, 14);
    }

    #[test]
    fn includes_every_cell_even_when_empty() {
        let map = build_heatmap(&[], NOW);

        assert_eq!(map.cells.len(), Category::ALL.len() * AgeBucket::ALL.len());
        assert!(
            map.cells
                .iter()
                .all(|c| c.size_bytes == 0 && c.file_count == 0)
        );
    }
}
