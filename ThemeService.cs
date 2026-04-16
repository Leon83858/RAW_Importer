using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Styling;

namespace RawImporterCS
{
    public static class ThemeService
    {
        private const double HoverAlpha = 0.08;
        private const double PressedAlpha = 0.14;
        private const double BorderSoftAlpha = 0.55;
        private const double FocusRingAlpha = 0.60;

        private static ThemeMode _currentMode = ThemeMode.System;

        public static ThemeMode CurrentMode => _currentMode;

        public static void ApplyThemeMode(ThemeMode mode)
        {
            var app = Application.Current ?? throw new InvalidOperationException("Application not available.");
            ThemeResources.EnsureLoaded();

            _currentMode = mode;
            app.RequestedThemeVariant = mode switch
            {
                ThemeMode.Light => ThemeVariant.Light,
                ThemeMode.Dark => ThemeVariant.Dark,
                _ => ThemeVariant.Default
            };
            // Use requested mode to decide which palette to apply to avoid timing mismatches
            var applyDark = mode switch
            {
                ThemeMode.Dark => true,
                ThemeMode.Light => false,
                _ => IsDark(app.ActualThemeVariant) // System follows current actual variant
            };

            SetActiveThemeResources(applyDark);
        }

        public static void ReapplySystemThemeIfNeeded()
        {
            if (_currentMode == ThemeMode.System)
            {
                var app = Application.Current ?? throw new InvalidOperationException("Application not available.");
                SetActiveThemeResources(IsDark(app.ActualThemeVariant));
            }
        }

        public static void SetActiveThemeResources(bool isDark)
        {
            var resources = Application.Current?.Resources ?? throw new InvalidOperationException("Application resources not available.");

            Color Core(string key) => RequireColor(resources, $"Color{key}{(isDark ? "Dark" : "Light")}");
            bool TryCore(string key, out Color color) => TryGetColor(resources, $"Color{key}{(isDark ? "Dark" : "Light")}", out color);

            var bg = Core("Bg");
            var surface = Core("Surface");
            var control = Core("Control");
            var border = Core("Border");
            var text = Core("Text");
            var textMuted = Core("TextMuted");
            var accent = Core("Accent");
            var danger = Core("Danger");
            var success = TryCore("Success", out var successColor) ? successColor : (Color?)null;
            var warning = TryCore("Warning", out var warningColor) ? warningColor : (Color?)null;

            var controlHover = Blend(control, accent, HoverAlpha);
            var controlPressed = Blend(control, accent, PressedAlpha);
            var borderSoft = WithAlpha(border, BorderSoftAlpha);
            var focusRing = WithAlpha(accent, FocusRingAlpha);
            var accentHover = Blend(accent, text, HoverAlpha);
            var accentPressed = Blend(accent, text, PressedAlpha);
            var dangerHover = Blend(danger, text, HoverAlpha);
            var dangerPressed = Blend(danger, text, PressedAlpha);

            var onAccent = ChooseOnColor(accent, bg, text, Colors.Black);
            var onDanger = ChooseOnColor(danger, bg, text, Colors.Black);

            SetColor(resources, "Color.Bg", bg);
            SetColor(resources, "Color.Surface", surface);
            SetColor(resources, "Color.Control", control);
            SetColor(resources, "Color.Border", border);
            SetColor(resources, "Color.Text", text);
            SetColor(resources, "Color.TextMuted", textMuted);
            SetColor(resources, "Color.Accent", accent);
            SetColor(resources, "Color.Danger", danger);
            SetColor(resources, "Color.FocusRing", focusRing);
            SetColor(resources, "Color.AccentHover", accentHover);
            SetColor(resources, "Color.AccentPressed", accentPressed);
            SetColor(resources, "Color.DangerHover", dangerHover);
            SetColor(resources, "Color.DangerPressed", dangerPressed);

            if (success.HasValue)
            {
                SetColor(resources, "Color.Success", success.Value);
            }

            if (warning.HasValue)
            {
                SetColor(resources, "Color.Warning", warning.Value);
            }

            SetBrush(resources, "Brush.Bg", Brush(bg));
            SetBrush(resources, "Brush.Surface", Brush(surface));
            // Ensure card surfaces render with correct contrast; link Card to Surface by default
            SetBrush(resources, "Brush.Card", Brush(surface));
            SetBrush(resources, "Brush.Control", Brush(control));
            // Optional input brush to align input elements with control background
            SetBrush(resources, "Brush.Input", Brush(control));
            SetBrush(resources, "Brush.Border", Brush(border));
            SetBrush(resources, "Brush.Text", Brush(text));
            SetBrush(resources, "Brush.TextMuted", Brush(textMuted));
            SetBrush(resources, "Brush.Accent", Brush(accent));
            SetBrush(resources, "Brush.Danger", Brush(danger));
            SetBrush(resources, "Brush.AccentHover", Brush(accentHover));
            SetBrush(resources, "Brush.AccentPressed", Brush(accentPressed));
            SetBrush(resources, "Brush.DangerHover", Brush(dangerHover));
            SetBrush(resources, "Brush.DangerPressed", Brush(dangerPressed));
            SetBrush(resources, "Brush.OnAccent", Brush(onAccent));
            SetBrush(resources, "Brush.OnDanger", Brush(onDanger));
            SetBrush(resources, "Brush.ControlHover", Brush(controlHover));
            SetBrush(resources, "Brush.ControlPressed", Brush(controlPressed));
            SetBrush(resources, "Brush.BorderSoft", Brush(borderSoft));
            SetBrush(resources, "Brush.FocusRing", Brush(focusRing));
            SetBrush(resources, "Brush.Selection", Brush(Blend(control, accent, 0.25)));
            SetBrush(resources, "Brush.SelectionForeground", Brush(text));

            if (success.HasValue)
            {
                SetBrush(resources, "Brush.Success", Brush(success.Value));
            }

            if (warning.HasValue)
            {
                SetBrush(resources, "Brush.Warning", Brush(warning.Value));
            }

            LogContrast("Accent", accent, onAccent);
            LogContrast("Danger", danger, onDanger);
        }

