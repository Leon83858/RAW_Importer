//! Destination path planning.
//!
//! Pure functions only: nothing here touches the filesystem, so every rule is
//! unit-testable. Order (identical to 2.x, master prompt §3/§9):
//!
//! destination root → date structure → preserved source subfolders → file-type folder

use std::path::{Component, Path, PathBuf};

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Folder layout. Persisted with stable identifiers — never with UI labels
/// (fixes 2.x bug B1, see DECISIONS.md D-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum Layout {
    #[default]
    YearMonthDay,
    YearMonth,
    Year,
    DayMonthYear,
    Flat,
}

impl Layout {
    /// Parses a 2.x `FolderStructure` value. 2.x stored the *displayed* text,
    /// so German labels occur in real settings files.
    pub fn from_legacy(value: &str) -> Option<Layout> {
        match value.trim() {
            "Year/Month/Day" | "Jahr/Monat/Tag" => Some(Layout::YearMonthDay),
            "Year/Month" | "Jahr/Monat" => Some(Layout::YearMonth),
            "Year" | "Jahr" => Some(Layout::Year),
            "Day-Month-Year" | "Tag-Monat-Jahr" => Some(Layout::DayMonthYear),
            "Flat" | "Flach" => Some(Layout::Flat),
            _ => None,
        }
    }
}

/// How the month folder is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum MonthNaming {
    /// 2.x convention: `11 November` (always English). Default for compatibility.
    #[default]
    EnglishCompat,
    /// `11`
    Numeric,
    /// `11 November` in German (`11 November`, `03 März`, …).
    German,
}

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTHS_DE: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];

pub fn month_folder(month: u32, naming: MonthNaming) -> String {
    assert!((1..=12).contains(&month), "month out of range: {month}");
    let idx = (month - 1) as usize;
    match naming {
        MonthNaming::EnglishCompat => format!("{month:02} {}", MONTHS_EN[idx]),
        MonthNaming::German => format!("{month:02} {}", MONTHS_DE[idx]),
        MonthNaming::Numeric => format!("{month:02}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct StructureOptions {
    pub layout: Layout,
    pub month_naming: MonthNaming,
    pub preserve_subfolders: bool,
    pub type_folders: bool,
}

/// Relative date part of the destination directory.
pub fn date_segments(date: NaiveDate, layout: Layout, naming: MonthNaming) -> PathBuf {
    let year = date.year().to_string();
    let month = month_folder(date.month(), naming);
    let day = format!("{:02}", date.day());
    match layout {
        Layout::YearMonthDay => [year, month, day].iter().collect(),
        Layout::YearMonth => [year, month].iter().collect(),
        Layout::Year => PathBuf::from(year),
        Layout::DayMonthYear => PathBuf::from(format!(
            "{:02}-{:02}-{}",
            date.day(),
            date.month(),
            date.year()
        )),
        Layout::Flat => PathBuf::new(),
    }
}

/// Validates a relative source sub-path: only normal components are allowed.
///
/// 2.x silently dropped unsafe paths; 3.0 reports them (fixes part of B-list,
/// AC-02).
pub fn safe_relative(rel: &Path) -> Result<PathBuf> {
    let mut out = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::UnsafeRelativePath(rel.to_path_buf()))
            }
        }
    }
    Ok(out)
}

/// File-type folder: extension in upper case without dot (`ARW`), as in 2.x.
pub fn type_folder(file_name: &Path) -> Option<String> {
    file_name
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty())
        .map(|e| e.to_uppercase())
}

