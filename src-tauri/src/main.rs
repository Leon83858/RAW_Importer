mod commands;
mod menu;

use raw_importer_core::session::{ImportSession, SessionPaths};
use raw_importer_core::settings::{Language, ThemeMode};
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            app.manage(ImportSession::new(SessionPaths {
                history_db: data.join("history.sqlite"),
                journal: data.join("recovery-journal.jsonl"),
            }));
            // Default labels until the UI has loaded the saved language.
            let handle = app.handle().clone();
            handle.set_menu(menu::build(&handle, Language::English, ThemeMode::System)?)?;
            Ok(())
        })
        .on_menu_event(menu::handle_event)
        .invoke_handler(tauri::generate_handler![
            commands::validate_paths,
            commands::format_registry,
            commands::load_settings,
            commands::save_settings,
            commands::apply_ui,
            commands::check_for_updates,
            commands::scan,
            commands::start_import,
            commands::session_state,
            commands::pause_import,
            commands::resume_import,
            commands::cancel_session,
            commands::reset_session
        ])
        .run(tauri::generate_context!())
        .expect("failed to run RAW Importer");
}
