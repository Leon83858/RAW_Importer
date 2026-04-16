using System; // Provides base types and exceptions
using System.Collections.Generic; // Supplies generic collections like List<T> and HashSet<T>
using System.IO; // Enables filesystem operations for reading and writing files
using System.Linq; // Provides LINQ extensions used to materialize collections
using System.Threading; // Offers cancellation tokens and threading primitives
using System.Threading.Tasks; // Supports asynchronous programming constructs

namespace RawImporterCS // Namespace for the RAW importer services
{
    public class ImportService // Handles scanning and importing files according to user settings
    {
        // RAW format extensions grouped by camera manufacturers
        private static readonly string[] ArwExtensions = { ".arw" }; // Sony RAW format
        private static readonly string[] Cr2Extensions = { ".cr2" }; // Canon RAW format (older)
        private static readonly string[] Cr3Extensions = { ".cr3" }; // Canon RAW format (newer)
        private static readonly string[] NefExtensions = { ".nef" }; // Nikon RAW format
        private static readonly string[] RafExtensions = { ".raf" }; // Fujifilm RAW format
        private static readonly string[] OrfExtensions = { ".orf" }; // Olympus RAW format
        private static readonly string[] Rw2Extensions = { ".rw2" }; // Panasonic RAW format
        private static readonly string[] DngExtensions = { ".dng" }; // Adobe Digital Negative format

        // Common image format extensions for non-RAW files
        private static readonly string[] JpgOnlyExtensions = { ".jpg" }; // JPG image variant
        private static readonly string[] JpegOnlyExtensions = { ".jpeg" }; // JPEG image variant
        private static readonly string[] PngExtensions = { ".png" }; // PNG image format
        private static readonly string[] TiffExtensions = { ".tiff", ".tif" }; // TIFF image formats
        private static readonly string[] HeicExtensions = { ".heic", ".heif" }; // HEIC/HEIF image formats

        public async Task<List<string>> ScanAsync( // Asynchronously scans the source folder for matching files
            ImportSettings settings, // User-defined import settings controlling source, filters, and options
            IProgress<(int current, int total, string file)>? progress = null, // Optional progress reporter that receives count and file path
            CancellationToken cancellationToken = default) // Token allowing the caller to cancel the scan
        {
            var result = new List<string>(); // Accumulates file paths that satisfy the filters

            if (!Directory.Exists(settings.SourceFolder)) // Abort early if the source folder is missing
                return result; // Return an empty list because no files can be scanned

            var enumerationOptions = new EnumerationOptions
            {
                RecurseSubdirectories = true,
                IgnoreInaccessible = true
            };

            // A1: Build allowed extensions respecting separate JPG/JPEG toggles and manual entries
            var exts = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            if (settings.IncludeRaw)
                exts.UnionWith(ArwExtensions);
            if (settings.IncludeCr2)
                exts.UnionWith(Cr2Extensions);
            if (settings.IncludeCr3)
                exts.UnionWith(Cr3Extensions);
            if (settings.IncludeNef)
                exts.UnionWith(NefExtensions);
            if (settings.IncludeRaf)
                exts.UnionWith(RafExtensions);
            if (settings.IncludeOrf)
                exts.UnionWith(OrfExtensions);
            if (settings.IncludeRw2)
                exts.UnionWith(Rw2Extensions);
            if (settings.IncludeDng)
                exts.UnionWith(DngExtensions);

            if (settings.IncludeJpg)
                exts.UnionWith(JpgOnlyExtensions);
            if (settings.IncludeJpeg)
                exts.UnionWith(JpegOnlyExtensions);
            if (settings.IncludePng)
                exts.UnionWith(PngExtensions);
            if (settings.IncludeTiff)
                exts.UnionWith(TiffExtensions);
            if (settings.IncludeHeic)
                exts.UnionWith(HeicExtensions);

            if (settings.ManualExtensions != null)
            {
                foreach (var ext in settings.ManualExtensions)
                {
                    if (string.IsNullOrWhiteSpace(ext)) continue;
                    var e = ext.StartsWith(".") ? ext : "." + ext;
                    exts.Add(e);
                }
            }

            // A2: Pre-filter consistent candidate list so current/total and percentage align
            var allFiles = Directory.EnumerateFiles(settings.SourceFolder, "*", enumerationOptions).ToList();
            List<string> scanCandidates;
            if (settings.IncludeAllFiles)
            {
                scanCandidates = allFiles;
            }
            else
            {
                scanCandidates = new List<string>(allFiles.Count);
                foreach (var f in allFiles)
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    var ext = Path.GetExtension(f);
                    if (exts.Contains(ext))
                    {
                        scanCandidates.Add(f);
                    }
                }
            }

