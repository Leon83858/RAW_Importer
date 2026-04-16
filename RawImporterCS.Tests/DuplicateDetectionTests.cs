using System.Text;

namespace RawImporterCS.Tests;

public class DuplicateDetectionTests
{
    public static TheoryData<string> SupportedExtensions => new()
    {
        ".arw",
        ".cr2",
        ".cr3",
        ".nef",
        ".raf",
        ".orf",
        ".rw2",
        ".dng",
        ".jpg",
        ".jpeg",
        ".png",
        ".tiff",
        ".tif",
        ".heic",
        ".heif"
    };

    [Theory]
    [MemberData(nameof(SupportedExtensions))]
    public async Task ImportAsync_SkipDuplicatesTrue_DoesNotOverwriteExistingTarget_ForSupportedExtension(string extension)
    {
        using var workspace = new TestWorkspace();
        var sourceFileName = "dup-test" + extension;
        var sourceFile = Path.Combine(workspace.SourceFolder, sourceFileName);
        var targetFile = Path.Combine(workspace.TargetFolder, sourceFileName);

        await File.WriteAllTextAsync(sourceFile, "new-content", Encoding.UTF8);
        await File.WriteAllTextAsync(targetFile, "existing-content", Encoding.UTF8);

        var settings = CreateSettings(workspace.SourceFolder, workspace.TargetFolder, extension, skipDuplicates: true, useMultithread: false);
        var service = new ImportService();

        var scanned = await service.ScanAsync(settings);
        Assert.Single(scanned);

        await service.ImportAsync(settings, scanned);

        var actualContent = await File.ReadAllTextAsync(targetFile, Encoding.UTF8);
        Assert.Equal("existing-content", actualContent);
        Assert.Single(Directory.GetFiles(workspace.TargetFolder));
    }

    [Theory]
    [MemberData(nameof(SupportedExtensions))]
    public async Task ImportAsync_SkipDuplicatesFalse_CreatesUniqueCopy_ForSupportedExtension(string extension)
    {
        using var workspace = new TestWorkspace();
        var sourceFileName = "dup-test" + extension;
        var sourceFile = Path.Combine(workspace.SourceFolder, sourceFileName);
        var targetFile = Path.Combine(workspace.TargetFolder, sourceFileName);

        await File.WriteAllTextAsync(sourceFile, "new-content", Encoding.UTF8);
        await File.WriteAllTextAsync(targetFile, "existing-content", Encoding.UTF8);

        var settings = CreateSettings(workspace.SourceFolder, workspace.TargetFolder, extension, skipDuplicates: false, useMultithread: false);
        var service = new ImportService();

        var scanned = await service.ScanAsync(settings);
        Assert.Single(scanned);

        await service.ImportAsync(settings, scanned);

        var originalContent = await File.ReadAllTextAsync(targetFile, Encoding.UTF8);
        var uniqueTargetFile = Path.Combine(
            workspace.TargetFolder,
            $"{Path.GetFileNameWithoutExtension(sourceFileName)} (1){Path.GetExtension(sourceFileName)}");

        Assert.Equal("existing-content", originalContent);
        Assert.True(File.Exists(uniqueTargetFile));
        Assert.Equal("new-content", await File.ReadAllTextAsync(uniqueTargetFile, Encoding.UTF8));
        Assert.Equal(2, Directory.GetFiles(workspace.TargetFolder).Length);
    }

