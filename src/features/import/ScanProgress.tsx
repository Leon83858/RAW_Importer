import type { ScanStatus } from "../../types/generated/ScanStatus";
import { useI18n } from "../../i18n/context";

type Props = {
  /** `null` until the first report of the running scan has arrived. */
  status: ScanStatus | null;
};

/**
 * Shows that a scan is still working. While the source folder is listed the
 * total is unknown, so the bar moves without a percentage; while capture
 * dates are read the total is known and the bar fills up.
 */
export function ScanProgress({ status }: Props) {
  const { t, locale } = useI18n();
  const readingDates = status?.phase === "reading_dates";
  const fraction =
    readingDates && status.dates_total > 0 ? Math.min(1, status.dates_done / status.dates_total) : null;
  const text = readingDates
    ? t.scanReadingDates(status.dates_done, status.dates_total)
    : t.scanListing(status?.files_found ?? 0, status?.entries_seen ?? 0);

  return (
    <div className="scan-progress" role="status" aria-live="polite">
      <div className="progress-head">
        <span>{text}</span>
        {fraction != null && (
          <span>{fraction.toLocaleString(locale, { style: "percent", maximumFractionDigits: 0 })}</span>
        )}
      </div>
      <div
        className="progress-track"
        role="progressbar"
        aria-label={t.scanProgressAria}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={fraction != null ? Math.round(fraction * 100) : undefined}
      >
        {fraction != null ? (
          <div className="progress-fill" style={{ width: `${fraction * 100}%` }} />
        ) : (
          <div className="progress-fill progress-indeterminate" />
        )}
      </div>
    </div>
  );
}
