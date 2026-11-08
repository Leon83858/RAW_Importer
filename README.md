# RAW Importer

A desktop application for importing camera RAW and image files into a date-based folder structure. Source files are copied, not modified.

## Features

- Organise imports by year, month and day, with optional source subfolders or file-type folders.
- Read capture dates from EXIF metadata, with a filesystem-date option.
- Select common RAW and image formats, or add custom file extensions.
- Handle name conflicts by skipping or renaming files; optionally skip verified identical content.
- Keep local import history and recoverable session state.
- Use English, German, French, Dutch or Belgian Dutch, with light, dark or system appearance.
- Check for stable updates from GitHub manually or opt in to a startup check. Updates are never downloaded or installed automatically.

Supported built-in extensions include ARW, CR2, CR3, NEF, RAF, ORF, RW2, DNG, JPG, JPEG, PNG, TIFF/TIF and HEIC/HEIF. Additional extensions can be configured in the app.

## Build

Requirements: Rust 1.90 or later, Node.js 22.12 or later, and the platform's Tauri/WebView dependencies.

```sh
npm ci
npm run types:generate
npm run tauri dev
```

TypeScript bindings in `src/types/generated` are generated from the Rust data transfer types by `npm run types:generate`.

Platform installer scripts:

- macOS Apple Silicon: `scripts/build-mac-installer.sh`
- Windows x64 from macOS: run `scripts/build-windows-installer.sh --setup` once, then `scripts/build-windows-installer.sh`
- Linux x86_64: run the **Linux installer** workflow from GitHub Actions; the ZIP is published as a workflow artifact. Linux builds require WebKitGTK 4.1 and GTK 3.

The macOS DMG and Windows setup executable are currently unsigned; macOS is not notarized. Windows cross-compilation is experimental. No public release is created by these build workflows.

## License

RAW Importer is distributed under the GNU General Public License v3.0 only. See [LICENSE](LICENSE).
