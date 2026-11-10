import type { ScanResponse } from "@/lib/api/types";
import { formatDateTimeIST } from "@/lib/format";

interface ScanStatusLineProps {
  data: ScanResponse;
}

/** One line on where the background scan is, e.g. "Scanned 500 stocks · finished 10 Nov, 16:20 IST". */
export function ScanStatusLine({ data }: ScanStatusLineProps) {
  const s = data.status;
  const failed = s.failed.length > 0 ? ` · ${s.failed.length} failed` : "";
  if (s.state === "running") {
    const pct = s.universe_size > 0 ? Math.round((s.scanned / s.universe_size) * 100) : 0;
    return (
      <span className="flex items-center gap-2">
        <span
          className="relative h-1.5 w-24 overflow-hidden rounded-full bg-surface-3"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={s.universe_size}
          aria-valuenow={s.scanned}
          aria-label="Scan progress"
        >
          <span className="absolute inset-y-0 left-0 bg-accent" style={{ width: `${pct}%` }} />
        </span>
        Scanning {s.scanned} of {s.universe_size}
        {failed}
        {s.finished_at ? " · showing the previous scan" : ""}
      </span>
    );
  }
  if (s.state === "pending") return <span>Waiting for the first scan to start…</span>;
  return (
    <span suppressHydrationWarning>
      Scanned {s.universe_size} stocks{failed} · finished {formatDateTimeIST(s.finished_at)}
      {s.next_run_at ? ` · next ${formatDateTimeIST(s.next_run_at)}` : ""}
    </span>
  );
}
