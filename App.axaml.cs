using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;

namespace RawImporterCS;

public partial class App : Application
{
    private AppSettings? _settings;

    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
    }

    public override void OnFrameworkInitializationCompleted()
    {
        ThemeResources.EnsureLoaded();
        _settings = AppSettings.Load();
        ThemeService.ApplyThemeMode(_settings.ThemeMode);

        Application.Current!.ActualThemeVariantChanged += (_, _) =>
        {
            ThemeService.ReapplySystemThemeIfNeeded();
        };

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            desktop.MainWindow = new MainWindow(_settings);
        }

        base.OnFrameworkInitializationCompleted();
    }
}