            // exts set constructed above

            var total = scanCandidates.Count;
            var current = 0;
            foreach (var file in scanCandidates) // Iterate over filtered candidates for consistent progress
            {
                cancellationToken.ThrowIfCancellationRequested();
                current++;
                progress?.Report((current, total, file));
                result.Add(file);
                await Task.Yield();
            }

            return result; // Return the filtered list of files
        } // End of ScanAsync

        public async Task ImportAsync( // Copies the provided files to the target directory honoring settings
            ImportSettings settings, // Settings describing destination, structure, and duplicate handling
            IEnumerable<string> files, // Collection of source files to import
            IProgress<(int current, int total, string file, string speed)>? progress = null, // Optional progress reporter for status updates
            CancellationToken cancellationToken = default) // Token allowing the caller to cancel import work
        {
            Directory.CreateDirectory(settings.TargetFolder); // Ensure the target root exists before copying

            var list = files is List<string> l ? l : files.ToList(); // Materialize the file list for counting and iteration
            var total = list.Count; // Total number of files to import
            var current = 0; // Tracks the current progress index
            var imported = 0; // Counts successfully imported files

            if (settings.UseMultithread) // Use parallel processing when enabled
            {
                var options = new ParallelOptions // Configure parallel execution options
                {
                    CancellationToken = cancellationToken, // Allow cancellation across parallel workers
                    MaxDegreeOfParallelism = Environment.ProcessorCount // Limit concurrency to available CPU cores
                };

                var lockObj = new object(); // Synchronization object for shared state updates

                await Task.Run(() => // Run parallel copy work on a background thread
                {
                    Parallel.ForEach(list, options, src => // Iterate each source file in parallel
                    {
                        cancellationToken.ThrowIfCancellationRequested(); // Abort promptly if cancelled

                        try // Attempt to copy the file
                        {
                            var targetDir = BuildTargetDirectory(settings, src); // Resolve destination directory based on settings
                            var destPath = Path.Combine(targetDir, Path.GetFileName(src)); // Build full destination path for the file

                            // C1: Duplikatspr\u00fcfung anhand des Zielpfads, nicht nur des Dateinamens
                            if (settings.SkipDuplicates && File.Exists(destPath))
                            {
                                lock (lockObj) // Synchronize progress updates
                                {
                                    current++; // Advance progress counter even when skipping
                                    progress?.Report((current, total, src, "0 MB/s")); // Report skipped file with zero speed
                                }
                                return; // Skip further processing for this file
                            }

                            // C2: Retry-Logik falls zwischen Exists und Move/Copy eine Race-Condition auftritt
                            if (!settings.SkipDuplicates)
                            {
                                destPath = EnsureUniquePath(destPath);
                            }

                            lock (lockObj) // Synchronize directory creation to avoid race conditions
                            {
                                Directory.CreateDirectory(Path.GetDirectoryName(destPath)!); // Ensure destination directory exists
                            }

                            var sw = System.Diagnostics.Stopwatch.StartNew(); // Start timing the copy for speed reporting
                            var fileSize = new FileInfo(src).Length; // Measure source file size in bytes

                            // Retry logic: if a race occurs between Exists and Move/Copy, generate new unique path when not skipping duplicates
                            int retries = 0;
                            const int maxRetries = 5;
                            while (retries < maxRetries)
                            {
                                try
                                {
                                    CopyFileCancellable(src, destPath, cancellationToken); // Perform a cancellable synchronous copy
                                    break; // Success
                                }
                                catch (IOException)
                                {
                                    // Duplicate race guard for multithread: if SkipDuplicates and dest now exists, treat as SKIPPED
                                    if (settings.SkipDuplicates && File.Exists(destPath))
                                    {
                                        lock (lockObj)
                                        {
                                            current++;
                                            progress?.Report((current, total, src, "0 MB/s"));
                                        }
                                        return; // Skip reporting error
                                    }
                                    if (retries < maxRetries - 1 && !settings.SkipDuplicates)
                                    {
                                        retries++;
                                        destPath = EnsureUniquePath(destPath);
                                        continue; // retry with a new unique path
                                    }
                                    throw; // propagate as error when retries exhausted or SkipDuplicates is false and not a duplicate race
                                }
                                // filtered catch removed; handled above
                            }

                            sw.Stop(); // Stop timing once copy completes

                            var durationSeconds = sw.Elapsed.TotalSeconds; // Calculate elapsed time in seconds
                            var fileSizeMB = fileSize / (1024.0 * 1024.0); // Convert file size to megabytes
                            var speedMBps = durationSeconds > 0 ? fileSizeMB / durationSeconds : 0; // Compute throughput avoiding divide-by-zero
                            var speedText = $"{speedMBps:F2} MB/s"; // Format speed as text for display

                            lock (lockObj) // Synchronize updates shared across threads
                            {
                                current++; // Increment processed count
                                imported++; // Increment successful import count
                                progress?.Report((current, total, src, speedText)); // Notify listeners of completion and speed
                            }
                        }
                        catch (OperationCanceledException) // Special-case cancellation to propagate correctly
                        {
                            throw; // Re-throw so Parallel honors cancellation
                        }
                        catch // Handle any other copy error gracefully
                        {
                            lock (lockObj) // Synchronize progress update for failures
                            {
                                current++; // Increment processed count even on failure
                                progress?.Report((current, total, src, "Error")); // Report error status to the UI
                            }
                        }
                    }); // End of Parallel.ForEach loop
                }, cancellationToken); // Bind cancellation to the background Task
            }
            else // Run import sequentially when multithreading is disabled
            {
                foreach (var src in list) // Process each source file one by one
                {
                    cancellationToken.ThrowIfCancellationRequested(); // Allow cancellation between files

                    try // Attempt to copy sequentially
                    {
                        var targetDir = BuildTargetDirectory(settings, src); // Resolve destination directory based on structure settings
                        var destPath = Path.Combine(targetDir, Path.GetFileName(src)); // Build full destination path

                        // C1: Duplikatspr\u00fcfung anhand des Zielpfads
                        if (settings.SkipDuplicates && File.Exists(destPath))
                        {
                            current++; // Advance progress counter for the skipped file
                            progress?.Report((current, total, src, "0 MB/s")); // Report skip with zero speed
                            continue; // Move to next file
                        }

                        // C2: Retry-Logik falls zwischen Exists und Move/Copy eine Race-Condition auftritt
                        if (!settings.SkipDuplicates)
                        {
                            destPath = EnsureUniquePath(destPath);
                        }

                        Directory.CreateDirectory(Path.GetDirectoryName(destPath)!); // Ensure destination folder exists

                        var sw = System.Diagnostics.Stopwatch.StartNew(); // Start timing the copy
                        var fileSize = new FileInfo(src).Length; // Get source file size for speed calculation

                        // Retry logic with duplicate-race handling
                        int retries = 0;
                        const int maxRetries = 5;
                        while (retries < maxRetries)
                        {
                            try
                            {
                                await Task.Run(() => CopyFileCancellable(src, destPath, cancellationToken), cancellationToken); // Copy on background thread to honor cancellation
                                break; // Success
                            }
                            catch (IOException)
                            {
                                // Duplicate race guard: if SkipDuplicates and dest exists now, SKIP
                                if (settings.SkipDuplicates && File.Exists(destPath))
                                {
                                    current++;
                                    progress?.Report((current, total, src, "0 MB/s"));
                                    goto SkipSequentialReport; // skip error reporting
                                }
                                if (retries < maxRetries - 1 && !settings.SkipDuplicates)
                                {
                                    retries++;
                                    destPath = EnsureUniquePath(destPath);
                                    continue;
                                }
                                throw; // escalate when not a duplicate race or retries exhausted
                            }
                            // filtered catch removed; handled above
                        }

                        sw.Stop(); // Stop timing after copy finishes

                        var durationSeconds = sw.Elapsed.TotalSeconds; // Measure elapsed copy time
                        var fileSizeMB = fileSize / (1024.0 * 1024.0); // Convert bytes to megabytes
                        var speedMBps = durationSeconds > 0 ? fileSizeMB / durationSeconds : 0; // Calculate throughput avoiding divide-by-zero
                        var speedText = $"{speedMBps:F2} MB/s"; // Format speed string for UI

                        current++; // Advance processed count
                        imported++; // Increment successful import count
                        progress?.Report((current, total, src, speedText)); // Report progress including speed
                    SkipSequentialReport: ;
                    }
                    catch (OperationCanceledException) // Pass cancellation back to caller
                    {
                        throw; // Re-throw to respect cancellation token
                    }
                    catch // Handle any unexpected errors during copy
                    {
                        current++; // Advance processed count even on failure
                        progress?.Report((current, total, src, "Error")); // Report error status for this file
                    }

                    await Task.Yield(); // Yield control to keep UI responsive between iterations
                }
            }
        } // End of ImportAsync

