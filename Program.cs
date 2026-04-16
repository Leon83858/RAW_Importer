using System;
using System.IO;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Layout;
using Avalonia.Native;
using Avalonia.Media;
using Avalonia.Threading;

namespace RawImporterCS;

internal static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        RegisterGlobalExceptionHandlers();

        BuildAvaloniaApp()
            .StartWithClassicDesktopLifetime(args);
    }

    public static AppBuilder BuildAvaloniaApp()
    {
        var builder = AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();

        if (OperatingSystem.IsMacOS())
        {
            builder = builder.With(new AvaloniaNativePlatformOptions
            {
                RenderingMode = new[] { AvaloniaNativeRenderingMode.Software }
            });
        }

        return builder;
    }

    private static void RegisterGlobalExceptionHandlers()
    {
        AppDomain.CurrentDomain.UnhandledException += (_, args) =>
        {
            if (args.ExceptionObject is Exception ex)
            {
                var logPath = LogCrash("AppDomain.UnhandledException", ex);
                ShowCrashNotice(logPath, ex);
            }
        };

        TaskScheduler.UnobservedTaskException += (_, args) =>
        {
            var logPath = LogCrash("TaskScheduler.UnobservedTaskException", args.Exception);
            args.SetObserved();
            ShowCrashNotice(logPath, args.Exception);
        };
    }

    private static string LogCrash(string source, Exception exception)
    {
        var logPath = GetCrashLogPath();

        try
        {
            var directory = Path.GetDirectoryName(logPath);
            if (!string.IsNullOrEmpty(directory))
            {
                Directory.CreateDirectory(directory);
            }

            using var writer = new StreamWriter(logPath, append: true);
            writer.WriteLine($"[{DateTimeOffset.Now:u}] {source}");
            WriteException(writer, exception, 0);
            writer.WriteLine(new string('-', 60));
        }
        catch
        {
            // ignore logging errors to keep crash path consistent
        }

        return logPath;
    }

    private static void WriteException(TextWriter writer, Exception exception, int depth)
    {
        var indent = new string(' ', depth * 2);
        writer.WriteLine($"{indent}{exception.GetType().FullName}: {exception.Message}");
        if (!string.IsNullOrWhiteSpace(exception.StackTrace))
        {
            writer.WriteLine($"{indent}{exception.StackTrace}");
        }

        if (exception is AggregateException aggregate)
        {
            foreach (var inner in aggregate.InnerExceptions)
            {
                WriteException(writer, inner, depth + 1);
            }
        }

        if (exception.InnerException != null)
        {
            WriteException(writer, exception.InnerException, depth + 1);
        }
    }

    private static string GetCrashLogPath()
    {
        var baseDirectory = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        if (string.IsNullOrWhiteSpace(baseDirectory))
        {
            baseDirectory = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        }

        if (string.IsNullOrWhiteSpace(baseDirectory))
        {
            baseDirectory = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".rawimportercs");
        }

        var directory = Path.Combine(baseDirectory, "RawImporterCS");
        return Path.Combine(directory, "crash.log");
    }

    private static void ShowCrashNotice(string logPath, Exception exception)
    {
        try
        {
            Console.Error.WriteLine($"Fatal error: {exception}");
            Console.Error.WriteLine($"Crash log: {logPath}");

            if (Application.Current?.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime { MainWindow: not null })
            {
                Dispatcher.UIThread.Post(() =>
                {
                    var notice = new Window
                    {
                        Title = "RAW Importer abgestürzt",
                        Width = 420,
                        Height = 200,
                        CanResize = false,
                        WindowStartupLocation = WindowStartupLocation.CenterOwner,
                        Content = new TextBlock
                        {
                            Margin = new Thickness(16),
                            TextWrapping = TextWrapping.Wrap,
                            Text = $"App ist abgestürzt. Siehe crash.log:\n{logPath}"
                        }
                    };

                    notice.Show();
                });
            }
        }
        catch
        {
            // Keep shutdown path resilient
        }
    }
}