        public static Color ParseHex(string hex) => Color.Parse(hex);

        public static Color WithAlpha(Color color, double alpha)
        {
            var a = (byte)Math.Clamp(Math.Round(alpha * 255), 0, 255);
            return Color.FromArgb(a, color.R, color.G, color.B);
        }

        public static Color Blend(Color baseColor, Color overlay, double overlayAlpha)
        {
            var alpha = Math.Clamp(overlayAlpha, 0, 1);

            byte BlendChannel(byte b, byte o) =>
                (byte)Math.Clamp(Math.Round(b * (1 - alpha) + o * alpha), 0, 255);

            return Color.FromArgb(255,
                BlendChannel(baseColor.R, overlay.R),
                BlendChannel(baseColor.G, overlay.G),
                BlendChannel(baseColor.B, overlay.B));
        }

        public static SolidColorBrush Brush(Color color) => new(color) { Opacity = 1 };

        private static Color ChooseOnColor(Color backgroundColor, params Color[] candidates)
        {
            var bestColor = Colors.White;
            var bestContrast = ContrastRatio(backgroundColor, bestColor);

            foreach (var candidate in candidates)
            {
                var contrast = ContrastRatio(backgroundColor, candidate);
                if (contrast > bestContrast)
                {
                    bestContrast = contrast;
                    bestColor = candidate;
                }
            }

            return bestColor;
        }

        private static double ContrastRatio(Color a, Color b)
        {
            var l1 = RelativeLuminance(a);
            var l2 = RelativeLuminance(b);
            var (lighter, darker) = l1 > l2 ? (l1, l2) : (l2, l1);
            return (lighter + 0.05) / (darker + 0.05);
        }

        private static double RelativeLuminance(Color color)
        {
            static double LinearChannel(double channel) =>
                channel <= 0.03928 ? channel / 12.92 : Math.Pow((channel + 0.055) / 1.055, 2.4);

            var r = LinearChannel(color.R / 255.0);
            var g = LinearChannel(color.G / 255.0);
            var b = LinearChannel(color.B / 255.0);
            return 0.2126 * r + 0.7152 * g + 0.0722 * b;
        }

        private static void SetColor(IResourceDictionary resources, string key, Color color) => resources[key] = color;

        private static void SetBrush(IResourceDictionary resources, string key, IBrush brush) => resources[key] = brush;

        private static Color RequireColor(IResourceDictionary resources, string key)
        {
            if (TryGetColor(resources, key, out var color))
            {
                return color;
            }

            throw new InvalidOperationException($"Missing color resource '{key}'.");
        }

        private static bool TryGetColor(IResourceDictionary resources, string key, out Color color)
        {
            if (resources is IResourceProvider provider &&
                provider.TryGetResource(key, ThemeVariant.Default, out var resource) &&
                resource is Color parsed)
            {
                color = parsed;
                return true;
            }

            color = default;
            return false;
        }

        private static bool IsDark(ThemeVariant variant) =>
            variant == ThemeVariant.Dark;

        private static void LogContrast(string name, Color background, Color foreground)
        {
            var contrast = ContrastRatio(background, foreground);
            var level = contrast < 4.5 ? "LOW" : "OK";
            Console.WriteLine($"[Theme] {name} contrast: {contrast:F2} ({level}) (fg: {foreground})");
        }
    }
}