        /// <summary>
        /// Copies a file to a temporary path and atomically moves it into place while honoring cancellation.
        /// Cleans up temporary artifacts when failures occur and is synchronous for use inside Parallel.ForEach.
        /// </summary>
        private void CopyFileCancellable(string src, string dest, CancellationToken cancellationToken) // Performs cancellable file copy with atomic move
        {
            var destDir = Path.GetDirectoryName(dest) ?? throw new InvalidOperationException("Destination directory missing"); // Derive target directory or fail fast
            Directory.CreateDirectory(destDir); // Ensure the destination directory exists

            var temp = dest + ".part." + Guid.NewGuid().ToString("N"); // Compose a unique temporary file name

            const int bufferSize = 81920; // Use 80 KB buffer size for efficient streaming
            try // Attempt the copy and move
            {
                using (var srcFs = new FileStream(src, FileMode.Open, FileAccess.Read, FileShare.Read)) // Open source file for reading
                using (var destFs = new FileStream(temp, FileMode.CreateNew, FileAccess.Write, FileShare.None)) // Create temp destination for writing
                {
                    var buffer = new byte[bufferSize]; // Allocate buffer for streaming data
                    int read; // Holds the number of bytes read per iteration
                    while ((read = srcFs.Read(buffer, 0, buffer.Length)) > 0) // Read until end of file
                    {
                        cancellationToken.ThrowIfCancellationRequested(); // Respect cancellation between read operations
                        destFs.Write(buffer, 0, read); // Write the read bytes to the temp file
                    }

                    destFs.Flush(); // Flush remaining buffered data to disk
                }

                File.Move(temp, dest); // Atomically move the temp file to the final destination
            }
            catch // Handle any failure during copy or move
            {
                try { if (File.Exists(temp)) File.Delete(temp); } catch { } // Attempt to delete the temporary file if it exists
                throw; // Re-throw to propagate error to caller
            }
        } // End of CopyFileCancellable

