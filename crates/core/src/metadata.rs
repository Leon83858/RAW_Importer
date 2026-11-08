//! Capture-date resolution (master prompt §9/§10).
//!
//! 2.x always used the filesystem modification time. 3.0 adds EXIF dates as an
//! explicit, configurable option with a visible fallback chain:
//!
//! * the policy lists date sources in priority order;
//! * the first source that yields a valid date wins; if it is not the first
//!   entry the result is flagged `fell_back`;
//! * if no listed source works, the filesystem modification time is used and
//!   flagged as a fallback; if even that is unavailable there is **no** date
//!   and the caller must report the file (no arbitrary date is invented).
//!
//! EXIF timestamps carry no time zone. They are kept as the camera's local
//! wall-clock time; no conversion is applied.
//!
//! Native reading uses `kamadak-exif`. It parses JPEG, TIFF-based containers
//! (ARW, CR2, NEF, DNG, ORF, …) and HEIF. Which RAW containers are readable is
//! **only claimed for what is tested** (see TESTING.md); files it cannot read
//! simply fall back and are reported as such.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum DateSource {
    ExifOriginal,
    ExifDigitized,
    FilesystemCreated,
    FilesystemModified,
}

impl DateSource {
    fn is_exif(self) -> bool {
        matches!(self, DateSource::ExifOriginal | DateSource::ExifDigitized)
    }
}

/// Priority list of date sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export, as = "Vec<DateSource>"))]
pub struct DatePolicy {
    pub order: Vec<DateSource>,
}

impl Default for DatePolicy {
    /// 2.x behaviour: filesystem modification time only.
    fn default() -> Self {
        DatePolicy::filesystem_only()
    }
}

impl DatePolicy {
    pub fn filesystem_only() -> Self {
        DatePolicy {
            order: vec![DateSource::FilesystemModified],
        }
    }

    pub fn exif_first() -> Self {
        DatePolicy {
            order: vec![
                DateSource::ExifOriginal,
                DateSource::ExifDigitized,
                DateSource::FilesystemModified,
            ],
        }
    }

    fn needs_exif(&self) -> bool {
        self.order.iter().any(|s| s.is_exif())
    }
}

/// State of one EXIF field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldState {
    Value(NaiveDateTime),
    /// The field does not exist (or the container has no EXIF block).
    Missing,
    /// The field exists but is not a valid date (e.g. `0000:00:00 00:00:00`).
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExifStatus {
    /// EXIF block read (individual fields may still be missing/invalid).
    Read,
    /// File type not readable by the native reader, or no EXIF block.
    NoExif,
    /// The EXIF block is damaged.
    Corrupt,
    /// The file could not be opened or read.
    Unreadable,
    /// The policy does not use EXIF, nothing was read.
    NotRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExifDates {
    pub status: ExifStatus,
    pub original: FieldState,
    pub digitized: FieldState,
}

impl ExifDates {
    fn none(status: ExifStatus) -> Self {
        ExifDates {
            status,
            original: FieldState::Missing,
            digitized: FieldState::Missing,
        }
    }
}

fn field_state(exif: &exif::Exif, tag: exif::Tag) -> FieldState {
    let Some(field) = exif.get_field(tag, exif::In::PRIMARY) else {
        return FieldState::Missing;
    };
    let exif::Value::Ascii(values) = &field.value else {
        return FieldState::Invalid;
    };
    let Some(bytes) = values.first() else {
        return FieldState::Invalid;
    };
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    match NaiveDateTime::parse_from_str(text, "%Y:%m:%d %H:%M:%S") {
        Ok(dt) => FieldState::Value(dt),
        Err(_) => FieldState::Invalid,
    }
}

/// Reads `DateTimeOriginal` and `DateTimeDigitized`.
pub fn read_exif_dates(path: &Path) -> ExifDates {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return ExifDates::none(ExifStatus::Unreadable),
    };
    let mut reader = BufReader::new(file);
    match exif::Reader::new().read_from_container(&mut reader) {
        Ok(exif) => ExifDates {
            status: ExifStatus::Read,
            original: field_state(&exif, exif::Tag::DateTimeOriginal),
            digitized: field_state(&exif, exif::Tag::DateTimeDigitized),
        },
        Err(exif::Error::NotFound(_)) => ExifDates::none(ExifStatus::NoExif),
        Err(exif::Error::Io(_)) => ExifDates::none(ExifStatus::Unreadable),
        Err(_) => ExifDates::none(ExifStatus::Corrupt),
    }
}

