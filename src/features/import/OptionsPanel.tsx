import { useState, type ReactNode } from "react";
import type { ConflictPolicy } from "../../types/generated/ConflictPolicy";
import type { FormatDescriptor } from "../../types/generated/FormatDescriptor";
import type { FormatId } from "../../types/generated/FormatId";
import type { Layout } from "../../types/generated/Layout";
import type { Settings } from "../../types/generated/Settings";
import { layoutExample } from "../../i18n/catalogues";
import { useI18n } from "../../i18n/context";

const LAYOUTS: Layout[] = ["year_month_day", "year_month", "year", "day_month_year", "flat"];
const CONFLICTS: ConflictPolicy[] = ["skip", "rename", "skip_identical"];
const MANUAL_EXTENSION_PATTERN = /^[a-z0-9_-]{1,16}$/;

function normalizeManualExtension(input: string): string | null {
  const trimmed = input.trim();
  const withoutDot = trimmed.startsWith(".") ? trimmed.slice(1) : trimmed;
  const normalized = withoutDot.toLowerCase();
  return MANUAL_EXTENSION_PATTERN.test(normalized) ? normalized : null;
}

type Props = {
  settings: Settings;
  registry: FormatDescriptor[];
  disabled: boolean;
  onChange: (next: Settings) => void;
};

export function Section({ title, hint, children }: { title: string; hint: string; children: ReactNode }) {
  return (
    <section className="form-section">
      <header className="form-section-head">
        <h3>{title}</h3>
        <p>{hint}</p>
      </header>
      <div className="form-group">{children}</div>
    </section>
  );
}

export function Row({ label, htmlFor, children }: { label: string; htmlFor?: string; children: ReactNode }) {
  return (
    <div className="form-row">
      {htmlFor ? <label htmlFor={htmlFor} className="form-label">{label}</label> : <span className="form-label">{label}</span>}
      <div className="form-control">{children}</div>
    </div>
  );
}

function Toggle({ checked, label, onChange }: { checked: boolean; label: string; onChange: (v: boolean) => void }) {
  return (
    <label className="toggle">
      <input type="checkbox" role="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>{label}</span>
    </label>
  );
}