        /// <summary>
        /// Builds the destination folder path based on folder structure and optional subfolder features.
        /// </summary>
        private string BuildTargetDirectory(ImportSettings settings, string sourceFile) // Constructs the target directory for a given source file
        {
            var fileInfo = new FileInfo(sourceFile); // Gather information about the source file
            var fileDate = fileInfo.LastWriteTime; // Use the file's last write time for organizing
            var monthName = GetMonthName(fileDate.Month); // Convert month number to readable month name

            var structuredRelativePath = settings.FolderStructure switch // Decide folder layout based on selected structure
            {
                "Year/Month/Day" => Path.Combine( // Year/Month/Day layout
                    fileDate.Year.ToString(), // Year component
                    $"{fileDate.Month:D2} {monthName}", // Month component including number and name
                    fileDate.Day.ToString("D2") // Day component
                ),
                "Year/Month" => Path.Combine( // Year/Month layout
                    fileDate.Year.ToString(), // Year component
                    $"{fileDate.Month:D2} {monthName}" // Month component including number and name
                ),
                "Year" => fileDate.Year.ToString(), // Year-only layout
                "Day-Month-Year" => $"{fileDate.Day:D2}-{fileDate.Month:D2}-{fileDate.Year}", // Custom Day-month-year segment
                "Flat" => string.Empty, // Flat layout copies directly into target root
                _ => string.Empty // Default fallback uses the target root as-is
            };

            return BuildTargetDirectoryPath(
                settings.TargetFolder,
                structuredRelativePath,
                settings.SourceFolder,
                sourceFile,
                settings.PreserveSubfolders,
                settings.CreateSubfoldersPerFileType);
        } // End of BuildTargetDirectory

