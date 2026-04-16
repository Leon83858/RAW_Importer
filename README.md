# RAW Importer (C# / Avalonia)

Desktop app for importing and organizing RAW photo files.

## Tech Stack

- .NET 10
- Avalonia UI 11.1.3
- xUnit tests in `RawImporterCS.Tests`

## Project Structure

- `RawImporterCS.csproj`: Main desktop app
- `RawImporterCS.Tests/RawImporterCS.Tests.csproj`: Test project
- `Properties/PublishProfiles/`: Manual publish profiles for self-contained exports

## Prerequisites

- .NET SDK 10.x

## Local Development

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

## Release Builds (Manual)

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

## GitHub Actions Release Workflow

A release workflow is included at `.github/workflows/release.yml`.

It will:

- run tests
- build self-contained release artifacts for:
  - `linux-x64`
  - `osx-arm64`
  - `osx-x64`
  - `win-arm64`
  - `win-x64`
- package each output as a `.zip`
- publish GitHub Release assets automatically

### Trigger a Release

Create and push a version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

This creates a GitHub Release for that tag and uploads all platform archives.
