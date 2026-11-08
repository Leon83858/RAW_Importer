import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { FormatDescriptor } from "../types/generated/FormatDescriptor";
import type { Language } from "../types/generated/Language";
import type { PathIssue } from "../types/generated/PathIssue";
import type { ScanPreview } from "../types/generated/ScanPreview";
import type { SessionSnapshot } from "../types/generated/SessionSnapshot";
import type { Settings } from "../types/generated/Settings";
import type { ThemeMode } from "../types/generated/ThemeMode";
import type { UpdateStatus } from "../types/generated/UpdateStatus";
import { ipc, isPathIssueList, isSessionError } from "../services/ipc";
import { catalogues, localeTag, type Messages } from "../i18n/catalogues";
import { I18nProvider } from "../i18n/context";
import { formatBytes } from "../lib/format";
import { OptionsPanel, Row, Section } from "../features/import/OptionsPanel";
import { ProgressView } from "../features/import/ProgressView";

type Step = "setup" | "preview" | "import";
type Folder = "source" | "destination";
const STEP_IDS: Step[] = ["setup", "preview", "import"];
const RUNNING = ["preparing_import", "importing", "pausing", "paused", "cancelling"];
const LANGUAGE_OPTIONS: Array<{ value: Language; label: string }> = [
  { value: "english", label: "English (UK)" },
  { value: "german", label: "Deutsch" },
  { value: "french", label: "Français" },
  { value: "dutch", label: "Nederlands" },
  { value: "dutch_belgium", label: "Nederlands (België)" },
];

type Problem = { text: string; issues: PathIssue[] };

function describe(error: unknown, t: Messages): Problem {
  if (isSessionError(error)) return { text: t.errors[error.code] ?? error.message, issues: error.issues };
  if (isPathIssueList(error)) return { text: t.pathCheckFailed, issues: error };
  return { text: String(error), issues: [] };
}

function applyTheme(theme: ThemeMode) {
  document.documentElement.dataset.theme = theme;
}