/// File facts needed for date resolution.
#[derive(Debug, Clone, Copy)]
pub struct FileTimes {
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResolvedDate {
    pub date: NaiveDate,
    pub source: DateSource,
    /// `true` if the first source of the policy did not deliver.
    pub fell_back: bool,
    pub exif: ExifStatus,
}

fn local_date(t: SystemTime) -> NaiveDate {
    let dt: DateTime<Local> = t.into();
    dt.date_naive()
}

/// Resolves the folder date for one file, or `None` if no source delivers.
pub fn resolve_date(path: &Path, times: FileTimes, policy: &DatePolicy) -> Option<ResolvedDate> {
    let exif = if policy.needs_exif() {
        read_exif_dates(path)
    } else {
        ExifDates::none(ExifStatus::NotRequested)
    };

    let try_source = |source: DateSource| -> Option<NaiveDate> {
        match source {
            DateSource::ExifOriginal => match exif.original {
                FieldState::Value(dt) => Some(dt.date()),
                _ => None,
            },
            DateSource::ExifDigitized => match exif.digitized {
                FieldState::Value(dt) => Some(dt.date()),
                _ => None,
            },
            DateSource::FilesystemCreated => times.created.map(local_date),
            DateSource::FilesystemModified => times.modified.map(local_date),
        }
    };

    for (i, source) in policy.order.iter().enumerate() {
        if let Some(date) = try_source(*source) {
            return Some(ResolvedDate {
                date,
                source: *source,
                fell_back: i > 0,
                exif: exif.status,
            });
        }
    }
    // Documented last resort: modification time, flagged as fallback.
    let tried_modified = policy.order.contains(&DateSource::FilesystemModified);
    if !tried_modified {
        if let Some(date) = try_source(DateSource::FilesystemModified) {
            return Some(ResolvedDate {
                date,
                source: DateSource::FilesystemModified,
                fell_back: true,
                exif: exif.status,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Builds a minimal little-endian TIFF with an Exif IFD that contains the
    /// given `DateTimeOriginal` / `DateTimeDigitized` strings (19 chars each).
    pub(crate) fn tiff_with_dates(original: Option<&str>, digitized: Option<&str>) -> Vec<u8> {
        let entries: Vec<(u16, &str)> = [(0x9003u16, original), (0x9004u16, digitized)]
            .into_iter()
            .filter_map(|(tag, v)| v.map(|v| (tag, v)))
            .collect();
        let n = entries.len() as u32;
        let exif_ifd_offset: u32 = 26;
        let data_offset: u32 = exif_ifd_offset + 2 + 12 * n + 4;

        let mut b = Vec::new();
        b.extend_from_slice(b"II");
        b.extend_from_slice(&42u16.to_le_bytes());
        b.extend_from_slice(&8u32.to_le_bytes());
        // IFD0: one entry -> ExifIFDPointer
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&0x8769u16.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&exif_ifd_offset.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(b.len() as u32, exif_ifd_offset);
        // Exif IFD
        b.extend_from_slice(&(n as u16).to_le_bytes());
        for (i, (tag, _)) in entries.iter().enumerate() {
            b.extend_from_slice(&tag.to_le_bytes());
            b.extend_from_slice(&2u16.to_le_bytes());
            b.extend_from_slice(&20u32.to_le_bytes());
            b.extend_from_slice(&(data_offset + 20 * i as u32).to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        for (_, v) in &entries {
            assert_eq!(v.len(), 19);
            b.extend_from_slice(v.as_bytes());
            b.push(0);
        }
        b
    }

    fn times(modified_secs: Option<u64>) -> FileTimes {
        FileTimes {
            modified: modified_secs.map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s)),
            created: None,
        }
    }

    fn write(dir: &Path, name: &str, data: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, data).unwrap();
        p
    }

    // 2024-03-05 12:00 UTC: same calendar day in every zone UTC-11..UTC+11.
    const MTIME: u64 = 1_709_640_000;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn reads_both_fields_from_tiff_container() {
        let t = tempfile::tempdir().unwrap();
        let p = write(
            t.path(),
            "a.dng",
            &tiff_with_dates(Some("2020:01:02 03:04:05"), Some("2021:06:07 08:09:10")),
        );
        let e = read_exif_dates(&p);
        assert_eq!(e.status, ExifStatus::Read);
        assert_eq!(
            e.original,
            FieldState::Value(
                NaiveDate::from_ymd_opt(2020, 1, 2)
                    .unwrap()
                    .and_hms_opt(3, 4, 5)
                    .unwrap()
            )
        );
        assert!(matches!(e.digitized, FieldState::Value(_)));
    }

    #[test]
    fn exif_first_uses_original_without_fallback() {
        let t = tempfile::tempdir().unwrap();
        let p = write(
            t.path(),
            "a.arw",
            &tiff_with_dates(Some("2020:01:02 03:04:05"), None),
        );
        let r = resolve_date(&p, times(Some(MTIME)), &DatePolicy::exif_first()).unwrap();
        assert_eq!(r.date, d(2020, 1, 2));
        assert_eq!(r.source, DateSource::ExifOriginal);
        assert!(!r.fell_back);
    }

    #[test]
    fn engine_uses_the_configured_exif_date_policy() {
        let t = tempfile::tempdir().unwrap();
        let path = write(
            t.path(),
            "a.arw",
            &tiff_with_dates(Some("2020:01:02 03:04:05"), None),
        );
        let file_times = times(Some(MTIME));
        let file = crate::scanner::ScannedFile {
            path,
            rel_dir: std::path::PathBuf::new(),
            size: 0,
            modified: file_times.modified,
            created: file_times.created,
        };

        let resolved = crate::engine::file_date(&file, &DatePolicy::exif_first()).unwrap();
        assert_eq!(resolved.date, d(2020, 1, 2));
        assert_eq!(resolved.source, DateSource::ExifOriginal);
    }

    #[test]
    fn missing_original_falls_back_to_digitized_and_is_flagged() {
        let t = tempfile::tempdir().unwrap();
        let p = write(
            t.path(),
            "a.arw",
            &tiff_with_dates(None, Some("2021:06:07 08:09:10")),
        );
        let r = resolve_date(&p, times(Some(MTIME)), &DatePolicy::exif_first()).unwrap();
        assert_eq!(
            (r.date, r.source),
            (d(2021, 6, 7), DateSource::ExifDigitized)
        );
        assert!(r.fell_back);
    }

    #[test]
    fn invalid_exif_date_is_not_used() {
        let t = tempfile::tempdir().unwrap();
        let p = write(
            t.path(),
            "a.arw",
            &tiff_with_dates(Some("0000:00:00 00:00:00"), None),
        );
        assert_eq!(read_exif_dates(&p).original, FieldState::Invalid);
        let r = resolve_date(&p, times(Some(MTIME)), &DatePolicy::exif_first()).unwrap();
        assert_eq!(r.source, DateSource::FilesystemModified);
        assert!(r.fell_back);
    }

    #[test]
    fn file_without_exif_falls_back_to_modified() {
        let t = tempfile::tempdir().unwrap();
        let p = write(t.path(), "a.png", b"\x89PNG\r\n\x1a\n");
        assert_eq!(read_exif_dates(&p).status, ExifStatus::NoExif);
        let r = resolve_date(&p, times(Some(MTIME)), &DatePolicy::exif_first()).unwrap();
        assert_eq!(r.date, d(2024, 3, 5));
        assert!(r.fell_back);
    }

    #[test]
    fn truncated_exif_is_reported_not_trusted() {
        let t = tempfile::tempdir().unwrap();
        let mut data = tiff_with_dates(Some("2020:01:02 03:04:05"), None);
        data.truncate(30);
        let p = write(t.path(), "a.tif", &data);
        let s = read_exif_dates(&p).status;
        assert!(
            matches!(s, ExifStatus::Corrupt | ExifStatus::NoExif),
            "{s:?}"
        );
        let r = resolve_date(&p, times(Some(MTIME)), &DatePolicy::exif_first()).unwrap();
        assert_eq!(r.source, DateSource::FilesystemModified);
    }

    #[test]
    fn default_policy_is_2x_filesystem_modified_and_reads_no_exif() {
        let t = tempfile::tempdir().unwrap();
        // Even with EXIF present, the legacy policy must ignore it.
        let p = write(
            t.path(),
            "a.arw",
            &tiff_with_dates(Some("2020:01:02 03:04:05"), None),
        );
        let r = resolve_date(&p, times(Some(MTIME)), &DatePolicy::default()).unwrap();
        assert_eq!(
            (r.date, r.source),
            (d(2024, 3, 5), DateSource::FilesystemModified)
        );
        assert!(!r.fell_back);
        assert_eq!(r.exif, ExifStatus::NotRequested);
    }

    #[test]
    fn no_usable_date_gives_none_instead_of_an_invented_one() {
        let t = tempfile::tempdir().unwrap();
        let p = write(t.path(), "a.png", b"x");
        assert!(resolve_date(&p, times(None), &DatePolicy::exif_first()).is_none());
        assert!(resolve_date(&p, times(None), &DatePolicy::default()).is_none());
    }

    #[test]
    fn created_time_is_only_used_if_available() {
        let t = tempfile::tempdir().unwrap();
        let p = write(t.path(), "a.png", b"x");
        let policy = DatePolicy {
            order: vec![DateSource::FilesystemCreated],
        };
        // Not available -> documented fallback to modified.
        let r = resolve_date(&p, times(Some(MTIME)), &policy).unwrap();
        assert_eq!(r.source, DateSource::FilesystemModified);
        assert!(r.fell_back);
        // Available -> used.
        let ft = FileTimes {
            modified: times(Some(MTIME)).modified,
            created: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(MTIME + 86_400 * 10)),
        };
        let r = resolve_date(&p, ft, &policy).unwrap();
        assert_eq!(r.source, DateSource::FilesystemCreated);
        assert!(!r.fell_back);
    }

    #[test]
    fn policy_serialises_as_plain_list() {
        let json = serde_json::to_string(&DatePolicy::exif_first()).unwrap();
        assert_eq!(
            json,
            r#"["exif_original","exif_digitized","filesystem_modified"]"#
        );
    }
}
