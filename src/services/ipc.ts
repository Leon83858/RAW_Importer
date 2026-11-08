// Typed wrappers around the Tauri commands. All DTOs come from
// `types/generated` (ts-rs); only the Tauri-local SettingsResponse is
// declared here because it lives in the app crate, not in the core.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FormatDescriptor } from "../types/generated/FormatDescriptor";
import type { Language } from "../types/generated/Language";
import type { ThemeMode } from "../types/generated/ThemeMode";
import type { UpdateStatus } from "../types/generated/UpdateStatus";
import type { PathIssue } from "../types/generated/PathIssue";
import type { ScanPreview } from "../types/generated/ScanPreview";
import type { SessionError } from "../types/generated/SessionError";
import type { SessionSnapshot } from "../types/generated/SessionSnapshot";
import type { Settings } from "../types/generated/Settings";
import type { ValidatedPaths } from "../types/generated/ValidatedPaths";

export type SettingsResponse = {
  settings: Settings;
  migrated_from_2x: boolean;
  warnings: string[];
  app_version: string;
};

export const PROGRESS_EVENT = "import-progress";
export const MENU_EVENT = "menu-action";

/** Native menu actions that are handled by the frontend window. */
export type MenuAction = { kind: "open_settings" };

export const ipc = {
  formatRegistry: () => invoke<FormatDescriptor[]>("format_registry"),
  loadSettings: () => invoke<SettingsResponse>("load_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  validatePaths: (source: string, destination: string) =>
    invoke<ValidatedPaths>("validate_paths", { source, destination }),
  scan: (source: string, destination: string, settings: Settings) =>
    invoke<ScanPreview>("scan", { source, destination, settings }),
  startImport: () => invoke<string>("start_import"),
  sessionState: () => invoke<SessionSnapshot>("session_state"),
  pause: () => invoke<SessionSnapshot>("pause_import"),
  resume: () => invoke<SessionSnapshot>("resume_import"),
  cancel: () => invoke<SessionSnapshot>("cancel_session"),
  reset: () => invoke<SessionSnapshot>("reset_session"),
  applyUi: (language: Language, theme: ThemeMode) => invoke<void>("apply_ui", { language, theme }),
  checkForUpdates: () => invoke<UpdateStatus>("check_for_updates"),
  onMenu: (handler: (action: MenuAction) => void): Promise<UnlistenFn> =>
    listen<MenuAction>(MENU_EVENT, (event) => handler(event.payload)),
  onProgress: (handler: (snapshot: SessionSnapshot) => void): Promise<UnlistenFn> =>
    listen<SessionSnapshot>(PROGRESS_EVENT, (event) => handler(event.payload)),
};

export function isSessionError(value: unknown): value is SessionError {
  return typeof value === "object" && value !== null && "code" in value && "message" in value;
}

export function isPathIssueList(value: unknown): value is PathIssue[] {
  return Array.isArray(value) && value.every((v) => typeof v === "object" && v !== null && "code" in v);
}