function App() {
  const [registry, setRegistry] = useState<FormatDescriptor[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [folders, setFolders] = useState<Record<Folder, string>>({ source: "", destination: "" });
  const [step, setStep] = useState<Step>("setup");
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<ScanPreview | null>(null);
  const [snapshot, setSnapshot] = useState<SessionSnapshot | null>(null);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [migrated, setMigrated] = useState(false);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [appVersion, setAppVersion] = useState("");
  const [updatePromptOpen, setUpdatePromptOpen] = useState(false);
  const [updatePromptAutoCheck, setUpdatePromptAutoCheck] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updateCheckFailed, setUpdateCheckFailed] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const lastSeq = useRef(0);
  const settingsRef = useRef<Settings | null>(null);

  const language: Language = settings?.ui.language ?? "english";
  const t = catalogues[language];
  const locale = localeTag[language];

  // Keep only the newest snapshot: events and command replies may overtake each other.
  const accept = useCallback((snap: SessionSnapshot) => {
    if (snap.seq <= lastSeq.current) return;
    lastSeq.current = snap.seq;
    setSnapshot(snap);
  }, []);

  /** Persist language/theme changes and re-label the native Settings menu item. */
  const changeUi = useCallback(async (patch: Partial<Settings["ui"]>) => {
    const current = settingsRef.current;
    if (!current) return;
    const next: Settings = { ...current, ui: { ...current.ui, ...patch } };
    settingsRef.current = next;
    setSettings(next);
    applyTheme(next.ui.theme);
    await ipc.saveSettings(next);
    await ipc.applyUi(next.ui.language, next.ui.theme);
  }, []);

  const checkForUpdates = useCallback(async () => {
    setCheckingUpdates(true);
    setUpdateCheckFailed(false);
    try {
      setUpdateStatus(await ipc.checkForUpdates());
    } catch {
      setUpdateCheckFailed(true);
    } finally {
      setCheckingUpdates(false);
    }
  }, []);

  const saveUpdatePreference = useCallback(
    async (checkOnStart: boolean, closePrompt: boolean) => {
      const current = settingsRef.current;
      if (!current || !appVersion) return;
      const next: Settings = {
        ...current,
        updates: {
          ...current.updates,
          check_on_start: checkOnStart,
          prompted_for_version: appVersion,
        },
      };
      settingsRef.current = next;
      setSettings(next);
      try {
        await ipc.saveSettings(next);
        if (closePrompt) setUpdatePromptOpen(false);
        if (checkOnStart) void checkForUpdates();
      } catch (error) {
        setProblem(describe(error, t));
      }
    },
    [appVersion, checkForUpdates, t],
  );

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    void ipc.onProgress(accept).then((fn) => unlisteners.push(fn));
    void ipc
      .onMenu((action) => {
        if (action.kind === "open_settings") setSettingsOpen(true);
      })
      .then((fn) => unlisteners.push(fn));
    void Promise.all([ipc.formatRegistry(), ipc.loadSettings(), ipc.sessionState()])
      .then(async ([formats, loaded, state]) => {
        setRegistry(formats);
        settingsRef.current = loaded.settings;
        setSettings(loaded.settings);
        setAppVersion(loaded.app_version);
        applyTheme(loaded.settings.ui.theme);
        await ipc.applyUi(loaded.settings.ui.language, loaded.settings.ui.theme);
        setFolders({
          source: loaded.settings.paths.source ?? "",
          destination: loaded.settings.paths.destination ?? "",
        });
        setMigrated(loaded.migrated_from_2x);
        setWarnings(loaded.warnings);
        if (loaded.settings.updates.prompted_for_version !== loaded.app_version) {
          setUpdatePromptAutoCheck(loaded.settings.updates.check_on_start);
          setUpdatePromptOpen(true);
        } else if (loaded.settings.updates.check_on_start) {
          void checkForUpdates();
        }
        accept(state);
        if (RUNNING.includes(state.state)) setStep("import");
      })
      .catch((e) => setProblem(describe(e, catalogues.english)));
    return () => unlisteners.forEach((fn) => fn());
  }, [accept, changeUi, checkForUpdates]);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  function updateSettings(next: Settings) {
    settingsRef.current = next;
    setSettings(next);
    setPreview(null);
  }

  async function choose(field: Folder) {
    const selection = await open({ directory: true, multiple: false });
    if (typeof selection === "string") {
      setFolders((f) => ({ ...f, [field]: selection }));
      setPreview(null);
      setProblem(null);
    }
  }

  async function scan() {
    if (!settings) return;
    setBusy(true);
    setProblem(null);
    const next: Settings = { ...settings, paths: { source: folders.source, destination: folders.destination } };
    try {
      await ipc.saveSettings(next);
      updateSettings(next);
      setPreview(await ipc.scan(folders.source, folders.destination, next));
      setStep("preview");
    } catch (e) {
      setProblem(describe(e, t));
    } finally {
      setBusy(false);
      accept(await ipc.sessionState());
    }
  }

  async function run<T>(action: () => Promise<T>, after?: (value: T) => void) {
    try {
      const value = await action();
      after?.(value);
    } catch (e) {
      setProblem(describe(e, t));
    }
  }

  async function startImport() {
    setProblem(null);
    await run(ipc.startImport, () => setStep("import"));
  }

  async function finish() {
    await run(ipc.reset, (snap) => {
      accept(snap);
      setPreview(null);
      setStep("setup");
    });
  }

  async function backToSetup() {
    await run(ipc.reset, accept);
    setStep("setup");
  }

  const stepIndex = STEP_IDS.indexOf(step);
  const formatCount = settings ? registry.filter((f) => settings.formats.enabled.includes(f.id)).length : 0;
  const formatsChosen = !!settings && (settings.formats.include_all || formatCount > 0);
  const bytes = (n: number) => formatBytes(n, locale);
  const notice = [migrated ? t.migrated : null, ...warnings].filter(Boolean).join(" ");
  const updateMessage = checkingUpdates
    ? t.updateChecking
    : updateCheckFailed
      ? t.updateFailed
      : updateStatus?.status === "update_available"
        ? t.updateAvailable(updateStatus.latest_version)
        : updateStatus?.status === "up_to_date"
          ? t.updateCurrent(updateStatus.current_version)
          : updateStatus?.status === "local_version_ahead"
            ? t.updateLocalAhead(updateStatus.current_version, updateStatus.latest_version)
            : updateStatus?.status === "no_stable_release"
              ? t.updateNoRelease
              : "";

  return (
    <I18nProvider language={language}>
      <main className="app-shell">
        <header className="topbar">
          <div className="brand-mark" aria-hidden="true">R</div>
          <div>
            <p className="eyebrow">RAW IMPORTER</p>
            <h1>{t.appTitle}</h1>
          </div>
          <span className="version">3.0</span>
        </header>

        <section className="workspace" aria-labelledby="step-heading">
          <div className="section-heading">
            <h2 id="step-heading">{t.stepTitles[step]}</h2>
            <ol className="stepper" aria-label={t.stepsAria}>
              {STEP_IDS.map((id, i) => (
                <li
                  key={id}
                  className={i === stepIndex ? "current" : i < stepIndex ? "done" : undefined}
                  aria-current={i === stepIndex ? "step" : undefined}
                >
                  <span>{i + 1}</span>
                  {t.steps[id]}
                </li>
              ))}
            </ol>
          </div>

          {notice && <div className="result result-success" role="status">{notice}</div>}

          {step === "setup" && settings && (
            <>
              <Section title={t.foldersTitle} hint={t.foldersHint}>
                {(["source", "destination"] as const).map((field) => (
                  <Row key={field} label={t[field]} htmlFor={field}>
                    <div className="path-field">
                      <input
                        id={field}
                        value={folders[field]}
                        onChange={(e) => {
                          setFolders((f) => ({ ...f, [field]: e.target.value }));
                          setPreview(null);
                        }}
                        placeholder={field === "source" ? t.sourcePlaceholder : t.destinationPlaceholder}
                        spellCheck={false}
                        disabled={busy}
                        title={folders[field]}
                      />
                      <button className="secondary-button" onClick={() => void choose(field)} disabled={busy}>
                        {t.choose}
                      </button>
                    </div>
                  </Row>
                ))}
              </Section>
              <OptionsPanel settings={settings} registry={registry} disabled={busy} onChange={updateSettings} />
              <div className="action-row">
                <p className="helper-text summary">
                  {!formatsChosen
                    ? t.chooseOneType
                    : `${settings.formats.include_all ? t.allFiles : t.typeCount(formatCount)} · ${t.layouts[settings.structure.layout]}`}
                </p>
                {busy && (
                  <button className="danger-button" onClick={() => void run(ipc.cancel, accept)}>
                    {t.cancelScan}
                  </button>
                )}
                <button
                  className="primary-button"
                  disabled={busy || !formatsChosen || !folders.source || !folders.destination}
                  onClick={() => void scan()}
                >
                  {busy ? t.scanning : t.createPreview}
                </button>
              </div>
            </>
          )}

          {step === "preview" && preview && (
            <>
              <div className="result result-success" role="status">
                <div>
                  <strong>{t.previewSummary(preview.files.length, bytes(preview.total_bytes))}</strong>
                  <p>
                    {t.previewDetails({
                      filtered: preview.filtered_out,
                      hidden: preview.hidden_skipped,
                      symlinks: preview.symlinks_skipped,
                      unreadable: preview.unreadable.length,
                      missingDate: preview.missing_date_count,
                      willCreate: preview.destination_will_be_created,
                    })}
                  </p>
                </div>
              </div>
              {preview.missing_date_count > 0 && (
                <div className="result result-error" role="alert">{t.missingDateWarning(preview.missing_date_count)}</div>
              )}
              <ul className="preview-list" aria-label={t.plannedDestinations}>
                {preview.files.slice(0, 200).map((f) => (
                  <li key={f.source}>
                    <span title={f.source}>{f.source}</span>
                    <span>
                      {f.relative_destination ?? t.noDate}
                      {f.date_fallback && f.relative_destination ? ` ${t.fallbackDate}` : ""}
                    </span>
                  </li>
                ))}
                {preview.files.length > 200 && <li>{t.andMore(preview.files.length - 200)}</li>}
              </ul>
              <div className="action-row">
                <p className="helper-text">{t.checkBeforeStart}</p>
                <button className="secondary-button" onClick={() => void backToSetup()}>{t.back}</button>
                <button className="primary-button" disabled={preview.files.length === 0} onClick={() => void startImport()}>
                  {t.startImport}
                </button>
              </div>
            </>
          )}

          {step === "import" && snapshot && (
            <ProgressView
              snapshot={snapshot}
              onPause={() => void run(ipc.pause, accept)}
              onResume={() => void run(ipc.resume, accept)}
              onCancel={() => void run(ipc.cancel, accept)}
              onDone={() => void finish()}
            />
          )}

          {problem && (
            <div className="result result-error" role="alert">
              <strong>{problem.text}</strong>
              {problem.issues.length > 0 && (
                <ul>
                  {problem.issues.map((i, n) => (
                    <li key={`${i.code}-${n}`}>
                      {t.errors[i.code]} <code>{i.path}</code>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </section>

        <footer className="statusbar">
          <span>{t.footerLocal}</span>
          <span>{t.footerSafe}</span>
        </footer>

        {settingsOpen && settings && (
          <div className="settings-dialog-backdrop">
            <section
              className="settings-dialog"
              role="dialog"
              aria-modal="true"
              aria-labelledby="settings-title"
            >
              <header className="settings-dialog-header">
                <div>
                  <p className="eyebrow">RAW IMPORTER {appVersion}</p>
                  <h2 id="settings-title">{t.settingsTitle}</h2>
                </div>
                <button className="secondary-button settings-close-button" onClick={() => setSettingsOpen(false)}>
                  {t.settingsClose}
                </button>
              </header>

              <section className="settings-group" aria-labelledby="settings-language-heading">
                <h3 id="settings-language-heading">{t.settingsLanguage}</h3>
                <label className="settings-row">
                  <span>{t.settingsLanguageLabel}</span>
                  <select
                    value={settings.ui.language}
                    onChange={(event) => void changeUi({ language: event.target.value as Language })}
                  >
                    {LANGUAGE_OPTIONS.map((option) => (
                      <option key={option.value} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
              </section>

              <section className="settings-group" aria-labelledby="settings-appearance-heading">
                <h3 id="settings-appearance-heading">{t.settingsAppearance}</h3>
                <label className="settings-row">
                  <span>{t.settingsAppearanceLabel}</span>
                  <select
                    value={settings.ui.theme}
                    onChange={(event) => void changeUi({ theme: event.target.value as ThemeMode })}
                  >
                    <option value="system">{t.themeSystem}</option>
                    <option value="light">{t.themeLight}</option>
                    <option value="dark">{t.themeDark}</option>
                  </select>
                </label>
              </section>

              <section className="settings-group" aria-labelledby="settings-updates-heading">
                <h3 id="settings-updates-heading">{t.settingsUpdates}</h3>
                <label className="settings-row settings-toggle-row">
                  <span>{t.updateAutoCheck}</span>
                  <input
                    type="checkbox"
                    checked={settings.updates.check_on_start}
                    disabled={checkingUpdates}
                    onChange={(event) => void saveUpdatePreference(event.target.checked, false)}
                  />
                </label>
                <div className="settings-update-status" aria-live="polite">
                  {updateMessage && <span role="status">{updateMessage}</span>}
                  <button
                    className="secondary-button"
                    disabled={checkingUpdates}
                    onClick={() => void checkForUpdates()}
                  >
                    {checkingUpdates ? t.updateChecking : t.updateCheck}
                  </button>
                  {updateStatus?.status === "update_available" && (
                    <button
                      className="text-button"
                      onClick={() => void openUrl(updateStatus.release_url).catch(() => setUpdateCheckFailed(true))}
                    >
                      {t.openRelease}
                    </button>
                  )}
                </div>
              </section>

              <section className="settings-group" aria-labelledby="settings-credits-heading">
                <h3 id="settings-credits-heading">{t.settingsCredits}</h3>
                <div className="settings-row">
                  <button
                    className="text-button"
                    onClick={() => void openUrl("https://www.instagram.com/thelomphotography/").catch(() => setProblem({ text: t.creditsOpenFailed, issues: [] }))}
                  >
                    {t.creditsLink}
                  </button>
                </div>
              </section>
            </section>
          </div>
        )}

        {updatePromptOpen && settings && (
          <div className="update-dialog-backdrop">
            <section
              className="update-dialog"
              role="dialog"
              aria-modal="true"
              aria-labelledby="update-prompt-title"
              aria-describedby="update-prompt-description"
            >
              <p className="eyebrow">RAW IMPORTER {appVersion}</p>
              <h2 id="update-prompt-title">{t.updatePromptTitle}</h2>
              <p id="update-prompt-description" className="update-dialog-copy">
                {t.updatePromptBody}
              </p>
              <label className="update-auto-check update-prompt-check">
                <input
                  type="checkbox"
                  checked={updatePromptAutoCheck}
                  onChange={(event) => setUpdatePromptAutoCheck(event.target.checked)}
                />
                {t.updateAutoCheck}
              </label>
              <div className="update-dialog-actions">
                <button
                  className="secondary-button"
                  onClick={() => void saveUpdatePreference(false, true)}
                >
                  {t.updatePromptNotNow}
                </button>
                <button
                  className="primary-button"
                  onClick={() => void saveUpdatePreference(updatePromptAutoCheck, true)}
                >
                  {t.updatePromptSave}
                </button>
              </div>
            </section>
          </div>
        )}
      </main>
    </I18nProvider>
  );
}

export default App;
