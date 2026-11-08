//! Native menu bar.
//!
//! * macOS: the system menu bar with the conventional menus (app menu,
//!   File, Edit, View with Language/Appearance, Window, Help). An Edit menu
//!   is required on macOS for ⌘C/⌘V/⌘A to work in text fields.
//! * Windows/Linux: the 2.x menu structure (File → Exit, Language, Theme,
//!   Credits → Instagram).
//!
//! Labels are localised here; language and theme changes are forwarded to
//! the UI (which persists them) and the UI calls `apply_ui`, which rebuilds
//! the menu with the new labels and check marks.

use raw_importer_core::settings::{Language, ThemeMode};
use serde::Serialize;
use tauri::menu::{Menu, MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Runtime};

pub const MENU_EVENT: &str = "menu-action";
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MenuAction {
    OpenSettings,
}

struct Labels {
    file: &'static str,
    edit: &'static str,
    window: &'static str,
    settings: &'static str,
    exit: &'static str,
    close_window: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
    minimize: &'static str,
    zoom: &'static str,
    about: &'static str,
    hide: &'static str,
    hide_others: &'static str,
    show_all: &'static str,
    services: &'static str,
    quit: &'static str,
}

fn labels(language: Language) -> &'static Labels {
    match language {
        Language::English => &Labels {
            file: "File",
            edit: "Edit",
            window: "Window",
            settings: "Settings…",
            exit: "Exit",
            close_window: "Close Window",
            undo: "Undo",
            redo: "Redo",
            cut: "Cut",
            copy: "Copy",
            paste: "Paste",
            select_all: "Select All",
            minimize: "Minimise",
            zoom: "Zoom",
            about: "About RAW Importer",
            hide: "Hide RAW Importer",
            hide_others: "Hide Others",
            show_all: "Show All",
            services: "Services",
            quit: "Quit RAW Importer",
        },
        Language::German => &Labels {
            file: "Ablage",
            edit: "Bearbeiten",
            window: "Fenster",
            settings: "Einstellungen …",
            exit: "Beenden",
            close_window: "Fenster schließen",
            undo: "Widerrufen",
            redo: "Wiederholen",
            cut: "Ausschneiden",
            copy: "Kopieren",
            paste: "Einsetzen",
            select_all: "Alles auswählen",
            minimize: "Im Dock ablegen",
            zoom: "Zoomen",
            about: "Über RAW Importer",
            hide: "RAW Importer ausblenden",
            hide_others: "Andere ausblenden",
            show_all: "Alle einblenden",
            services: "Dienste",
            quit: "RAW Importer beenden",
        },
        Language::French => &Labels {
            file: "Fichier",
            edit: "Édition",
            window: "Fenêtre",
            settings: "Réglages…",
            exit: "Quitter",
            close_window: "Fermer la fenêtre",
            undo: "Annuler",
            redo: "Rétablir",
            cut: "Couper",
            copy: "Copier",
            paste: "Coller",
            select_all: "Tout sélectionner",
            minimize: "Placer dans le Dock",
            zoom: "Réduire/agrandir",
            about: "À propos de RAW Importer",
            hide: "Masquer RAW Importer",
            hide_others: "Masquer les autres",
            show_all: "Tout afficher",
            services: "Services",
            quit: "Quitter RAW Importer",
        },
        Language::Dutch | Language::DutchBelgium => &Labels {
            file: "Archief",
            edit: "Wijzig",
            window: "Venster",
            settings: "Instellingen…",
            exit: "Afsluiten",
            close_window: "Sluit venster",
            undo: "Herstel",
            redo: "Opnieuw",
            cut: "Knip",
            copy: "Kopieer",
            paste: "Plak",
            select_all: "Selecteer alles",
            minimize: "Minimaliseer",
            zoom: "Zoom",
            about: "Over RAW Importer",
            hide: "Verberg RAW Importer",
            hide_others: "Verberg andere",
            show_all: "Toon alles",
            services: "Voorzieningen",
            quit: "Stop RAW Importer",
        },
    }
}

/// Builds the complete menu bar for the given language and theme.
pub fn build<R: Runtime>(
    app: &AppHandle<R>,
    language: Language,
    _theme: ThemeMode,
) -> tauri::Result<Menu<R>> {
    use tauri::menu::PredefinedMenuItem as P;
    let l = labels(language);
    let settings = MenuItemBuilder::with_id("settings:open", l.settings)
        .accelerator("CmdOrCtrl+,")
        .build(app)?;

    if cfg!(target_os = "macos") {
        let app_menu = SubmenuBuilder::new(app, "RAW Importer")
            .item(&P::about(app, Some(l.about), None)?)
            .separator()
            .item(&settings)
            .separator()
            .item(&P::services(app, Some(l.services))?)
            .separator()
            .item(&P::hide(app, Some(l.hide))?)
            .item(&P::hide_others(app, Some(l.hide_others))?)
            .item(&P::show_all(app, Some(l.show_all))?)
            .separator()
            .item(&P::quit(app, Some(l.quit))?)
            .build()?;
        let file = SubmenuBuilder::new(app, l.file)
            .item(&P::close_window(app, Some(l.close_window))?)
            .build()?;
        let edit = SubmenuBuilder::new(app, l.edit)
            .item(&P::undo(app, Some(l.undo))?)
            .item(&P::redo(app, Some(l.redo))?)
            .separator()
            .item(&P::cut(app, Some(l.cut))?)
            .item(&P::copy(app, Some(l.copy))?)
            .item(&P::paste(app, Some(l.paste))?)
            .item(&P::select_all(app, Some(l.select_all))?)
            .build()?;
        let window = SubmenuBuilder::new(app, l.window)
            .item(&P::minimize(app, Some(l.minimize))?)
            .item(&P::maximize(app, Some(l.zoom))?)
            .build()?;
        MenuBuilder::new(app)
            .items(&[&app_menu, &file, &edit, &window])
            .build()
    } else {
        // Settings replace the old language/theme/credits menus.
        let file = SubmenuBuilder::new(app, l.file)
            .item(&settings)
            .separator()
            .item(&P::quit(app, Some(l.exit))?)
            .build()?;
        MenuBuilder::new(app).item(&file).build()
    }
}

/// Handles clicks on our own (non-predefined) items.
pub fn handle_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().as_ref();
    if id == "settings:open" {
        if let Err(e) = app.emit(MENU_EVENT, MenuAction::OpenSettings) {
            eprintln!("could not forward menu action: {e}");
        }
    }
}
