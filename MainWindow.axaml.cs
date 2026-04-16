using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Threading;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using Avalonia.Styling;
using Avalonia.Threading;

namespace RawImporterCS;

/// <summary>
/// Hauptfenster des RAW Importers (Avalonia Desktop).
/// </summary>
public partial class MainWindow : Window
{
    private readonly ImportService _importService = new();
    private readonly MainWindowViewModel _viewModel = new();
    private List<string> _lastScanFiles = new();
    private CancellationTokenSource? _cts;
    private readonly AppSettings _settings;
    private bool _isImporting;
    private bool _scanComplete; // Flag to prevent progress updates after scan completion

    public MainWindow() : this(AppSettings.Load())
    {
    }

    public MainWindow(AppSettings settings)
    {
        _settings = settings;
        DataContext = _viewModel;
        InitializeComponent();
        Closing += Window_Closing;

        Language.CurrentLanguage = _settings.CurrentLanguage;

        ApplyThemeMode(_settings.ThemeMode);
        UpdateLanguageTexts();
        LoadSettingsToUI();
        SyncThemeStateFromActualVariant();

        Application.Current!.ActualThemeVariantChanged += (_, _) =>
        {
            SyncThemeStateFromActualVariant();
        };

        SetStatusIdle();
    }

    private void SetImportRunning(bool running)
    {
        if (running)
        {
            StartButton.Content = Language.T("button.cancel");
            StartButton.Classes.Remove("primary");
            if (!StartButton.Classes.Contains("danger"))
            {
                StartButton.Classes.Add("danger");
            }
            ScanButton.IsEnabled = false;
        }
        else
        {
            StartButton.Content = Language.T("button.import");
            StartButton.Classes.Remove("danger");
            if (!StartButton.Classes.Contains("primary"))
            {
                StartButton.Classes.Add("primary");
            }
            ScanButton.IsEnabled = true;
        }
    }

    private void SetStatusIdle()
    {
        StatusLeftText.Text = Language.T("status.ready");
        StatusPercent.Text = "0%";
        StatusCount.Text = $"0 {Language.T("unit.files")}";
        StatusSpeed.Text = (0d).ToString("N2", CultureInfo.CurrentCulture) + " MB";
        ProgressBar.Value = 0;
    }

