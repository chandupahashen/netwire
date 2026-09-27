import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { HistoryPoint } from "../types";
import { fmtBytes } from "../types";

const RANGES = [
  { label: "5m", secs: 300, bucket: 5 },
  { label: "1h", secs: 3600, bucket: 30 },
  { label: "24h", secs: 86400, bucket: 300 },
  { label: "7d", secs: 604800, bucket: 3600 },
  { label: "30d", secs: 2592000, bucket: 10800 },
  { label: "60d", secs: 5184000, bucket: 21600 },
  { label: "90d", secs: 7776000, bucket: 21600 },
];

const KEEP_DAYS = 90;
type CustomRange = { fromTs: number; toTs: number; label: string };

function dateInputValue(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function dateStartTimestamp(value: string) {
  const [year, month, day] = value.split("-").map(Number);
  return Math.floor(new Date(year, month - 1, day).getTime() / 1000);
}

function dateEndTimestamp(value: string) {
  const [year, month, day] = value.split("-").map(Number);
  return Math.floor(new Date(year, month - 1, day + 1).getTime() / 1000) - 1;
}

function formatDate(timestamp: number) {
  return new Date(timestamp * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

export default function HistoryBar() {
  const [range, setRange] = useState(RANGES[1]);
  const [rows, setRows] = useState<HistoryPoint[]>([]);
  const [loading, setLoading] = useState(false);
  const [saved, setSaved] = useState("");
  const [loadError, setLoadError] = useState(false);
  const [customOpen, setCustomOpen] = useState(false);
  const [customRange, setCustomRange] = useState<CustomRange | null>(null);
  const [customFrom, setCustomFrom] = useState(() => {
    const date = new Date();
    date.setDate(date.getDate() - 6);
    return dateInputValue(date);
  });
  const [customTo, setCustomTo] = useState(() => dateInputValue(new Date()));
  const [rangeError, setRangeError] = useState("");

  const today = dateInputValue(new Date());
  const earliest = new Date();
  earliest.setHours(0, 0, 0, 0);
  earliest.setDate(earliest.getDate() - (KEEP_DAYS - 1));
  const earliestAllowed = dateInputValue(earliest);

  useEffect(() => { void load(RANGES[1]); }, []);

  async function load(r = range, custom: CustomRange | null = customRange) {
    setLoading(true);
    setLoadError(false);
    try {
      const now = Math.floor(Date.now() / 1000);
      const fromTs = custom?.fromTs ?? now - r.secs;
      const toTs = custom?.toTs ?? now;
      const granularityS = custom
        ? Math.max(5, Math.ceil((toTs - fromTs + 1) / 300 / 5) * 5)
        : r.bucket;
      const data = await invoke<HistoryPoint[]>("get_history", {
        fromTs,
        toTs,
        granularityS,
      });
      setRows(data);
    } catch {
      setRows([]);
      setLoadError(true);
    } finally {
      setLoading(false);
    }
  }

  function applyCustomRange() {
    if (!customFrom || !customTo) {
      setRangeError("Choose both a start date and an end date.");
      return;
    }
    const fromTs = dateStartTimestamp(customFrom);
    const toTs = dateEndTimestamp(customTo);
    if (fromTs > toTs) {
      setRangeError("The start date must be before the end date.");
      return;
    }
    const period = { fromTs, toTs, label: `${formatDate(fromTs)} – ${formatDate(toTs)}` };
    const customSelection = { label: "Custom", secs: toTs - fromTs + 1, bucket: 0 };
    setRange(customSelection);
    setCustomRange(period);
    setRangeError("");
    void load(customSelection, period);
  }

  const totals = new Map<string, number>();
  for (const p of rows) totals.set(p.app, (totals.get(p.app) ?? 0) + p.up_bytes + p.down_bytes);
  const top = [...totals.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8);
  const max = top[0]?.[1] ?? 1;

  return (
    <div className="panel">
      <div className="panel-head">
        <div><h3>Traffic history</h3><p className="section-subtitle">Compare which apps used the most data</p></div>
        <div className="history-controls">
          <div className="row-gap history-presets">
            {RANGES.map((r) => (
              <button
                key={r.label}
                className={r.label === range.label ? "btn active" : "btn"}
                aria-pressed={r.label === range.label}
                disabled={loading}
                onClick={() => {
                  setRange(r);
                  setCustomRange(null);
                  setCustomOpen(false);
                  setRangeError("");
                  void load(r, null);
                }}
              >
                {r.label}
              </button>
            ))}
            <button className={customRange ? "btn active" : "btn"} aria-pressed={Boolean(customRange)} onClick={() => setCustomOpen((open) => !open)}>
              Custom
            </button>
            <button className="btn" onClick={() => void load()} disabled={loading}>
              {loading ? "Loading…" : "Refresh"}
            </button>
            <button
              className="btn"
              disabled={loading}
              onClick={async () => {
                setSaved("");
                try {
                  const now = Math.floor(Date.now() / 1000);
                  const p = await invoke<string>("export_history_csv", {
                    fromTs: customRange?.fromTs ?? now - range.secs,
                    toTs: customRange?.toTs ?? now,
                  });
                  setSaved(`Saved ${p}`);
                } catch (e) {
                  if (String(e) !== "cancelled") setSaved(String(e));
                }
              }}
            >
              Export CSV
            </button>
          </div>
          {customOpen && (
            <div className="date-range-controls">
              <label>From<input className="search" type="date" aria-label="Start date" min={earliestAllowed} max={today} value={customFrom} onChange={(e) => setCustomFrom(e.target.value)} /></label>
              <label>To<input className="search" type="date" aria-label="End date" min={earliestAllowed} max={today} value={customTo} onChange={(e) => setCustomTo(e.target.value)} /></label>
              <button className="btn active" onClick={applyCustomRange} disabled={loading}>Apply</button>
            </div>
          )}
          {rangeError && <div className="range-error" role="alert">{rangeError}</div>}
        </div>
      </div>
      {saved && <div className="muted small">{saved}</div>}
      {top.length === 0 ? (
        <div className="empty-state compact"><span className="empty-icon">↗</span><div><b>{loading ? "Loading traffic history" : loadError ? "Couldn’t load traffic history" : "No history in this time range"}</b><p>{loading ? "Your recent network activity will appear here." : loadError ? "Try refreshing in a moment." : "Leave NetWire running to build a local record of app traffic."}</p></div></div>
      ) : (
        <div className="bars">
          {top.map(([app, bytes]) => (
            <div key={app} className="bar-row">
              <span className="bar-label">{app}</span>
              <div className="bar-track">
                <div className="bar-fill" style={{ width: `${(bytes / max) * 100}%` }} />
              </div>
              <span className="num">{fmtBytes(bytes)}</span>
            </div>
          ))}
        </div>
      )}
      <div className="table-note">{rows.length} time intervals · {customRange ? customRange.label : `last ${range.label}`}</div>
    </div>
  );
}
