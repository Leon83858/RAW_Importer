//! Thin IPC layer: every command validates/forwards to `raw-importer-core`.
//! No import logic lives here (ARCHITECTURE.md §2).

use std::path::{Path, PathBuf};

use raw_importer_core::error::ErrorCode;
use raw_importer_core::formats::{self, FormatDescriptor};
use raw_importer_core::paths::{self, PathIssue, ValidatedPaths};
use raw_importer_core::session::{ImportSession, ScanPreview, SessionError, SessionSnapshot};
use raw_importer_core::settings::{Language, Settings, ThemeMode};
use raw_importer_core::updates::UpdateStatus;
use serde::Deserialize;
use serde::Serialize;
use tauri::{Emitter, Manager, State};

/// Event carrying a [`SessionSnapshot`] while an import runs (≤ 10 Hz).
pub const PROGRESS_EVENT: &str = "import-progress";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Leon83858/RAW_Importer/releases/latest";
const RELEASE_PAGE_PREFIX: &str = "https://github.com/Leon83858/RAW_Importer/releases/tag/";

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
}

#[tauri::command]
pub async fn check_for_updates(app: tauri::AppHandle) -> Result<UpdateStatus, String> {
    let current_version = app.package_info().version.to_string();
    let client = reqwest::Client::builder()
        .user_agent(format!("RAW-Importer/{current_version}"))
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(LATEST_RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| format!("GitHub release check failed: {error}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateStatus::NoStableRelease { current_version });
    }
    let release: GitHubRelease = response
        .error_for_status()
        .map_err(|error| format!("GitHub release check failed: {error}"))?
        .json()
        .await
        .map_err(|error| format!("GitHub release response was invalid: {error}"))?;
    if release.draft || release.prerelease {
        return Ok(UpdateStatus::NoStableRelease { current_version });
    }
    if !release.html_url.starts_with(RELEASE_PAGE_PREFIX) {
        return Err("GitHub returned an unexpected release URL".into());
    }
    raw_importer_core::updates::compare_release(
        &current_version,
        &release.tag_name,
        &release.html_url,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn validate_paths(
    source: String,
    destination: String,
) -> Result<ValidatedPaths, Vec<PathIssue>> {
    paths::validate(Path::new(&source), Path::new(&destination))
}

/// Supported formats from the central registry in the core.
#[tauri::command]
pub fn format_registry() -> Vec<FormatDescriptor> {
    formats::descriptors()
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsResponse {
    pub settings: Settings,
    pub migrated_from_2x: bool,
    pub warnings: Vec<String>,
    pub app_version: String,
}

fn settings_paths(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let current = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?
        .join("settings.json");
    let legacy_root = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
    }
    .ok_or_else(|| "could not locate the legacy settings directory".to_string())?;
    Ok((
        current,
        legacy_root.join("RawImporter").join("settings.json"),
    ))
}

#[tauri::command]
pub fn load_settings(app: tauri::AppHandle) -> Result<SettingsResponse, String> {
    let (current, legacy) = settings_paths(&app)?;
    let loaded = raw_importer_core::settings::load_or_migrate(&current, &legacy)
        .map_err(|error| error.to_string())?;
    Ok(SettingsResponse {
        settings: loaded.settings,
        migrated_from_2x: loaded.migrated_from_2x,
        warnings: loaded.warnings,
        app_version: app.package_info().version.to_string(),
    })
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), String> {
    let (current, _) = settings_paths(&app)?;
    raw_importer_core::settings::save(&current, &settings).map_err(|error| error.to_string())
}

/// Rebuilds the native menu for the UI language and marks the active theme.
/// Called by the UI after loading settings and after every change.
#[tauri::command]
pub fn apply_ui(app: tauri::AppHandle, language: Language, theme: ThemeMode) -> Result<(), String> {
    let menu = crate::menu::build(&app, language, theme).map_err(|e| e.to_string())?;
    app.set_menu(menu).map_err(|e| e.to_string())?;
    Ok(())
}

fn join_error(e: tauri::Error) -> SessionError {
    SessionError {
        code: ErrorCode::Io,
        message: format!("background task failed: {e}"),
        issues: Vec::new(),
    }
}

/// Scans the source and plans all destinations. Runs off the main thread;
/// can be cancelled with `cancel_session`.
#[tauri::command]
pub async fn scan(
    session: State<'_, ImportSession>,
    source: String,
    destination: String,
    settings: Settings,
) -> Result<ScanPreview, SessionError> {
    let session = session.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        session.scan(Path::new(&source), Path::new(&destination), settings)
    })
    .await
    .map_err(join_error)?
}

/// Starts the import of the reviewed preview. Progress arrives as
/// `import-progress` events; the final snapshot is the last event.
#[tauri::command]
pub fn start_import(
    app: tauri::AppHandle,
    session: State<'_, ImportSession>,
) -> Result<String, SessionError> {
    session.start_import(move |snapshot| {
        if let Err(e) = app.emit(PROGRESS_EVENT, snapshot) {
            eprintln!("could not emit progress: {e}");
        }
    })
}

#[tauri::command]
pub fn session_state(session: State<'_, ImportSession>) -> SessionSnapshot {
    session.snapshot()
}

#[tauri::command]
pub fn pause_import(session: State<'_, ImportSession>) -> Result<SessionSnapshot, SessionError> {
    session.pause()
}

#[tauri::command]
pub fn resume_import(session: State<'_, ImportSession>) -> Result<SessionSnapshot, SessionError> {
    session.resume()
}

#[tauri::command]
pub fn cancel_session(session: State<'_, ImportSession>) -> Result<SessionSnapshot, SessionError> {
    session.cancel()
}

#[tauri::command]
pub fn reset_session(session: State<'_, ImportSession>) -> Result<SessionSnapshot, SessionError> {
    session.reset()
}
