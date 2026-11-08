import type { SessionSnapshot } from "../../types/generated/SessionSnapshot";
import { useI18n } from "../../i18n/context";
import { fileName, formatBytes, formatDuration } from "../../lib/format";

type Props = {
  snapshot: SessionSnapshot;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onDone: () => void;
};

const RUNNING = new Set(["preparing_import", "importing", "pausing", "paused", "cancelling"]);

export function ProgressView({ snapshot, onPause, onResume, onCancel, onDone }: Props) {
  const { t, locale } = useI18n();
  const bytes = (n: number) => formatBytes(n, locale);
  const p = snapshot.result?.progress ?? snapshot.progress;
  const running = RUNNING.has(snapshot.state);
  const percent = p && p.bytes_total > 0 ? Math.min(100, (p.bytes_done / p.bytes_total) * 100) : 0;

  return (
    <div className="progress-view">
      <div className="progress-head">
        <strong>{t.states[snapshot.state]}</strong>
        {p && <span>{t.filesProgress(p.files_done, p.files_total)}</span>}
      </div>

      <div
        className="progress-track"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(percent)}
        aria-label={t.progressAria}
      >
        <div className="progress-fill" style={{ width: `${percent}%` }} />
      </div>

      {p && (
        <dl className="stats">
          <div><dt>{t.statImported}</dt><dd>{p.imported}</dd></div>
          <div><dt>{t.statSkipped}</dt><dd>{p.skipped}</dd></div>
          <div><dt>{t.statFailed}</dt><dd className={p.failed > 0 ? "bad" : undefined}>{p.failed}</dd></div>
          <div><dt>{t.statData}</dt><dd>{bytes(p.bytes_done)} / {bytes(p.bytes_total)}</dd></div>
          <div><dt>{t.statThroughput}</dt><dd>{p.bytes_per_second != null ? `${bytes(p.bytes_per_second)}/s` : "–"}</dd></div>
          <div><dt>{t.statRemaining}</dt><dd>{running && p.eta_ms != null ? t.approx(formatDuration(p.eta_ms)) : "–"}</dd></div>
        </dl>
      )}

      {running && p?.current_file && (
        <p className="helper-text current-file" title={p.current_file}>{t.currentFile(fileName(p.current_file))}</p>
      )}

      {snapshot.error && (
        <div className="result result-error" role="alert">
          <strong>{t.errors[snapshot.error.code]}</strong>
          <p>{snapshot.error.message}</p>
        </div>
      )}

      {snapshot.result && (
        <div className={`result ${snapshot.result.failures.length || snapshot.result.history_error ? "result-error" : "result-success"}`} role="status">
          <div>
            <strong>
              {snapshot.result.end === "cancelled"
                ? t.resultCancelled
                : snapshot.result.failures.length
                  ? t.resultFailed(snapshot.result.failures.length)
                  : t.resultDone}
            </strong>
            {snapshot.result.history_error && <p>{t.historyError(snapshot.result.history_error)}</p>}
            {snapshot.result.failures.length > 0 && (
              <ul className="failure-list">
                {snapshot.result.failures.slice(0, 50).map((f) => (
                  <li key={f.source} title={f.detail}>
                    <span>{fileName(f.source)}</span>
                    <span>{t.errors[f.code]}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}

      <div className="action-row">
        <p className="helper-text">
          {running ? t.safeHint : t.sessionId + (snapshot.result?.session_id ?? "–")}
        </p>
        {snapshot.state === "importing" && <button className="secondary-button" onClick={onPause}>{t.pause}</button>}
        {snapshot.state === "paused" && <button className="secondary-button" onClick={onResume}>{t.resume}</button>}
        {running && snapshot.state !== "cancelling" && (
          <button className="danger-button" onClick={onCancel}>{t.cancel}</button>
        )}
        {!running && <button className="primary-button" onClick={onDone}>{t.newImport}</button>}
      </div>
    </div>
  );
}
