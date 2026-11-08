import type { ConflictPolicy } from "../types/generated/ConflictPolicy";
import type { ErrorCode } from "../types/generated/ErrorCode";
import type { ImportState } from "../types/generated/ImportState";
import type { Layout } from "../types/generated/Layout";

export type PreviewCounts = {
  filtered: number;
  hidden: number;
  symlinks: number;
  unreadable: number;
  missingDate: number;
  willCreate: boolean;
};

/** Every UI string. A missing key in any language is a compile error. */
export interface Messages {
  appTitle: string;
  stepsAria: string;
  steps: { setup: string; preview: string; import: string };
  stepTitles: { setup: string; preview: string; import: string };
  footerLocal: string;
  footerSafe: string;
  updateCheck: string;
  updateChecking: string;
  updateAvailable: (version: string) => string;
  updateCurrent: (version: string) => string;
  updateLocalAhead: (current: string, latest: string) => string;
  updateNoRelease: string;
  updateFailed: string;
  openRelease: string;
  updateAutoCheck: string;
  updatePromptTitle: string;
  updatePromptBody: string;
  updatePromptSave: string;
  updatePromptNotNow: string;

  settingsTitle: string;
  settingsClose: string;
  settingsLanguage: string;
  settingsLanguageLabel: string;
  settingsAppearance: string;
  settingsAppearanceLabel: string;
  themeSystem: string;
  themeLight: string;
  themeDark: string;
  settingsCredits: string;
  creditsLink: string;
  creditsOpenFailed: string;
  settingsUpdates: string;

  foldersTitle: string;
  foldersHint: string;
  source: string;
  destination: string;
  sourcePlaceholder: string;
  destinationPlaceholder: string;
  choose: string;

  filesTitle: string;
  filesHint: string;
  raw: string;
  images: string;
  other: string;
  allFilesSelected: string;
  allFilesToggle: string;
  selectAll: string;
  selectNone: string;
  customExtensions: string;
  customExtensionPlaceholder: string;
  addCustomExtension: string;
  customExtensionInvalid: string;
  customExtensionExists: string;
  removeCustomExtension: (extension: string) => string;

  storageTitle: string;
  storageHint: string;
  folderStructure: string;
  example: string;
  dateFrom: string;
  dateFilesystem: string;
  dateExif: string;
  subfolders: string;
  keepSourceStructure: string;
  typeFolders: string;

  advanced: string;
  conflicts: string;
  concurrency: string;
  concurrency1: string;
  concurrency2: string;
  concurrency4: string;

  chooseOneType: string;
  allFiles: string;
  typeCount: (n: number) => string;

  createPreview: string;
  scanning: string;
  cancelScan: string;
  back: string;
  startImport: string;
  pause: string;
  resume: string;
  cancel: string;
  newImport: string;

  previewSummary: (files: number, size: string) => string;
  previewDetails: (c: PreviewCounts) => string;
  missingDateWarning: (n: number) => string;
  checkBeforeStart: string;
  noDate: string;
  fallbackDate: string;
  andMore: (n: number) => string;
  plannedDestinations: string;

  migrated: string;
  pathCheckFailed: string;

  filesProgress: (done: number, total: number) => string;
  statImported: string;
  statSkipped: string;
  statFailed: string;
  statData: string;
  statThroughput: string;
  statRemaining: string;
  approx: (duration: string) => string;
  currentFile: (name: string) => string;
  progressAria: string;
  safeHint: string;
  sessionId: string;
  resultCancelled: string;
  resultFailed: (n: number) => string;
  resultDone: string;
  historyError: (message: string) => string;

  errors: Record<ErrorCode, string>;
  states: Record<ImportState, string>;
  layouts: Record<Layout, string>;
  layoutFlatExample: string;
  conflictPolicies: Record<ConflictPolicy, string>;
}
