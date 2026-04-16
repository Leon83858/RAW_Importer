# RAW Importer v2.1.0

First public release.

## Highlights

- Delivered as a ready-to-run, self-contained executable (no .NET SDK required for end users)
- Avalonia-based desktop app focused on fast RAW import workflows
- Automated release pipeline with platform-specific build artifacts

## Included Features

- Import from a selectable source folder into a target folder
- Flexible target structure:
  - Year/Month/Day
  - Year/Month
  - Flat
  - optionally preserve source subfolders
  - optionally create subfolders by file type
- Duplicate handling:
  - skip duplicates
  - or create unique file names with numeric suffixes
- Optional multithreaded import mode (experimental)
- Persistent app settings (including language and theme)
- Language switch between German and English
- Theme modes: Light, Dark, System

## Supported File Types

RAW:
- ARW, CR2, CR3, NEF, RAF, ORF, RW2, DNG

Image formats:
- JPG, JPEG, PNG, TIFF, TIF, HEIC, HEIF

Additionally:
- Option to import all files regardless of extension
- Support for manually defined additional extensions

## Distribution

Release artifacts are provided as ZIP files per platform:
- linux-x64
- osx-arm64
- osx-x64
- win-arm64
- win-x64

## Quality and Stability

- Automated tests for:
  - duplicate detection and filename collision handling
  - target path generation across different folder structure options
- CI-based release generation with automatic package upload

## Known Notes

- Multithreaded mode is marked as experimental.
- On first launch, the operating system may show a security prompt depending on the platform.

## Thanks

Thanks to everyone who helped with testing and feedback for this first public release.
