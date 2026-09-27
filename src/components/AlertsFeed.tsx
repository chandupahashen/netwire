import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { AlertItem } from "../types";

export default function AlertsFeed({ signal }: { signal: number }) {
  const [alerts, setAlerts] = useState<AlertItem[]>([]);

  async function load() {
    try {
      setAlerts(await invoke<AlertItem[]>("get_alerts", { limit: 100 }));
    } catch {
      /* backend not ready */
    }
  }

  useEffect(() => {
    load();
  }, [signal]);

  return (
    <div className="panel">
      <div className="panel-head">
        <div><h3>Recent alerts</h3><p className="section-subtitle">{alerts.length} {alerts.length === 1 ? "event" : "events"} recorded</p></div>
        <button
          className="btn"
          onClick={async () => {
            try {
              await invoke("mark_alerts_read");
              load();
            } catch {
              /* Keep the current feed visible if the update fails. */
            }
          }}
        >
          Mark read
        </button>
      </div>
      <div className="feed">
        {alerts.map((a) => (
          <div key={a.id} className={`alert sev-${a.severity} ${a.read ? "read" : ""}`}>
            <span className="kind">{a.kind}</span>
            <span>{a.message}</span>
            <span className="muted small">{new Date(a.ts * 1000).toLocaleString()}</span>
          </div>
        ))}
        {alerts.length === 0 && <div className="empty-state"><span className="empty-icon">✓</span><div><b>You’re all caught up</b><p>Network changes and activity alerts will appear here.</p></div></div>}
      </div>
    </div>
  );
}
