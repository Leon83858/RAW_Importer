using System.Collections.Generic;

namespace RawImporterCS
{
    public static class Language
    {
        private static string _currentLanguage = "en";

        public static string CurrentLanguage
        {
            get => _currentLanguage;
            set
            {
                if (value == "de" || value == "en")
                    _currentLanguage = value;
            }
        }

        private static readonly Dictionary<string, (string de, string en)> _texts = new()
        {
            { "app.title", ("RAW Importer", "RAW Importer") },
            { "status.ready", ("Bereit", "Ready") },
            { "status.scanning", ("Scanne…", "Scanning…") },
            { "status.scanComplete", ("Scan abgeschlossen", "Scan Complete") },
            { "status.importing", ("Import läuft...", "Import in progress...") },
            { "status.done", ("Fertig.", "Done.") },
            { "label.source", ("Quellordner:", "Source folder:") },
            { "label.target", ("Zielordner:", "Target folder:") },
            { "button.browse", ("Durchsuchen...", "Browse...") },
            { "button.browseSource", ("Quellordner wählen", "Choose source folder") },
            { "button.browseTarget", ("Zielordner wählen", "Choose target folder") },
            { "group.options", ("Optionen", "Options") },
            { "checkbox.raw", ("RAW", "RAW") },
            { "checkbox.jpg", ("JPG", "JPG") },
            { "checkbox.video", ("Video", "Video") },
            { "checkbox.skipdup", ("Duplikate über EXIF erkennen", "Detect duplicates via EXIF") },
            { "checkbox.multithread", ("Multithread (experimentell)", "Multithreaded import (experimental)") },
            { "checkbox.preserve", ("Unterordner beibehalten", "Preserve subfolders") },
            { "checkbox.filetypeSubfolders", ("Unterordner pro Dateityp erstellen", "Create subfolders per file type") },
            { "label.filetypes", ("Dateitypen:", "Filetypes:") },
            { "label.extensions", ("Zusätzliche Endungen (mit Komma):", "Additional extensions (comma separated):") },
            { "label.extensions.enter", ("Zusätzliche Endungen (mit Enter bestätigen):", "Additional extensions (confirm with Enter):") },
            { "label.folderstructure", ("Ordnerstruktur:", "Folder structure:") },
            { "folder.ymd", ("Jahr/Monat/Tag", "Year/Month/Day") },
            { "folder.ym", ("Jahr/Monat", "Year/Month") },
            { "folder.y", ("Jahr", "Year") },
            { "folder.dmy", ("Tag-Monat-Jahr", "Day-Month-Year") },
            { "folder.flat", ("Flach", "Flat") },
            { "label.allfiles", ("Alle Dateien", "All files") },
            { "unit.files", ("Dateien", "files") },
            { "button.scan", ("Scannen", "Scan") },
            { "button.import", ("Import starten", "Start import") },
            { "button.cancel", ("Abbrechen", "Cancel") },
            { "menu.file", ("_Datei", "_File") },
            { "menu.exit", ("Beenden", "Exit") },
            { "menu.language", ("_Sprache", "_Language") },
            { "menu.german", ("Deutsch", "German") },
            { "menu.english", ("English", "English") },
            { "menu.credits", ("Credits", "Credits") },
            { "menu.theme", ("_Design", "_Theme") },
            { "menu.system", ("System (OS)", "System") },
            { "menu.light", ("Heller Modus", "Light mode") },
            { "menu.dark", ("Dunkler Modus", "Dark mode") },
            { "menu.instagram", ("Instagram - @thelomphotography", "Instagram - @thelomphotography") },
            { "dialog.source", ("Quellordner auswählen", "Select source folder") },
            { "dialog.target", ("Zielordner auswählen", "Select target folder") },
            { "error.title", ("Fehler", "Error") },
            { "info.title", ("Hinweis", "Information") },
            { "error.noscan", ("Es wurden noch keine Dateien gescannt. Trotzdem fortfahren?", "No files have been scanned yet. Continue anyway?") },
            { "error.scancancelled", ("Scan abgebrochen.", "Scan cancelled.") },
            { "error.scandone", ("Gefundene Dateien: ", "Files found: ") },
            { "error.importcancelled", ("Import abgebrochen.", "Import cancelled.") },
            { "error.importdone", ("Fehler beim Import.", "Error during import.") },
            { "error.scantitle", ("Fehler beim Scan", "Scan error") },
            { "error.invalidSource", ("Bitte einen gültigen Quellordner auswählen.", "Please choose a valid source folder.") },
            { "error.invalidTarget", ("Bitte einen Zielordner auswählen.", "Please choose a target folder.") },
            { "error.noFiletypes", ("Bitte mindestens einen Dateityp oder eine manuelle Endung auswählen.", "Please select at least one file type or manual extension.") }
        };

        public static string T(string key)
        {
            if (_texts.TryGetValue(key, out var value))
            {
                return _currentLanguage == "en" ? value.en : value.de;
            }

            return key;
        }
    }
}
