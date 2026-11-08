//! Versioned settings storage and 2.x migration.
//!
//! Migration reads the legacy file without modifying it, writes a timestamped
//! backup beside it, and creates the v3 file only when one does not exist.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::destination::{Layout, MonthNaming, StructureOptions};
use crate::duplicates::{ConflictPolicy, DuplicateScope};
use crate::engine::Parallelism;
use crate::formats::{FormatId, FormatSelection};
use crate::metadata::DatePolicy;

pub const SETTINGS_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum Language {
    /// English (United Kingdom). Stored as `english` for compatibility with
    /// settings written before more languages existed.
    #[default]
    English,
    German,
    French,
    Dutch,
    /// Dutch as used in Belgium (Flemish).
    DutchBelgium,
}

impl Language {
    pub const ALL: [Language; 5] = [
        Language::English,
        Language::German,
        Language::French,
        Language::Dutch,
        Language::DutchBelgium,
    ];

    /// BCP 47 tag used for number/date formatting and the UI catalogue.
    pub fn tag(self) -> &'static str {
        match self {
            Language::English => "en-GB",
            Language::German => "de-DE",
            Language::French => "fr-FR",
            Language::Dutch => "nl-NL",
            Language::DutchBelgium => "nl-BE",
        }
    }

    /// The language's own name, as shown in language menus.
    pub fn native_name(self) -> &'static str {
        match self {
            Language::English => "English (UK)",
            Language::German => "Deutsch",
            Language::French => "Français",
            Language::Dutch => "Nederlands",
            Language::DutchBelgium => "Nederlands (België)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct SettingsPaths {
    pub source: Option<PathBuf>,
    pub destination: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct UiSettings {
    pub language: Language,
    pub theme: ThemeMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct DuplicateSettings {
    pub content_detection_enabled: bool,
    pub scopes: BTreeSet<DuplicateScope>,
    pub use_hash_cache: bool,
    pub force_rehash: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct UpdateSettings {
    pub check_on_start: bool,
    pub prompted_for_version: Option<String>,
}

impl Default for DuplicateSettings {
    fn default() -> Self {
        Self {
            content_detection_enabled: false,
            scopes: [DuplicateScope::DestinationDirectory].into_iter().collect(),
            use_hash_cache: true,
            force_rehash: false,
        }
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            language: Language::English,
            theme: ThemeMode::System,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct Settings {
    pub version: u32,
    pub paths: SettingsPaths,
    pub formats: FormatSelection,
    pub conflict: ConflictPolicy,
    pub structure: StructureOptions,
    pub parallelism: Parallelism,
    pub date_policy: DatePolicy,
    pub duplicates: DuplicateSettings,
    pub updates: UpdateSettings,
    pub ui: UiSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            paths: SettingsPaths::default(),
            formats: FormatSelection::legacy_default(),
            conflict: ConflictPolicy::Skip,
            structure: StructureOptions {
                month_naming: MonthNaming::EnglishCompat,
                preserve_subfolders: true,
                ..Default::default()
            },
            parallelism: Parallelism::Sequential,
            date_policy: DatePolicy::default(),
            duplicates: DuplicateSettings::default(),
            updates: UpdateSettings::default(),
            ui: UiSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsLoad {
    pub settings: Settings,
    pub migrated_from_2x: bool,
    pub backup_path: Option<PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("settings I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid settings JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("settings root must be a JSON object")]
    InvalidRoot,
    #[error("settings version {0} is not supported")]
    UnsupportedVersion(u32),
    #[error("legacy and current settings paths must be different")]
    SamePath,
}

fn io_error(path: &Path, source: std::io::Error) -> SettingsError {
    SettingsError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Loads v3 settings, returning defaults only when the file is absent.
pub fn load(path: &Path) -> Result<Settings, SettingsError> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes = fs::read(path).map_err(|e| io_error(path, e))?;
    let settings: Settings = serde_json::from_slice(&bytes)?;
    if settings.version != SETTINGS_VERSION {
        return Err(SettingsError::UnsupportedVersion(settings.version));
    }
    Ok(settings)
}

/// Loads the v3 file first; otherwise migrates an existing 2.x file.
pub fn load_or_migrate(
    current_path: &Path,
    legacy_path: &Path,
) -> Result<SettingsLoad, SettingsError> {
    if current_path == legacy_path {
        return Err(SettingsError::SamePath);
    }
    if current_path.exists() {
        return Ok(SettingsLoad {
            settings: load(current_path)?,
            migrated_from_2x: false,
            backup_path: None,
            warnings: Vec::new(),
        });
    }
    if !legacy_path.exists() {
        return Ok(SettingsLoad {
            settings: Settings::default(),
            migrated_from_2x: false,
            backup_path: None,
            warnings: Vec::new(),
        });
    }

    let legacy_bytes = fs::read(legacy_path).map_err(|e| io_error(legacy_path, e))?;
    let legacy: serde_json::Value = serde_json::from_slice(&legacy_bytes)?;
    let (settings, warnings) = migrate_2x_value(&legacy)?;
    let backup_path = backup_legacy(current_path, &legacy_bytes)?;
    save(current_path, &settings)?;
    Ok(SettingsLoad {
        settings,
        migrated_from_2x: true,
        backup_path: Some(backup_path),
        warnings,
    })
}

/// Atomically replaces the current v3 settings file.
pub fn save(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
    let bytes = serde_json::to_vec_pretty(settings)?;
    let parent = parent_dir(path);
    fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| "settings.json".into());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{name}.tmp-{}-{nonce}", std::process::id()));
    let result = write_new_file(&temporary, &bytes)
        .and_then(|_| fs::rename(&temporary, path).map_err(|e| io_error(path, e)));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    } else {
        crate::platform::sync_dir(parent);
    }
    result
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), SettingsError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| io_error(path, e))?;
    file.write_all(bytes).map_err(|e| io_error(path, e))?;
    file.sync_all().map_err(|e| io_error(path, e))
}

fn backup_legacy(current_path: &Path, bytes: &[u8]) -> Result<PathBuf, SettingsError> {
    let parent = parent_dir(current_path);
    fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
    let name = current_path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_else(|| "settings.json".into());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let backup = parent.join(format!("{name}.v2-backup-{stamp}"));
    write_new_file(&backup, bytes)?;
    Ok(backup)
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn migrate_2x_value(value: &serde_json::Value) -> Result<(Settings, Vec<String>), SettingsError> {
    let root = value.as_object().ok_or(SettingsError::InvalidRoot)?;
    let mut settings = Settings::default();
    let mut warnings = Vec::new();

    settings.paths.source = legacy_path(root.get("SourceFolder"), "SourceFolder", &mut warnings);
    settings.paths.destination =
        legacy_path(root.get("TargetFolder"), "TargetFolder", &mut warnings);

    let formats = [
        ("IncludeRaw", FormatId::Arw, true),
        ("IncludeCr2", FormatId::Cr2, false),
        ("IncludeCr3", FormatId::Cr3, false),
        ("IncludeNef", FormatId::Nef, false),
        ("IncludeRaf", FormatId::Raf, false),
        ("IncludeOrf", FormatId::Orf, false),
        ("IncludeRw2", FormatId::Rw2, false),
        ("IncludeDng", FormatId::Dng, false),
        ("IncludeJpg", FormatId::Jpg, false),
        ("IncludeJpeg", FormatId::Jpeg, false),
        ("IncludePng", FormatId::Png, false),
        ("IncludeTiff", FormatId::Tiff, false),
        ("IncludeHeic", FormatId::Heic, false),
    ];
    for (key, id, default) in formats {
        if legacy_bool(root.get(key), key, default, &mut warnings) {
            settings.formats.enabled.insert(id);
        }
    }
    settings.formats.include_all = legacy_bool(
        root.get("IncludeAllFiles"),
        "IncludeAllFiles",
        false,
        &mut warnings,
    );

    let skip = legacy_bool(
        root.get("SkipDuplicates"),
        "SkipDuplicates",
        true,
        &mut warnings,
    );
    settings.conflict = if skip {
        ConflictPolicy::Skip
    } else {
        ConflictPolicy::Rename
    };
    settings.structure.preserve_subfolders = legacy_bool(
        root.get("PreserveSubfolders"),
        "PreserveSubfolders",
        true,
        &mut warnings,
    );
    settings.structure.type_folders = legacy_bool(
        root.get("CreateSubfoldersPerFileType"),
        "CreateSubfoldersPerFileType",
        false,
        &mut warnings,
    );
    settings.parallelism = if legacy_bool(
        root.get("UseMultithread"),
        "UseMultithread",
        false,
        &mut warnings,
    ) {
        Parallelism::Auto
    } else {
        Parallelism::Sequential
    };

    if let Some(layout) = legacy_string(
        root.get("FolderStructure"),
        "FolderStructure",
        &mut warnings,
    ) {
        match Layout::from_legacy(&layout) {
            Some(layout) => settings.structure.layout = layout,
            None => warnings.push(format!(
                "FolderStructure value {layout:?} is unknown; using Year/Month/Day"
            )),
        }
    }

    match root.get("ManualExtensions") {
        None => {}
        Some(serde_json::Value::Array(values)) => {
            for value in values {
                match value.as_str() {
                    Some(extension) => match settings.formats.add_manual(extension) {
                        Ok(_) => {}
                        Err(error) => warnings.push(error.to_string()),
                    },
                    None => warnings.push("ManualExtensions contains a non-string value".into()),
                }
            }
        }
        Some(_) => warnings.push("ManualExtensions is not an array".into()),
    }

    if let Some(language) = legacy_string(
        root.get("CurrentLanguage"),
        "CurrentLanguage",
        &mut warnings,
    ) {
        settings.ui.language = match language.to_ascii_lowercase().as_str() {
            "de" => Language::German,
            "en" => Language::English,
            _ => {
                warnings.push(format!(
                    "CurrentLanguage value {language:?} is unknown; using en"
                ));
                Language::English
            }
        };
    }
    let theme_key = if root.contains_key("ThemeMode") {
        "ThemeMode"
    } else {
        "Theme"
    };
    if let Some(theme) = legacy_string(root.get(theme_key), theme_key, &mut warnings) {
        settings.ui.theme = match theme.to_ascii_lowercase().as_str() {
            "dark" => ThemeMode::Dark,
            "light" => ThemeMode::Light,
            "system" => ThemeMode::System,
            _ => {
                warnings.push(format!(
                    "{theme_key} value {theme:?} is unknown; using system"
                ));
                ThemeMode::System
            }
        };
    }

    settings.version = SETTINGS_VERSION;
    settings.date_policy = DatePolicy::filesystem_only();
    settings.structure.month_naming = MonthNaming::EnglishCompat;
    Ok((settings, warnings))
}

fn legacy_path(
    value: Option<&serde_json::Value>,
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<PathBuf> {
    legacy_string(value, key, warnings)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

fn legacy_bool(
    value: Option<&serde_json::Value>,
    key: &str,
    default: bool,
    warnings: &mut Vec<String>,
) -> bool {
    match value {
        None => default,
        Some(serde_json::Value::Bool(value)) => *value,
        Some(_) => {
            warnings.push(format!("{key} is not a boolean; using its default"));
            default
        }
    }
}

fn legacy_string(
    value: Option<&serde_json::Value>,
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<String> {
    match value {
        None => None,
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        Some(_) => {
            warnings.push(format!("{key} is not a string; using its default"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_german_layout_and_keeps_legacy_file_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("v2/settings.json");
        let current = temp.path().join("v3/settings.json");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let legacy_bytes = br#"{
            "SourceFolder":"/photos/in",
            "TargetFolder":"/photos/out",
            "IncludeRaw":true,
            "IncludeJpg":true,
            "SkipDuplicates":false,
            "PreserveSubfolders":true,
            "CreateSubfoldersPerFileType":true,
            "UseMultithread":true,
            "FolderStructure":"Jahr/Monat/Tag",
            "CurrentLanguage":"de",
            "Theme":"dark",
            "ManualExtensions":[".xmp", "bad/ext"]
        }"#;
        fs::write(&legacy, legacy_bytes).unwrap();

        let loaded = load_or_migrate(&current, &legacy).unwrap();
        assert!(loaded.migrated_from_2x);
        assert_eq!(loaded.settings.version, SETTINGS_VERSION);
        assert_eq!(
            loaded.settings.paths.source,
            Some(PathBuf::from("/photos/in"))
        );
        assert_eq!(loaded.settings.structure.layout, Layout::YearMonthDay);
        assert_eq!(
            loaded.settings.structure.month_naming,
            MonthNaming::EnglishCompat
        );
        assert_eq!(loaded.settings.conflict, ConflictPolicy::Rename);
        assert_eq!(loaded.settings.parallelism, Parallelism::Auto);
        assert_eq!(loaded.settings.ui.language, Language::German);
        assert_eq!(loaded.settings.ui.theme, ThemeMode::Dark);
        assert!(loaded.settings.formats.enabled.contains(&FormatId::Arw));
        assert!(loaded.settings.formats.enabled.contains(&FormatId::Jpg));
        assert!(loaded.settings.formats.manual.contains("xmp"));
        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(fs::read(&legacy).unwrap(), legacy_bytes);
        let backup_path = loaded.backup_path.unwrap();
        assert_eq!(backup_path.parent(), current.parent());
        assert_eq!(fs::read(backup_path).unwrap(), legacy_bytes);
        assert_eq!(load(&current).unwrap(), loaded.settings);
    }

    #[test]
    fn unknown_and_malformed_legacy_values_are_reported() {
        let value = serde_json::json!({
            "FolderStructure": "Jahr/Quartal",
            "IncludeRaw": "yes",
            "CurrentLanguage": "fr",
            "ManualExtensions": ["bad/ext"]
        });
        let (settings, warnings) = migrate_2x_value(&value).unwrap();
        assert_eq!(settings.structure.layout, Layout::YearMonthDay);
        assert_eq!(
            settings.formats.enabled,
            [FormatId::Arw].into_iter().collect()
        );
        assert_eq!(settings.ui.language, Language::English);
        assert_eq!(warnings.len(), 4);
    }

    #[test]
    fn valid_current_settings_are_never_overwritten_by_migration() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("legacy.json");
        let current = temp.path().join("settings.json");
        fs::write(&legacy, br#"{"FolderStructure":"Flach"}"#).unwrap();
        let mut settings = Settings::default();
        settings.ui.language = Language::German;
        save(&current, &settings).unwrap();
        let before = fs::read(&current).unwrap();

        let result = load_or_migrate(&current, &legacy).unwrap();
        assert!(!result.migrated_from_2x);
        assert!(result.backup_path.is_none());
        assert_eq!(fs::read(&current).unwrap(), before);
        assert_eq!(result.settings.ui.language, Language::German);
    }

    #[test]
    fn duplicate_scopes_and_hash_cache_options_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let settings = Settings {
            duplicates: DuplicateSettings {
                content_detection_enabled: true,
                scopes: [
                    DuplicateScope::DestinationTree,
                    DuplicateScope::PreviousImports,
                ]
                .into_iter()
                .collect(),
                use_hash_cache: true,
                force_rehash: false,
            },
            ..Settings::default()
        };
        save(&path, &settings).unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.duplicates, settings.duplicates);
        let json = fs::read_to_string(path).unwrap();
        assert!(json.contains("destination_tree"));
        assert!(json.contains("previous_imports"));
    }

    #[test]
    fn update_check_opt_in_and_prompted_version_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let settings = Settings {
            updates: UpdateSettings {
                check_on_start: true,
                prompted_for_version: Some("3.0.0".into()),
            },
            ..Settings::default()
        };
        save(&path, &settings).unwrap();

        assert_eq!(load(&path).unwrap().updates, settings.updates);
    }

    #[test]
    fn update_checks_default_to_opt_in_prompt_not_automatic() {
        assert!(!Settings::default().updates.check_on_start);
        assert_eq!(Settings::default().updates.prompted_for_version, None);
    }

    #[test]
    fn existing_v3_settings_without_update_choice_remain_opted_out() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        std::fs::write(&path, br#"{"version":3}"#).unwrap();

        let loaded = load(&path).unwrap();
        assert!(!loaded.updates.check_on_start);
        assert_eq!(loaded.updates.prompted_for_version, None);
    }

    #[test]
    fn language_identifiers_are_stable() {
        // Existing v3 files contain "english"/"german"; new values must not break them.
        let names: Vec<String> = Language::ALL
            .iter()
            .map(|l| serde_json::to_string(l).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "\"english\"",
                "\"german\"",
                "\"french\"",
                "\"dutch\"",
                "\"dutch_belgium\""
            ]
        );
        assert_eq!(Language::default(), Language::English);
        assert_eq!(Language::English.tag(), "en-GB");
        assert_eq!(Language::DutchBelgium.tag(), "nl-BE");
    }
}