    private void UpdateLanguageTexts()
    {
        Title = Language.T("app.title");

        MenuFile.Header = Language.T("menu.file");
        MenuExit.Header = Language.T("menu.exit");
        MenuLanguage.Header = Language.T("menu.language");
        MenuGerman.Header = Language.T("menu.german");
        MenuEnglish.Header = Language.T("menu.english");
        MenuTheme.Header = Language.T("menu.theme");
        MenuSystem.Header = Language.T("menu.system");
        MenuLight.Header = Language.T("menu.light");
        MenuDark.Header = Language.T("menu.dark");
        MenuCredits.Header = Language.T("menu.credits");
        MenuInstagram.Header = Language.T("menu.instagram");

        SourceLabel.Text = Language.T("label.source");
        TargetLabel.Text = Language.T("label.target");
        FiletypesHeader.Text = Language.T("label.filetypes");
        AdditionalExtensionsLabel.Text = Language.T("label.extensions.enter");
        FolderStructureLabel.Text = Language.T("label.folderstructure");
        CheckSkipDuplicates.Content = Language.T("checkbox.skipdup");
        CheckPreserveSubfolders.Content = Language.T("checkbox.preserve");
        CheckCreateSubfoldersPerFileType.Content = Language.T("checkbox.filetypeSubfolders");
        CheckMultithread.Content = Language.T("checkbox.multithread");
        CheckAllFiles.Content = Language.T("label.allfiles");

        BrowseSourceButton.Content = Language.T("button.browseSource");
        BrowseTargetButton.Content = Language.T("button.browseTarget");
        ScanButton.Content = Language.T("button.scan");

        // Always update StartButton content based on current import state
        if (!_isImporting)
        {
            StartButton.Content = Language.T("button.import");
        }
        else
        {
            StartButton.Content = Language.T("button.cancel");
        }

        // Localize folder structure combo items by index (no layout change)
        if (FolderStructureCombo.ItemCount >= 5)
        {
            (FolderStructureCombo.Items[0] as ComboBoxItem)!.Content = Language.T("folder.ymd");
            (FolderStructureCombo.Items[1] as ComboBoxItem)!.Content = Language.T("folder.ym");
            (FolderStructureCombo.Items[2] as ComboBoxItem)!.Content = Language.T("folder.y");
            (FolderStructureCombo.Items[3] as ComboBoxItem)!.Content = Language.T("folder.dmy");
            (FolderStructureCombo.Items[4] as ComboBoxItem)!.Content = Language.T("folder.flat");
        }

        // Update status fields when language changes - translate known status messages
        var knownStatus = new (string key, string de, string en)[]
        {
            ("status.ready", "Bereit", "Ready"),
            ("status.scanning", "Scanne…", "Scanning…"),
            ("status.scanComplete", "Scan abgeschlossen", "Scan Complete"),
            ("status.importing", "Import läuft...", "Import in progress..."),
            ("status.done", "Fertig.", "Done."),
            ("error.scancancelled", "Scan abgebrochen.", "Scan cancelled."),
            ("error.scantitle", "Fehler beim Scan", "Scan error"),
            ("error.importcancelled", "Import abgebrochen.", "Import cancelled."),
            ("error.importdone", "Fehler beim Import.", "Error during import."),
        };

        foreach (var status in knownStatus)
        {
            if (StatusLeftText.Text == status.de || StatusLeftText.Text == status.en)
            {
                StatusLeftText.Text = Language.T(status.key);
                break;
            }
        }

        // Update status count display to show current language unit suffix
        var text = StatusCount.Text ?? string.Empty;
        if (text.Contains("/"))
        {
            var parts = text.Split('/');
            if (parts.Length == 2)
            {
                var left = parts[0].Trim();
                var right = parts[1].Trim();
                StatusCount.Text = $"{left} / {right}";
            }
        }
        else if (_lastScanFiles != null && _lastScanFiles.Count > 0)
        {
            StatusCount.Text = $"{_lastScanFiles.Count} {Language.T("unit.files")}";
        }
        else
        {
            StatusCount.Text = $"0 {Language.T("unit.files")}";
        }

        UpdateThemeMenuHeaders();
    }

    private void ApplyThemeMode(ThemeMode mode)
    {
        ThemeService.ApplyThemeMode(mode);
        _settings.ThemeMode = mode;
        _settings.Save();
        UpdateThemeMenuHeaders();
    }

    private void SyncThemeStateFromActualVariant()
    {
        UpdateThemeMenuHeaders();
    }

    private static bool TryGetResource<T>(IResourceDictionary resources, string key, out T? value)
    {
        if (resources is IResourceProvider provider &&
            provider.TryGetResource(key, ThemeVariant.Default, out var obj) &&
            obj is T typed)
        {
            value = typed;
            return true;
        }

        value = default;
        return false;
    }

    private IBrush GetBrushOrDefault(string key, IBrush fallback)
    {
        var resources = Application.Current?.Resources;
        if (resources != null && TryGetResource(resources, key, out IBrush? brush) && brush is not null)
        {
            return brush;
        }

        return fallback;
    }

    private void ThemeSystem_Click(object? sender, RoutedEventArgs e) => ApplyThemeMode(ThemeMode.System);
    private void ThemeLight_Click(object? sender, RoutedEventArgs e) => ApplyThemeMode(ThemeMode.Light);
    private void ThemeDark_Click(object? sender, RoutedEventArgs e) => ApplyThemeMode(ThemeMode.Dark);