    [Fact]
    public async Task ImportAsync_SkipDuplicatesTrue_Multithread_DoesNotOverwriteExistingTargets_ForAllSupportedTypes()
    {
        using var workspace = new TestWorkspace();
        var service = new ImportService();

        foreach (var extension in SupportedExtensions)
        {
            var fileName = "multi-dup" + extension;
            await File.WriteAllTextAsync(Path.Combine(workspace.SourceFolder, fileName), "new-content", Encoding.UTF8);
            await File.WriteAllTextAsync(Path.Combine(workspace.TargetFolder, fileName), "existing-content", Encoding.UTF8);
        }

        // Include one no-extension file as extra duplicate edge case in all-files mode.
        await File.WriteAllTextAsync(Path.Combine(workspace.SourceFolder, "noext"), "new-content", Encoding.UTF8);
        await File.WriteAllTextAsync(Path.Combine(workspace.TargetFolder, "noext"), "existing-content", Encoding.UTF8);

        var settings = new ImportSettings
        {
            SourceFolder = workspace.SourceFolder,
            TargetFolder = workspace.TargetFolder,
            IncludeAllFiles = true,
            SkipDuplicates = true,
            UseMultithread = true,
            PreserveSubfolders = false,
            CreateSubfoldersPerFileType = false,
            FolderStructure = "Flat"
        };

        var scanned = await service.ScanAsync(settings);
        Assert.Equal(SupportedExtensions.Count + 1, scanned.Count);

        await service.ImportAsync(settings, scanned);

        foreach (var extension in SupportedExtensions)
        {
            var fileName = "multi-dup" + extension;
            var targetContent = await File.ReadAllTextAsync(Path.Combine(workspace.TargetFolder, fileName), Encoding.UTF8);
            Assert.Equal("existing-content", targetContent);
        }

        Assert.Equal("existing-content", await File.ReadAllTextAsync(Path.Combine(workspace.TargetFolder, "noext"), Encoding.UTF8));
        Assert.Equal(SupportedExtensions.Count + 1, Directory.GetFiles(workspace.TargetFolder).Length);
    }

    private static ImportSettings CreateSettings(
        string sourceFolder,
        string targetFolder,
        string extension,
        bool skipDuplicates,
        bool useMultithread)
    {
        var settings = new ImportSettings
        {
            SourceFolder = sourceFolder,
            TargetFolder = targetFolder,
            SkipDuplicates = skipDuplicates,
            UseMultithread = useMultithread,
            PreserveSubfolders = false,
            CreateSubfoldersPerFileType = false,
            FolderStructure = "Flat"
        };

        EnableFileType(settings, extension);
        return settings;
    }

    private static void EnableFileType(ImportSettings settings, string extension)
    {
        var ext = extension.ToLowerInvariant();

        switch (ext)
        {
            case ".arw":
                settings.IncludeRaw = true;
                break;
            case ".cr2":
                settings.IncludeCr2 = true;
                break;
            case ".cr3":
                settings.IncludeCr3 = true;
                break;
            case ".nef":
                settings.IncludeNef = true;
                break;
            case ".raf":
                settings.IncludeRaf = true;
                break;
            case ".orf":
                settings.IncludeOrf = true;
                break;
            case ".rw2":
                settings.IncludeRw2 = true;
                break;
            case ".dng":
                settings.IncludeDng = true;
                break;
            case ".jpg":
                settings.IncludeJpg = true;
                break;
            case ".jpeg":
                settings.IncludeJpeg = true;
                break;
            case ".png":
                settings.IncludePng = true;
                break;
            case ".tif":
            case ".tiff":
                settings.IncludeTiff = true;
                break;
            case ".heic":
            case ".heif":
                settings.IncludeHeic = true;
                break;
            default:
                throw new InvalidOperationException($"No mapping defined for extension '{extension}'.");
        }
    }

    private sealed class TestWorkspace : IDisposable
    {
        public string RootFolder { get; }
        public string SourceFolder { get; }
        public string TargetFolder { get; }

        public TestWorkspace()
        {
            RootFolder = Path.Combine(Path.GetTempPath(), "RawImporterCS.Tests", Guid.NewGuid().ToString("N"));
            SourceFolder = Path.Combine(RootFolder, "source");
            TargetFolder = Path.Combine(RootFolder, "target");

            Directory.CreateDirectory(SourceFolder);
            Directory.CreateDirectory(TargetFolder);
        }

        public void Dispose()
        {
            try
            {
                if (Directory.Exists(RootFolder))
                {
                    Directory.Delete(RootFolder, recursive: true);
                }
            }
            catch
            {
                // Best-effort cleanup for temporary test workspace.
            }
        }
    }
}