export function OptionsPanel({ settings, registry, disabled, onChange }: Props) {
  const { t } = useI18n();
  const [customExtension, setCustomExtension] = useState("");
  const [customExtensionError, setCustomExtensionError] = useState<string | null>(null);
  const enabled = new Set(settings.formats.enabled);
  const exifFirst = settings.date_policy[0] === "exif_original";
  const raw = registry.filter((f) => f.raw);
  const images = registry.filter((f) => !f.raw);
  const all = settings.formats.include_all;

  function setFormats(ids: Set<FormatId>) {
    onChange({ ...settings, formats: { ...settings.formats, enabled: [...ids] } });
  }
  function toggle(id: FormatId) {
    const next = new Set(enabled);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setFormats(next);
  }
  function setGroup(group: FormatDescriptor[], on: boolean) {
    const next = new Set(enabled);
    group.forEach((f) => (on ? next.add(f.id) : next.delete(f.id)));
    setFormats(next);
  }
  function addCustomExtension() {
    const extension = normalizeManualExtension(customExtension);
    if (!extension) {
      setCustomExtensionError(t.customExtensionInvalid);
      return;
    }
    if (settings.formats.manual.some((existing) => existing.toLowerCase() === extension)) {
      setCustomExtensionError(t.customExtensionExists);
      return;
    }
    onChange({
      ...settings,
      formats: { ...settings.formats, manual: [...settings.formats.manual, extension].sort() },
    });
    setCustomExtension("");
    setCustomExtensionError(null);
  }
  function removeCustomExtension(extension: string) {
    onChange({
      ...settings,
      formats: { ...settings.formats, manual: settings.formats.manual.filter((value) => value !== extension) },
    });
  }

  const chips = (group: FormatDescriptor[]) => {
    const allOn = group.every((f) => enabled.has(f.id));
    return (
      <div className="chip-row">
        {group.map((f) => (
          <button
            key={f.id}
            type="button"
            className="chip"
            aria-pressed={enabled.has(f.id)}
            title={f.vendor ?? f.extensions.map((e) => `.${e}`).join(", ")}
            onClick={() => toggle(f.id)}
          >
            {f.extensions[0].toUpperCase()}
          </button>
        ))}
        <button type="button" className="link-button" onClick={() => setGroup(group, !allOn)}>
          {allOn ? t.selectNone : t.selectAll}
        </button>
      </div>
    );
  };

  return (
    <fieldset className="options" disabled={disabled}>
      <Section title={t.filesTitle} hint={t.filesHint}>
        <Row label={t.raw}>{all ? <span className="muted">{t.allFilesSelected}</span> : chips(raw)}</Row>
        <Row label={t.images}>{all ? <span className="muted">{t.allFilesSelected}</span> : chips(images)}</Row>
        <Row label={t.customExtensions} htmlFor="custom-extension">
          <div className="custom-format-control">
            <div className="custom-format-entry">
              <input
                id="custom-extension"
                value={customExtension}
                placeholder={t.customExtensionPlaceholder}
                aria-invalid={customExtensionError !== null}
                aria-describedby={customExtensionError ? "custom-extension-error" : undefined}
                onChange={(event) => {
                  setCustomExtension(event.target.value);
                  setCustomExtensionError(null);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    addCustomExtension();
                  }
                }}
              />
              <button
                type="button"
                className="secondary-button"
                disabled={!customExtension.trim()}
                onClick={addCustomExtension}
              >
                {t.addCustomExtension}
              </button>
            </div>
            {customExtensionError && <span id="custom-extension-error" className="custom-format-error" role="alert">{customExtensionError}</span>}
            {settings.formats.manual.length > 0 && (
              <div className="chip-row">
                {settings.formats.manual.map((extension) => (
                  <button
                    key={extension}
                    type="button"
                    className="chip"
                    aria-label={t.removeCustomExtension(extension)}
                    onClick={() => removeCustomExtension(extension)}
                  >
                    .{extension} x
                  </button>
                ))}
              </div>
            )}
          </div>
        </Row>
        <Row label={t.other}>
          <Toggle
            checked={all}
            label={t.allFilesToggle}
            onChange={(v) => onChange({ ...settings, formats: { ...settings.formats, include_all: v } })}
          />
        </Row>
      </Section>

      <Section title={t.storageTitle} hint={t.storageHint}>
        <Row label={t.folderStructure} htmlFor="layout">
          <select
            id="layout"
            value={settings.structure.layout}
            onChange={(e) =>
              onChange({ ...settings, structure: { ...settings.structure, layout: e.target.value as Layout } })
            }
          >
            {LAYOUTS.map((l) => (
              <option key={l} value={l}>{t.layouts[l]}</option>
            ))}
          </select>
          <span className="example">
            {t.example}{" "}
            <code>
              {settings.structure.layout === "flat" ? t.layoutFlatExample : layoutExample[settings.structure.layout]}
            </code>
          </span>
        </Row>
        <Row label={t.dateFrom} htmlFor="date">
          <select
            id="date"
            value={exifFirst ? "exif" : "fs"}
            onChange={(e) =>
              onChange({
                ...settings,
                date_policy:
                  e.target.value === "exif"
                    ? ["exif_original", "exif_digitized", "filesystem_modified"]
                    : ["filesystem_modified"],
              })
            }
          >
            <option value="fs">{t.dateFilesystem}</option>
            <option value="exif">{t.dateExif}</option>
          </select>
        </Row>
        <Row label={t.subfolders}>
          <div className="toggle-stack">
            <Toggle
              checked={settings.structure.preserve_subfolders}
              label={t.keepSourceStructure}
              onChange={(v) => onChange({ ...settings, structure: { ...settings.structure, preserve_subfolders: v } })}
            />
            <Toggle
              checked={settings.structure.type_folders}
              label={t.typeFolders}
              onChange={(v) => onChange({ ...settings, structure: { ...settings.structure, type_folders: v } })}
            />
          </div>
        </Row>
      </Section>

      <details className="advanced">
        <summary>{t.advanced}</summary>
        <div className="form-group">
          <Row label={t.conflicts} htmlFor="conflict">
            <select
              id="conflict"
              value={settings.conflict}
              onChange={(e) => onChange({ ...settings, conflict: e.target.value as ConflictPolicy })}
            >
              {CONFLICTS.map((c) => (
                <option key={c} value={c}>{t.conflictPolicies[c]}</option>
              ))}
            </select>
          </Row>
          <Row label={t.concurrency} htmlFor="parallel">
            <select
              id="parallel"
              value={settings.parallelism.mode}
              onChange={(e) =>
                onChange({
                  ...settings,
                  parallelism:
                    e.target.value === "manual"
                      ? { mode: "manual", workers: 4 }
                      : { mode: e.target.value as "sequential" | "auto" },
                })
              }
            >
              <option value="sequential">{t.concurrency1}</option>
              <option value="auto">{t.concurrency2}</option>
              <option value="manual">{t.concurrency4}</option>
            </select>
          </Row>
        </div>
      </details>
    </fieldset>
  );
}
