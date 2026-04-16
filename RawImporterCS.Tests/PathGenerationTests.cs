using System.IO;

namespace RawImporterCS.Tests;

public class PathGenerationTests
{
    private const string FileName = "photo.arw";

    [Fact]
    public void BuildTargetDirectoryPath_BothOptionsChecked_AppendsSourceSubfoldersThenFileType()
    {
        var sourceRoot = CombineRoot("DCIM");
        var sourceFile = Path.Combine(sourceRoot, "Cam1", FileName);
        var structure = Path.Combine("2026", "03", "01");

        var targetDirectory = ImportService.BuildTargetDirectoryPath(
            baseTargetFolder: CombineRoot("Output"),
            structuredRelativePath: structure,
            sourceFolder: sourceRoot,
            sourceFile: sourceFile,
            preserveSubfolders: true,
            createSubfoldersPerFileType: true);

        var finalPath = Path.Combine(targetDirectory, Path.GetFileName(sourceFile));
        var expectedFinalPath = Path.Combine(CombineRoot("Output"), "2026", "03", "01", "Cam1", "ARW", FileName);

        Assert.Equal(expectedFinalPath, finalPath);
    }

    [Fact]
    public void BuildTargetDirectoryPath_OnlyPreserveChecked_AppendsOnlyRelativeSourceSubfolders()
    {
        var sourceRoot = CombineRoot("DCIM");
        var sourceFile = Path.Combine(sourceRoot, "Cam1", FileName);
        var structure = Path.Combine("2026", "03", "01");

        var targetDirectory = ImportService.BuildTargetDirectoryPath(
            baseTargetFolder: CombineRoot("Output"),
            structuredRelativePath: structure,
            sourceFolder: sourceRoot,
            sourceFile: sourceFile,
            preserveSubfolders: true,
            createSubfoldersPerFileType: false);

        var finalPath = Path.Combine(targetDirectory, Path.GetFileName(sourceFile));
        var expectedFinalPath = Path.Combine(CombineRoot("Output"), "2026", "03", "01", "Cam1", FileName);

        Assert.Equal(expectedFinalPath, finalPath);
    }

    [Fact]
    public void BuildTargetDirectoryPath_OnlyFileTypeChecked_AppendsOnlyFileTypeFolder()
    {
        var sourceRoot = CombineRoot("DCIM");
        var sourceFile = Path.Combine(sourceRoot, "Cam1", FileName);
        var structure = Path.Combine("2026", "03", "01");

        var targetDirectory = ImportService.BuildTargetDirectoryPath(
            baseTargetFolder: CombineRoot("Output"),
            structuredRelativePath: structure,
            sourceFolder: sourceRoot,
            sourceFile: sourceFile,
            preserveSubfolders: false,
            createSubfoldersPerFileType: true);

        var finalPath = Path.Combine(targetDirectory, Path.GetFileName(sourceFile));
        var expectedFinalPath = Path.Combine(CombineRoot("Output"), "2026", "03", "01", "ARW", FileName);

        Assert.Equal(expectedFinalPath, finalPath);
    }

    [Fact]
    public void BuildTargetDirectoryPath_NeitherOptionChecked_UsesOnlyBaseAndStructure()
    {
        var sourceRoot = CombineRoot("DCIM");
        var sourceFile = Path.Combine(sourceRoot, "Cam1", FileName);
        var structure = Path.Combine("2026", "03", "01");

        var targetDirectory = ImportService.BuildTargetDirectoryPath(
            baseTargetFolder: CombineRoot("Output"),
            structuredRelativePath: structure,
            sourceFolder: sourceRoot,
            sourceFile: sourceFile,
            preserveSubfolders: false,
            createSubfoldersPerFileType: false);

        var finalPath = Path.Combine(targetDirectory, Path.GetFileName(sourceFile));
        var expectedFinalPath = Path.Combine(CombineRoot("Output"), "2026", "03", "01", FileName);

        Assert.Equal(expectedFinalPath, finalPath);
    }

    [Fact]
    public void BuildTargetDirectoryPath_FileWithoutExtension_DoesNotAppendEmptyFileTypeFolder()
    {
        var sourceRoot = CombineRoot("DCIM");
        var sourceFile = Path.Combine(sourceRoot, "Cam1", "photo");
        var structure = Path.Combine("2026", "03", "01");

        var targetDirectory = ImportService.BuildTargetDirectoryPath(
            baseTargetFolder: CombineRoot("Output"),
            structuredRelativePath: structure,
            sourceFolder: sourceRoot,
            sourceFile: sourceFile,
            preserveSubfolders: true,
            createSubfoldersPerFileType: true);

        var finalPath = Path.Combine(targetDirectory, Path.GetFileName(sourceFile));
        var expectedFinalPath = Path.Combine(CombineRoot("Output"), "2026", "03", "01", "Cam1", "photo");

        Assert.Equal(expectedFinalPath, finalPath);
    }

    [Fact]
    public void BuildTargetDirectoryPath_ComplexSourceStructure_PreservesAllNestedFoldersBeforeFileType()
    {
        var sourceRoot = CombineRoot("DCIM");
        var sourceFile = Path.Combine(sourceRoot, "Cam1", "Session A", "Set-01", "Take_01.v1.JPG");
        var structure = Path.Combine("2026", "03", "01");

        var targetDirectory = ImportService.BuildTargetDirectoryPath(
            baseTargetFolder: CombineRoot("Output"),
            structuredRelativePath: structure,
            sourceFolder: sourceRoot,
            sourceFile: sourceFile,
            preserveSubfolders: true,
            createSubfoldersPerFileType: true);

        var finalPath = Path.Combine(targetDirectory, Path.GetFileName(sourceFile));
        var expectedFinalPath = Path.Combine(
            CombineRoot("Output"),
            "2026",
            "03",
            "01",
            "Cam1",
            "Session A",
            "Set-01",
            "JPG",
            "Take_01.v1.JPG");

        Assert.Equal(expectedFinalPath, finalPath);
    }

    [Fact]
    public void BuildTargetDirectoryPath_EmptyBasePath_ReturnsRelativeStructureAndOptionalFileType()
    {
        var sourceRoot = CombineRoot("DCIM");
        var sourceFile = Path.Combine(sourceRoot, FileName);
        var structure = Path.Combine("2026", "03", "01");

        var targetDirectory = ImportService.BuildTargetDirectoryPath(
            baseTargetFolder: string.Empty,
            structuredRelativePath: structure,
            sourceFolder: sourceRoot,
            sourceFile: sourceFile,
            preserveSubfolders: false,
            createSubfoldersPerFileType: true);

        var finalPath = Path.Combine(targetDirectory, Path.GetFileName(sourceFile));
        var expectedFinalPath = Path.Combine("2026", "03", "01", "ARW", FileName);

        Assert.Equal(expectedFinalPath, finalPath);
    }

    private static string CombineRoot(string firstSegment)
    {
        return Path.Combine(Path.DirectorySeparatorChar.ToString(), firstSegment);
    }
}
