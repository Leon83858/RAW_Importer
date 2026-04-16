using System;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Layout;
using Avalonia.Media;

namespace RawImporterCS;

public enum DialogResult
{
    Ok,
    Yes,
    No
}

public enum DialogButtons
{
    Ok,
    YesNo
}

/// <summary>
/// Einfache modale Dialoge ohne externe Abhängigkeiten.
/// </summary>
public static class DialogService
{
    public static Task ShowErrorAsync(string message, string title = "Error") =>
        ShowMessageAsync(title, message, DialogButtons.Ok);

    public static Task ShowWarningAsync(string message, string title = "Warning") =>
        ShowMessageAsync(title, message, DialogButtons.Ok);

    public static Task<DialogResult> ShowYesNoAsync(string message, string title = "Confirm") =>
        ShowMessageAsync(title, message, DialogButtons.YesNo);

    public static async Task<DialogResult> ShowMessageAsync(string title, string message, DialogButtons buttons)
    {
        var owner = GetOwnerWindow();
        if (owner == null)
        {
            Console.WriteLine($"{title}: {message}");
            return buttons == DialogButtons.YesNo ? DialogResult.No : DialogResult.Ok;
        }

        var dialog = BuildDialogWindow(title, message, buttons);
        return await dialog.ShowDialog<DialogResult>(owner);
    }

    private static Window? GetOwnerWindow()
    {
        if (Application.Current?.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            return desktop.MainWindow;
        }

        return null;
    }

    private static Window BuildDialogWindow(string title, string message, DialogButtons buttons)
    {
        var okText = buttons == DialogButtons.YesNo ? "Yes" : "OK";
        var cancelText = buttons == DialogButtons.YesNo ? "No" : "Cancel";

        var dialog = new Window
        {
            Title = title,
            CanResize = false,
            SizeToContent = SizeToContent.WidthAndHeight,
            WindowStartupLocation = WindowStartupLocation.CenterOwner,
            Padding = new Thickness(16),
            MaxWidth = 560
        };

        var messageBlock = new TextBlock
        {
            Text = message,
            TextWrapping = TextWrapping.Wrap,
            MaxWidth = 520
        };

        var buttonsPanel = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Right,
            Spacing = 8
        };

        if (buttons == DialogButtons.YesNo)
        {
            var noButton = new Button { Content = cancelText, MinWidth = 80 };
            noButton.Click += (_, _) => dialog.Close(DialogResult.No);
            buttonsPanel.Children.Add(noButton);
        }

        var okButton = new Button { Content = okText, MinWidth = 80 };
        okButton.Click += (_, _) =>
        {
            var result = buttons == DialogButtons.YesNo ? DialogResult.Yes : DialogResult.Ok;
            dialog.Close(result);
        };

        buttonsPanel.Children.Add(okButton);

        var layout = new StackPanel
        {
            Spacing = 12
        };

        layout.Children.Add(messageBlock);
        layout.Children.Add(buttonsPanel);

        dialog.Content = layout;
        return dialog;
    }
}
