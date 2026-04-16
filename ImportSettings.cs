using System.Collections.Generic; // Provides the List<T> collection used for manual extensions
using System.IO; // Enables filesystem checks for validating folder paths

namespace RawImporterCS // Namespace for the RAW importer application
{
    public class ImportSettings // Encapsulates all configurable settings for an import run
    {
        public string SourceFolder { get; set; } = string.Empty; // Folder to scan for files
        public string TargetFolder { get; set; } = string.Empty; // Destination root folder for copied files

        // RAW formats
        public bool IncludeRaw { get; set; }    // Include Sony .ARW files
        public bool IncludeCr2 { get; set; }    // Include Canon .CR2 files
        public bool IncludeCr3 { get; set; }    // Include Canon .CR3 files
        public bool IncludeNef { get; set; }    // Include Nikon .NEF files
        public bool IncludeRaf { get; set; }    // Include Fuji .RAF files
        public bool IncludeOrf { get; set; }    // Include Olympus .ORF files
        public bool IncludeRw2 { get; set; }    // Include Panasonic .RW2 files
        public bool IncludeDng { get; set; }    // Include Adobe .DNG files

        // Image formats
        public bool IncludeJpg { get; set; }    // Include .JPG files
        public bool IncludeJpeg { get; set; }   // Include .JPEG files
        public bool IncludePng { get; set; }    // Include .PNG files
        public bool IncludeTiff { get; set; }   // Include .TIFF files
        public bool IncludeHeic { get; set; }   // Include .HEIC files
        public bool IncludeAllFiles { get; set; } // Include every file regardless of extension

        // Import options
        public bool SkipDuplicates { get; set; } = true; // Avoid copying files if a name already exists at the target
        public bool PreserveSubfolders { get; set; } = true; // Re-create source subfolders under the target
        public bool CreateSubfoldersPerFileType { get; set; } // Create file-type folders (e.g., JPG, ARW) as final directory segment
        public bool UseMultithread { get; set; } // Toggle parallel import mode

        // Target folder layout (Year/Month/Day, Year/Month, Flat, etc.)
        public string FolderStructure { get; set; } = "Year/Month/Day"; // Default organizes by year, month, and day

        public List<string> ManualExtensions { get; set; } = new(); // Extra user-defined extensions to include

        public bool IsValid(out string error) // Validates that settings contain usable paths and extensions
        {
            if (string.IsNullOrWhiteSpace(SourceFolder) || !Directory.Exists(SourceFolder)) // Ensure a source folder is provided and exists
            {
                error = Language.T("error.invalidSource"); // Provide localized error message for invalid source
                return false; // Signal invalid configuration
            }

            if (string.IsNullOrWhiteSpace(TargetFolder)) // Ensure a target path is provided
            {
                error = Language.T("error.invalidTarget"); // Provide localized error message for invalid target
                return false; // Signal invalid configuration
            }

            if (!IncludeRaw && !IncludeCr2 && !IncludeCr3 && !IncludeNef && !IncludeRaf &&  // Verify at least one file type is selected
                !IncludeOrf && !IncludeRw2 && !IncludeDng && !IncludeJpg && !IncludeJpeg && !IncludePng &&  // Check RAW and image toggles
                !IncludeTiff && !IncludeHeic && !IncludeAllFiles && // Allow bypass via "all files"
                (ManualExtensions == null || ManualExtensions.Count == 0)) // Allow custom extensions as a fallback
            {
                error = Language.T("error.noFiletypes"); // Provide localized error message when nothing is selected
                return false; // Signal invalid configuration
            }

            error = string.Empty; // Clear error when configuration is valid
            return true; // Signal valid settings
        }
    } // End of ImportSettings class
} // End of RawImporterCS namespace
