using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace RawImporterCS
{
    /// <summary>
    /// Verwaltet die Persistierung von Anwendungseinstellungen in einer JSON-Datei.
    /// </summary>
    public class AppSettings
    {
        private static readonly string SettingsPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "RawImporter",
            "settings.json"
        );

        public string SourceFolder { get; set; } = "";
        public string TargetFolder { get; set; } = "";
        
        public bool IncludeRaw { get; set; } = true;
        public bool IncludeCr2 { get; set; } = false;
        public bool IncludeCr3 { get; set; } = false;
        public bool IncludeNef { get; set; } = false;
        public bool IncludeRaf { get; set; } = false;
        public bool IncludeOrf { get; set; } = false;
        public bool IncludeRw2 { get; set; } = false;
        public bool IncludeDng { get; set; } = false;
        public bool IncludeJpg { get; set; } = false;
        public bool IncludeJpeg { get; set; } = false; // New: separate JPEG (.jpeg) toggle
        public bool IncludePng { get; set; } = false;
        public bool IncludeTiff { get; set; } = false;
        public bool IncludeHeic { get; set; } = false;
        public bool IncludeAllFiles { get; set; } = false;
        
        public bool SkipDuplicates { get; set; } = true;
        public bool PreserveSubfolders { get; set; } = true;
        public bool CreateSubfoldersPerFileType { get; set; } = false;
        public bool UseMultithread { get; set; } = false;
        
        public string FolderStructure { get; set; } = "Year/Month/Day";
        public string CurrentLanguage { get; set; } = "en";

        [JsonConverter(typeof(JsonStringEnumConverter))]
        public ThemeMode ThemeMode { get; set; } = ThemeMode.System;

        public List<string> ManualExtensions { get; set; } = new();

        /// <summary>
        /// Speichert die Einstellungen in eine JSON-Datei.
        /// </summary>
        public void Save()
        {
            try
            {
                // Verzeichnis erstellen wenn nicht vorhanden
                var directory = Path.GetDirectoryName(SettingsPath);
                if (directory != null && !Directory.Exists(directory))
                {
                    Directory.CreateDirectory(directory);
                }

                var options = new JsonSerializerOptions
                {
                    WriteIndented = true,
                    Converters = { new JsonStringEnumConverter() }
                };

                string json = JsonSerializer.Serialize(this, options);
                File.WriteAllText(SettingsPath, json);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Fehler beim Speichern der Einstellungen: {ex.Message}");
            }
        }

        /// <summary>
        /// Lädt die Einstellungen aus der JSON-Datei.
        /// Falls Datei nicht existiert, werden Standardwerte verwendet.
        /// </summary>
        public static AppSettings Load()
        {
            try
            {
                if (File.Exists(SettingsPath))
                {
                    string json = File.ReadAllText(SettingsPath);
                    var options = new JsonSerializerOptions
                    {
                        Converters = { new JsonStringEnumConverter() }
                    };

                    var settings = JsonSerializer.Deserialize<AppSettings>(json, options) ?? new AppSettings();

                    try
                    {
                        using var doc = JsonDocument.Parse(json);
                        var root = doc.RootElement;
                        if (root.TryGetProperty("ThemeMode", out var modeProp))
                        {
                            var modeString = modeProp.GetString();
                            if (!string.IsNullOrWhiteSpace(modeString) &&
                                Enum.TryParse<ThemeMode>(modeString, true, out var parsedMode))
                            {
                                settings.ThemeMode = parsedMode;
                            }
                        }
                        else if (root.TryGetProperty("Theme", out var legacyThemeProp))
                        {
                            var legacyValue = legacyThemeProp.GetString();
                            if (string.Equals(legacyValue, "dark", StringComparison.OrdinalIgnoreCase))
                            {
                                settings.ThemeMode = ThemeMode.Dark;
                            }
                            else if (string.Equals(legacyValue, "light", StringComparison.OrdinalIgnoreCase))
                            {
                                settings.ThemeMode = ThemeMode.Light;
                            }
                            else
                            {
                                settings.ThemeMode = ThemeMode.System;
                            }
                        }
                    }
                    catch
                    {
                        // Fallback to defaults if migration fails
                    }

                    return settings;
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Fehler beim Laden der Einstellungen: {ex.Message}");
            }

            return new AppSettings();
        }

        /// <summary>
        /// Löscht die gespeicherten Einstellungen.
        /// </summary>
        public static void Delete()
        {
            try
            {
                if (File.Exists(SettingsPath))
                {
                    File.Delete(SettingsPath);
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Fehler beim Löschen der Einstellungen: {ex.Message}");
            }
        }
    }
}
