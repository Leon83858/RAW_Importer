// Generated bridge to expose Colors.axaml resources at compile time
#nullable enable
using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Avalonia.Markup.Xaml.Styling;
using Avalonia.Styling;

namespace RawImporterCS
{
    public static class ThemeResources
    {
        public static void EnsureLoaded()
        {
            var app = Application.Current;
            if (app == null)
                return;

            var resources = app.Resources;
            if (HasPalette(resources))
            {
                resources["ThemeColorsLoaded"] = true;
                return;
            }

            if (AvaloniaXamlLoader.Load(new Uri("avares://RawImporterCS/Colors.axaml")) is ResourceDictionary colors)
            {
                resources.MergedDictionaries.Add(colors);
                resources["ThemeColorsLoaded"] = true;
            }
        }

        private static bool HasPalette(IResourceDictionary resources)
        {
            if (resources.TryGetValue("ThemeColorsLoaded", out var flag) && flag is bool loaded && loaded)
            {
                return true;
            }

            return TryGetResource(resources, "ColorBgLight", out _) &&
                   TryGetResource(resources, "ColorBgDark", out _) &&
                   TryGetResource(resources, "BrushShadowLight", out _);
        }

        private static bool TryGetResource(IResourceDictionary resources, string key, out object? value)
        {
            if (resources is IResourceProvider provider &&
                provider.TryGetResource(key, ThemeVariant.Default, out value))
            {
                return true;
            }

            value = null;
            return false;
        }
    }
}