        /// <summary>
        /// Builds target directory in strict order: base target -> structure -> preserved source subfolders -> file type folder.
        /// </summary>
        internal static string BuildTargetDirectoryPath(
            string baseTargetFolder,
            string structuredRelativePath,
            string sourceFolder,
            string sourceFile,
            bool preserveSubfolders,
            bool createSubfoldersPerFileType)
        {
            var targetDirectory = string.IsNullOrWhiteSpace(structuredRelativePath)
                ? (baseTargetFolder ?? string.Empty)
                : Path.Combine(baseTargetFolder ?? string.Empty, structuredRelativePath);

            if (preserveSubfolders) // Optionally append the relative source subfolder structure
            {
                var sourceFileDir = Path.GetDirectoryName(sourceFile) ?? sourceFolder; // Resolve the directory of the source file
                try // Attempt to compute relative path from the source root
                {
                    var relativePath = Path.GetRelativePath(sourceFolder, sourceFileDir); // Compute subfolder path relative to source root

                    // Security: never allow traversal segments to escape target directory.
                    var normalized = relativePath
                        .Replace('/', Path.DirectorySeparatorChar)
                        .Replace('\\', Path.DirectorySeparatorChar);

                    if (!string.IsNullOrWhiteSpace(normalized) && normalized != "." && !Path.IsPathRooted(normalized))
                    {
                        var unsafeTraversal = false;
                        foreach (var segment in normalized.Split(Path.DirectorySeparatorChar, StringSplitOptions.RemoveEmptyEntries))
                        {
                            if (segment == "..")
                            {
                                unsafeTraversal = true;
                                break;
                            }
                        }

                        if (!unsafeTraversal)
                        {
                            targetDirectory = Path.Combine(targetDirectory, normalized); // Append safe relative source path
                        }
                    }
                }
                catch // Swallow path calculation errors and keep current base/structure path
                {
                    // Fallback: keep targetDirectory unchanged
                }
            }

            if (createSubfoldersPerFileType) // Optionally append extension folder as final directory segment
            {
                var extension = Path.GetExtension(sourceFile ?? string.Empty);
                if (!string.IsNullOrWhiteSpace(extension))
                {
                    var fileTypeFolder = extension.TrimStart('.').ToUpperInvariant();
                    if (!string.IsNullOrWhiteSpace(fileTypeFolder))
                    {
                        targetDirectory = Path.Combine(targetDirectory, fileTypeFolder);
                    }
                }
            }

            return targetDirectory; // Return the resolved destination directory
        }

        /// <summary>
        /// Returns the English month name for a given month number.
        /// </summary>
        private string GetMonthName(int month) // Maps numeric month values to names
        {
            return month switch // Pattern-match month integer to its name
            {
                1 => "January", // Month 1
                2 => "February", // Month 2
                3 => "March", // Month 3
                4 => "April", // Month 4
                5 => "May", // Month 5
                6 => "June", // Month 6
                7 => "July", // Month 7
                8 => "August", // Month 8
                9 => "September", // Month 9
                10 => "October", // Month 10
                11 => "November", // Month 11
                12 => "December", // Month 12
                _ => "Unknown" // Fallback for invalid month numbers
            }; // End of switch expression
        } // End of GetMonthName

        /// <summary>
        /// Ensures a unique file path by appending a counter if the file already exists.
        /// E.g., "path/file.txt" becomes "path/file (1).txt", "path/file (2).txt", etc.
        /// </summary>
        private string EnsureUniquePath(string destPath, int maxAttempts = 1000)
        {
            if (!File.Exists(destPath))
                return destPath;

            var dir = Path.GetDirectoryName(destPath) ?? string.Empty;
            var fileName = Path.GetFileNameWithoutExtension(destPath);
            var ext = Path.GetExtension(destPath);

            for (int i = 1; i <= maxAttempts; i++)
            {
                var newPath = Path.Combine(dir, $"{fileName} ({i}){ext}");
                if (!File.Exists(newPath))
                    return newPath;
            }

            // Fallback: use GUID if all attempts fail
            return Path.Combine(dir, $"{fileName}_{Guid.NewGuid():N}{ext}");
        }
    } // End of ImportService class
} // End of RawImporterCS namespace
