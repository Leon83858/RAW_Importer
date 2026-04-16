# RAW Importer (C# / Avalonia)

Desktop app for importing and organizing RAW photo files.

## Distribution

This project is delivered as a ready-to-run executable.

- End users should download the platform-specific archive from GitHub Releases.
- No local .NET SDK installation is required for running released builds.

## Download And Run

1. Open the Releases page of this repository.
2. Download the archive for your platform.
3. Extract the ZIP file.
4. Start the executable:
  - macOS: `RawImporter.app`
  - Windows: `RawImporter.exe`
  - Linux: `RawImporter`

## Tech Stack

- .NET 10
- Avalonia UI 11.1.3
- xUnit tests in `RawImporterCS.Tests`

## Project Structure

- `RawImporterCS.csproj`: Main desktop app
- `RawImporterCS.Tests/RawImporterCS.Tests.csproj`: Test project
- `Properties/PublishProfiles/`: Manual publish profiles for self-contained exports

## Development Prerequisites

- .NET SDK 10.x

## Local Development (Optional)

Restore dependencies:

```bash
dotnet restore C#.sln
```

Build solution:

```bash
dotnet build C#.sln -c Debug
```

Run tests:

```bash
dotnet test C#.sln -c Debug
```

Run app:

```bash
dotnet run --project RawImporterCS.csproj
```

## Packaging Details

The project uses self-contained single-file publishing with Avalonia native libraries extracted at runtime.

Example (macOS ARM64):

```bash
dotnet publish RawImporterCS.csproj \
  -c Release \
  -r osx-arm64 \
  --self-contained true \
  -p:PublishSingleFile=true \
  -p:IncludeNativeLibrariesForSelfExtract=true \
  -p:IncludeAllContentForSelfExtract=true \
  -p:DebugType=none \
  -p:DebugSymbols=false
```

<img width="1072" height="860" alt="Screenshot 2026-04-16 at 17 50 56" src="https://github.com/user-attachments/assets/6e93ba56-2774-44c4-97b0-15820f833d09" />
<img width="1072" height="860" alt="Screenshot 2026-04-16 at 17 51 08" src="https://github.com/user-attachments/assets/0d082d95-a3e6-4689-87f7-a470e4f1b68c" />