    private void UpdateThemeMenuHeaders()
    {
        var system = Language.T("menu.system");
        var light = Language.T("menu.light");
        var dark = Language.T("menu.dark");
        var activeSuffix = Language.CurrentLanguage == "de" ? " (aktiv)" : " (active)";
        MenuSystem.Header = _settings.ThemeMode == ThemeMode.System ? $"{system}{activeSuffix}" : system;
        MenuLight.Header = _settings.ThemeMode == ThemeMode.Light ? $"{light}{activeSuffix}" : light;
        MenuDark.Header = _settings.ThemeMode == ThemeMode.Dark ? $"{dark}{activeSuffix}" : dark;
    }

    private async void BrowseSource_Click(object? sender, RoutedEventArgs e)
    {
        var result = await StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            AllowMultiple = false,
            SuggestedStartLocation = null
        });

        if (result != null && result.Count > 0)
        {
            SourceTextBox.Text = result[0].Path.LocalPath;
        }
    }

    private async void BrowseTarget_Click(object? sender, RoutedEventArgs e)
    {
        var result = await StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            AllowMultiple = false,
            SuggestedStartLocation = null
        });

        if (result != null && result.Count > 0)
        {
            TargetTextBox.Text = result[0].Path.LocalPath;
        }
    }

    private ImportSettings BuildSettingsFromUI()
    {
        var manualExts = new List<string>();

        foreach (var child in ExtensionsPanel.Children)
        {
            if (child is Button btn && btn.Content is string text)
            {
                text = text.Trim();
                if (text.EndsWith(" ✕", StringComparison.Ordinal))
                {
                    text = text[..^2].Trim();
                }

                if (!string.IsNullOrWhiteSpace(text))
                {
                    manualExts.Add(text);
                }
            }
        }

        return new ImportSettings
        {
            SourceFolder = SourceTextBox.Text?.Trim() ?? string.Empty,
            TargetFolder = TargetTextBox.Text?.Trim() ?? string.Empty,
            IncludeRaw = CheckRaw.IsChecked == true,
            IncludeCr2 = CheckCr2.IsChecked == true,
            IncludeCr3 = CheckCr3.IsChecked == true,
            IncludeNef = CheckNef.IsChecked == true,
            IncludeRaf = CheckRaf.IsChecked == true,
            IncludeOrf = CheckOrf.IsChecked == true,
            IncludeRw2 = CheckRw2.IsChecked == true,
            IncludeDng = CheckDng.IsChecked == true,
            // Separate JPG and JPEG switches to avoid misleading UI
            IncludeJpg = CheckJpg.IsChecked == true,
            IncludeJpeg = CheckJpeg.IsChecked == true,
            IncludePng = CheckPng.IsChecked == true,
            IncludeTiff = CheckTiff.IsChecked == true,
            IncludeHeic = CheckHeic.IsChecked == true,
            IncludeAllFiles = CheckAllFiles.IsChecked == true,
            SkipDuplicates = CheckSkipDuplicates.IsChecked == true,
            PreserveSubfolders = CheckPreserveSubfolders.IsChecked == true,
            CreateSubfoldersPerFileType = _viewModel.CreateSubfoldersPerFileType,
            UseMultithread = CheckMultithread.IsChecked == true,
            FolderStructure = (FolderStructureCombo.SelectedItem as ComboBoxItem)?.Content?.ToString() ?? "Year/Month/Day",
            ManualExtensions = manualExts
        };
    }

    private async void ScanButton_Click(object? sender, RoutedEventArgs e)
    {
        var tmpSettings = BuildSettingsFromUI();
        if (!tmpSettings.IsValid(out var error))
        {
            await DialogService.ShowWarningAsync(error, Language.T("error.title"));
            return;
        }

        _cts?.Cancel();
        _cts = new CancellationTokenSource();
        _scanComplete = false; // Reset flag at scan start

        StatusLeftText.Text = Language.T("status.scanning");
        StatusPercent.Text = "0%";
        StatusCount.Text = "0 / 0";
        StatusSpeed.Text = "—";
        ProgressBar.Value = 0;

        var progress = new Progress<(int current, int total, string file)>(p =>
        {
            // Prevent progress updates after scan completion
            if (_scanComplete) return;

            var (current, total, file) = p;
            Dispatcher.UIThread.Post(() =>
            {
                // Double-check flag in dispatcher to avoid race condition
                if (_scanComplete) return;

                var percent = total > 0 ? Math.Clamp((current * 100) / total, 0, 100) : 0;
                StatusLeftText.Text = Path.GetFileName(file ?? string.Empty);
                StatusPercent.Text = $"{percent}%";
                StatusCount.Text = $"{current} / {total}";
                StatusSpeed.Text = "—";
                ProgressBar.Value = percent;
            });
        });

        try
        {
            _lastScanFiles = await _importService.ScanAsync(
                tmpSettings,
                progress,
                _cts.Token);

            // Calculate total size after scan completes
            var totalBytes = 0L;
            foreach (var file in _lastScanFiles)
            {
                try
                {
                    totalBytes += new FileInfo(file).Length;
                }
                catch
                {
                    // File not accessible, ignore
                }
            }
            
            var totalMb = totalBytes / (1024.0 * 1024.0);
            
            // Mark scan as complete to prevent progress updates from overwriting status
            _scanComplete = true;
            StatusLeftText.Text = Language.T("status.scanComplete");
            StatusPercent.Text = "100%";
            StatusCount.Text = $"{_lastScanFiles.Count} {Language.T("unit.files")}";
            StatusSpeed.Text = totalMb.ToString("N2", CultureInfo.CurrentCulture) + " MB";
            ProgressBar.Value = 100;
        }
        catch (OperationCanceledException)
        {
            _scanComplete = true;
            StatusLeftText.Text = Language.T("error.scancancelled");
        }
        catch (Exception ex)
        {
            _scanComplete = true;
            StatusLeftText.Text = Language.T("error.scantitle");
            await DialogService.ShowErrorAsync(ex.Message, Language.T("error.title"));
        }
    }

    private async void StartButton_Click(object? sender, RoutedEventArgs e)
    {
        if (_isImporting)
        {
            _cts?.Cancel();
            return;
        }

        var tmpSettings = BuildSettingsFromUI();
        if (!tmpSettings.IsValid(out var error))
        {
            await DialogService.ShowWarningAsync(error, Language.T("error.title"));
            return;
        }

        if (_lastScanFiles.Count == 0)
        {
            var result = await DialogService.ShowYesNoAsync(
                Language.T("error.noscan"),
                Language.T("info.title"));

            if (result != DialogResult.Yes)
                return;

            _cts?.Cancel();
            _cts = new CancellationTokenSource();
            _scanComplete = false; // Reset flag at scan start

            StatusLeftText.Text = Language.T("status.scanning");
            StatusPercent.Text = "0%";
            StatusCount.Text = "0 / 0";
            StatusSpeed.Text = "—";
            ProgressBar.Value = 0;

            var scanProgress = new Progress<(int current, int total, string file)>(p =>
            {
                // Prevent progress updates after scan completion
                if (_scanComplete) return;

                var (current, total, file) = p;
                Dispatcher.UIThread.Post(() =>
                {
                    // Double-check flag in dispatcher to avoid race condition
                    if (_scanComplete) return;

                    var percent = total > 0 ? Math.Clamp((current * 100) / total, 0, 100) : 0;
                    StatusLeftText.Text = Path.GetFileName(file ?? string.Empty);
                    StatusPercent.Text = $"{percent}%";
                    StatusCount.Text = $"{current} / {total}";
                    StatusSpeed.Text = "—";
                    ProgressBar.Value = percent;
                });
            });

            try
            {
                _lastScanFiles = await _importService.ScanAsync(
                    tmpSettings,
                    scanProgress,
                    _cts.Token);
                
                // Calculate total size after scan completes
                var totalBytes = 0L;
                foreach (var file in _lastScanFiles)
                {
                    try
                    {
                        totalBytes += new FileInfo(file).Length;
                    }
                    catch
                    {
                        // File not accessible, ignore
                    }
                }
                
                var totalMb = totalBytes / (1024.0 * 1024.0);
                
                // Mark scan as complete to prevent progress updates from overwriting status
                _scanComplete = true;
                StatusLeftText.Text = Language.T("status.scanComplete");
                StatusPercent.Text = "100%";
                StatusCount.Text = $"{_lastScanFiles.Count} {Language.T("unit.files")}";
                StatusSpeed.Text = totalMb.ToString("N2", CultureInfo.CurrentCulture) + " MB";
                ProgressBar.Value = 100;
            }
            catch (Exception ex)
            {
                _scanComplete = true;
                await DialogService.ShowErrorAsync(ex.Message, Language.T("error.scantitle"));
                return;
            }
        }

        _cts?.Cancel();
        _cts = new CancellationTokenSource();

        SetImportRunning(true);

        StatusLeftText.Text = Language.T("status.importing");
        ProgressBar.Value = 0;

        var importProgress = new Progress<(int current, int total, string file, string speed)>(p =>
        {
            var (current, tot, file, speed) = p;

            speed = string.IsNullOrWhiteSpace(speed) ? "0 MB/s" : speed;
            if (tot < 0) tot = 0;
            if (current < 0) current = 0;

            var percent = tot > 0 ? Math.Clamp((current * 100) / tot, 0, 100) : 0;

            Dispatcher.UIThread.Post(() =>
            {
                StatusLeftText.Text = Path.GetFileName(file ?? string.Empty);
                StatusPercent.Text = $"{percent}%";
                StatusCount.Text = $"{current} / {tot}";
                StatusSpeed.Text = speed;
                ProgressBar.Value = percent;
            });
        });

        try
        {
            _isImporting = true;
            await _importService.ImportAsync(
                tmpSettings,
                _lastScanFiles,
                importProgress,
                _cts.Token);

            StatusLeftText.Text = Language.T("status.done");
        }
        catch (OperationCanceledException)
        {
            StatusLeftText.Text = Language.T("error.importcancelled");
        }
        catch (Exception ex)
        {
            StatusLeftText.Text = Language.T("error.importdone");
            await DialogService.ShowErrorAsync(ex.Message, Language.T("error.title"));
        }
        finally
        {
            _isImporting = false;
            _cts = null;
            SetImportRunning(false);
        }
    }

    private void ExitMenu_Click(object? sender, RoutedEventArgs e)
    {
        Close();
    }

    private void LangDe_Click(object? sender, RoutedEventArgs e)
    {
        Language.CurrentLanguage = "de";
        _settings.CurrentLanguage = "de";
        _settings.Save();
        UpdateLanguageTexts();
    }

    private void LangEn_Click(object? sender, RoutedEventArgs e)
    {
        Language.CurrentLanguage = "en";
        _settings.CurrentLanguage = "en";
        _settings.Save();
        UpdateLanguageTexts();
    }

    private async void MenuInstagram_Click(object? sender, RoutedEventArgs e)
    {
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = "https://www.instagram.com/thelomphotography/",
                UseShellExecute = true
            });
        }
        catch (Exception ex)
        {
            await DialogService.ShowWarningAsync(
                $"Error opening link: {ex.Message}",
                Language.T("error.title"));
        }
    }

    private void ManualExtTextBox_KeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            e.Handled = true;

            var text = ManualExtTextBox.Text?.Trim() ?? string.Empty;
            if (string.IsNullOrWhiteSpace(text))
                return;

            if (!text.StartsWith("."))
                text = "." + text;

            var exists = false;
            foreach (var child in ExtensionsPanel.Children)
            {
                if (child is Button btn && btn.Content is string content &&
                    content.StartsWith(text, StringComparison.OrdinalIgnoreCase))
                {
                    exists = true;
                    break;
                }
            }

            if (!exists)
            {
                CreateExtensionTag(text);
            }

            ManualExtTextBox.Clear();
            ManualExtTextBox.Focus();
        }
    }

    private void CreateExtensionTag(string extension)
    {
        var button = new Button
        {
            Content = extension + " ✕",
            Padding = new Thickness(8, 4, 8, 4),
            Margin = new Thickness(3),
            BorderThickness = new Thickness(1),
            VerticalAlignment = VerticalAlignment.Center
        };

        button.Classes.Add("ext-tag");

        button.Click += (_, _) =>
        {
            ExtensionsPanel.Children.Remove(button);
        };

        ExtensionsPanel.Children.Add(button);
    }

    private void LoadSettingsToUI()
    {
        SourceTextBox.Text = _settings.SourceFolder;
        TargetTextBox.Text = _settings.TargetFolder;

        CheckRaw.IsChecked = _settings.IncludeRaw;
        CheckCr2.IsChecked = _settings.IncludeCr2;
        CheckCr3.IsChecked = _settings.IncludeCr3;
        CheckNef.IsChecked = _settings.IncludeNef;
        CheckRaf.IsChecked = _settings.IncludeRaf;
        CheckOrf.IsChecked = _settings.IncludeOrf;
        CheckRw2.IsChecked = _settings.IncludeRw2;
        CheckDng.IsChecked = _settings.IncludeDng;
        CheckJpg.IsChecked = _settings.IncludeJpg;
        CheckJpeg.IsChecked = _settings.IncludeJpeg;
        CheckPng.IsChecked = _settings.IncludePng;
        CheckTiff.IsChecked = _settings.IncludeTiff;
        CheckHeic.IsChecked = _settings.IncludeHeic;
        CheckAllFiles.IsChecked = _settings.IncludeAllFiles;

        CheckSkipDuplicates.IsChecked = _settings.SkipDuplicates;
        CheckPreserveSubfolders.IsChecked = _settings.PreserveSubfolders;
        _viewModel.CreateSubfoldersPerFileType = _settings.CreateSubfoldersPerFileType;
        CheckMultithread.IsChecked = _settings.UseMultithread;

        FolderStructureCombo.SelectedIndex = GetFolderStructureIndex(_settings.FolderStructure);

        foreach (var ext in _settings.ManualExtensions)
        {
            CreateExtensionTag(ext);
        }
    }

    private void SaveSettingsFromUI()
    {
        var settings = BuildSettingsFromUI();

        _settings.SourceFolder = settings.SourceFolder;
        _settings.TargetFolder = settings.TargetFolder;

        _settings.IncludeRaw = settings.IncludeRaw;
        _settings.IncludeCr2 = settings.IncludeCr2;
        _settings.IncludeCr3 = settings.IncludeCr3;
        _settings.IncludeNef = settings.IncludeNef;
        _settings.IncludeRaf = settings.IncludeRaf;
        _settings.IncludeOrf = settings.IncludeOrf;
        _settings.IncludeRw2 = settings.IncludeRw2;
        _settings.IncludeDng = settings.IncludeDng;
        _settings.IncludeJpg = settings.IncludeJpg;
        _settings.IncludeJpeg = settings.IncludeJpeg;
        _settings.IncludePng = settings.IncludePng;
        _settings.IncludeTiff = settings.IncludeTiff;
        _settings.IncludeHeic = settings.IncludeHeic;
        _settings.IncludeAllFiles = settings.IncludeAllFiles;

        _settings.SkipDuplicates = settings.SkipDuplicates;
        _settings.PreserveSubfolders = settings.PreserveSubfolders;
        _settings.CreateSubfoldersPerFileType = settings.CreateSubfoldersPerFileType;
        _settings.UseMultithread = settings.UseMultithread;

        _settings.FolderStructure = settings.FolderStructure;
        _settings.CurrentLanguage = Language.CurrentLanguage;
        _settings.ManualExtensions = settings.ManualExtensions;
        _settings.ThemeMode = ThemeService.CurrentMode;

        _settings.Save();
    }

    private int GetFolderStructureIndex(string structure)
    {
        return structure switch
        {
            "Year/Month/Day" => 0,
            "Year/Month" => 1,
            "Year" => 2,
            "Day-Month-Year" => 3,
            "Flat" => 4,
            _ => 0
        };
    }

    private void Window_Closing(object? sender, CancelEventArgs e)
    {
        SaveSettingsFromUI();
    }
}