/// Plans the destination directory *relative to the destination root*.
///
/// * `date` – the date chosen by the date-source chain.
/// * `source_rel_dir` – directory of the file relative to the source root.
/// * `file_name` – used for the type folder.
pub fn plan_relative_dir(
    date: NaiveDate,
    source_rel_dir: &Path,
    file_name: &Path,
    opts: &StructureOptions,
) -> Result<PathBuf> {
    let mut dir = date_segments(date, opts.layout, opts.month_naming);
    if opts.preserve_subfolders {
        dir.push(safe_relative(source_rel_dir)?);
    }
    if opts.type_folders {
        if let Some(t) = type_folder(file_name) {
            dir.push(t);
        }
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn p(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    #[test]
    fn layouts_match_2x_output() {
        let date = d(2026, 11, 8);
        let n = MonthNaming::EnglishCompat;
        assert_eq!(
            date_segments(date, Layout::YearMonthDay, n),
            p(&["2026", "11 November", "08"])
        );
        assert_eq!(
            date_segments(date, Layout::YearMonth, n),
            p(&["2026", "11 November"])
        );
        assert_eq!(date_segments(date, Layout::Year, n), p(&["2026"]));
        assert_eq!(
            date_segments(date, Layout::DayMonthYear, n),
            p(&["08-11-2026"])
        );
        assert_eq!(date_segments(date, Layout::Flat, n), PathBuf::new());
    }

    #[test]
    fn month_naming_variants() {
        assert_eq!(month_folder(3, MonthNaming::EnglishCompat), "03 March");
        assert_eq!(month_folder(3, MonthNaming::German), "03 März");
        assert_eq!(month_folder(3, MonthNaming::Numeric), "03");
    }

    #[test]
    fn legacy_layout_strings_in_both_languages() {
        // Regression for B1: German UI stored German labels.
        assert_eq!(
            Layout::from_legacy("Jahr/Monat/Tag"),
            Some(Layout::YearMonthDay)
        );
        assert_eq!(
            Layout::from_legacy("Year/Month/Day"),
            Some(Layout::YearMonthDay)
        );
        assert_eq!(
            Layout::from_legacy("Tag-Monat-Jahr"),
            Some(Layout::DayMonthYear)
        );
        assert_eq!(Layout::from_legacy("Flach"), Some(Layout::Flat));
        assert_eq!(Layout::from_legacy("nonsense"), None);
    }

    // The following cases port the former xUnit PathGenerationTests of 2.x.

    #[test]
    fn both_options_append_subfolders_then_type() {
        let opts = StructureOptions {
            layout: Layout::YearMonthDay,
            preserve_subfolders: true,
            type_folders: true,
            ..Default::default()
        };
        let got = plan_relative_dir(
            d(2024, 1, 15),
            Path::new("Trip/Day1"),
            Path::new("a.arw"),
            &opts,
        )
        .unwrap();
        assert_eq!(got, p(&["2024", "01 January", "15", "Trip", "Day1", "ARW"]));
    }

    #[test]
    fn only_preserve() {
        let opts = StructureOptions {
            layout: Layout::Year,
            preserve_subfolders: true,
            ..Default::default()
        };
        let got =
            plan_relative_dir(d(2024, 1, 15), Path::new("Sub"), Path::new("a.jpg"), &opts).unwrap();
        assert_eq!(got, p(&["2024", "Sub"]));
    }

    #[test]
    fn only_type_folder() {
        let opts = StructureOptions {
            layout: Layout::Flat,
            type_folders: true,
            ..Default::default()
        };
        let got =
            plan_relative_dir(d(2024, 1, 15), Path::new("Sub"), Path::new("a.jpg"), &opts).unwrap();
        assert_eq!(got, p(&["JPG"]));
    }

    #[test]
    fn neither_option() {
        let opts = StructureOptions::default();
        let got =
            plan_relative_dir(d(2024, 12, 1), Path::new("Sub"), Path::new("a.jpg"), &opts).unwrap();
        assert_eq!(got, p(&["2024", "12 December", "01"]));
    }

    #[test]
    fn file_without_extension_gets_no_type_folder() {
        let opts = StructureOptions {
            layout: Layout::Flat,
            type_folders: true,
            ..Default::default()
        };
        let got =
            plan_relative_dir(d(2024, 1, 1), Path::new(""), Path::new("README"), &opts).unwrap();
        assert_eq!(got, PathBuf::new());
    }

    #[test]
    fn source_root_files_add_no_subfolder() {
        let opts = StructureOptions {
            layout: Layout::Year,
            preserve_subfolders: true,
            ..Default::default()
        };
        let got =
            plan_relative_dir(d(2024, 1, 1), Path::new(""), Path::new("a.arw"), &opts).unwrap();
        assert_eq!(got, p(&["2024"]));
    }

    #[test]
    fn traversal_is_rejected_not_dropped() {
        let opts = StructureOptions {
            preserve_subfolders: true,
            ..Default::default()
        };
        for bad in ["../x", "a/../../b", "/abs"] {
            let r = plan_relative_dir(d(2024, 1, 1), Path::new(bad), Path::new("a.arw"), &opts);
            assert!(matches!(r, Err(CoreError::UnsafeRelativePath(_))), "{bad}");
        }
    }
}
