# RAW Importer

<p align="center">
	<img src="src-tauri/icons/icon.png" width="120" alt="RAW Importer app icon">
</p>

A desktop app for copying camera RAW and image files into organised, date-based folders. RAW Importer never modifies the source files.

## Install

### macOS (Apple Silicon)

```sh
brew tap Leon83858/homebrew-raw-importer
brew install --cask raw-importer
```

The app is currently unsigned and not notarized. If Gatekeeper blocks the first launch, open Finder, Control-click **RAW Importer**, then choose **Open**.

### Windows 10/11 (x64)

Download and run the [Windows installer](https://github.com/Leon83858/RAW_Importer/releases/latest/download/RAW%20Importer_3.0.0_windows_x64-setup.exe). It installs for the current user without administrator access. If SmartScreen appears, choose **More info** and **Run anyway** only if you trust the download.

### Debian/Ubuntu (x86_64)

Download the Debian package and its checksum, verify it, then install it with apt:

```sh
curl -fLO https://github.com/Leon83858/RAW_Importer/releases/latest/download/raw-importer_3.0.0_amd64.deb
curl -fLO https://github.com/Leon83858/RAW_Importer/releases/latest/download/raw-importer_3.0.0_amd64.deb.sha256
sha256sum -c raw-importer_3.0.0_amd64.deb.sha256
sudo apt install ./raw-importer_3.0.0_amd64.deb
```

This installs the downloaded package file; no apt repository is configured. The package is unsigned and distributed with a SHA-256 checksum.

### Arch Linux (x86_64)

Download the Arch package and checksum, verify it, then install it with pacman:

```sh
curl -fLO https://github.com/Leon83858/RAW_Importer/releases/latest/download/raw-importer-3.0.0-1-x86_64.pkg.tar.zst
curl -fLO https://github.com/Leon83858/RAW_Importer/releases/latest/download/raw-importer-3.0.0-1-x86_64.pkg.tar.zst.sha256
sha256sum -c raw-importer-3.0.0-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./raw-importer-3.0.0-1-x86_64.pkg.tar.zst
```

This installs a downloaded local package; no pacman repository is configured. The package is unsigned and distributed with a SHA-256 checksum.

### Other Linux distributions

Download the x86_64 ZIP from [Releases](https://github.com/Leon83858/RAW_Importer/releases/latest), extract it, install WebKitGTK 4.1 and GTK 3 using your distribution's package manager, then run `./install.sh` from the extracted folder.

<!-- Screenshot placeholder: Main import screen. Replace this comment with a rendered screenshot when one is available. -->

## First import

1. Select the source folder, such as a camera card, and a destination folder.
2. Choose the file types and folder layout. Add custom file extensions when needed.
3. Scan the source and review the preview before starting the import.
4. Start the import. The source files remain unchanged.

<!-- Screenshot placeholder: Import preview. Keep this as an HTML comment until a real screenshot is added. -->

## Features

- Organise imports by year, month and day, with optional source subfolders or file-type folders.
- Use EXIF capture dates or filesystem modification dates for folder naming.
- Select common RAW and image formats, or add custom file extensions.
- Skip or rename name conflicts; optionally skip files verified to have identical content.
- Keep local import history and recover recoverable session state.
- Use English, German, French, Dutch or Belgian Dutch, with light, dark or system appearance.
- Open Settings from the menu or with `Cmd+,` on macOS.
- Check GitHub for updates manually or opt in to a startup check. Updates are never downloaded or installed automatically.

Built-in extensions include ARW, CR2, CR3, NEF, RAF, ORF, RW2, DNG, JPG, JPEG, PNG, TIFF/TIF and HEIC/HEIF.

## License

RAW Importer is distributed under the GNU General Public License v3.0 only. See [LICENSE](LICENSE).
