import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useMemo, useRef, useState } from "react";
import AlertsFeed from "./components/AlertsFeed";
import AppTable from "./components/AppTable";
import Connections from "./components/Connections";
import DevicesPanel from "./components/DevicesPanel";
import HistoryBar from "./components/HistoryBar";
import Settings from "./components/Settings";
import TrafficGraph from "./components/TrafficGraph";
import UpdateBanner from "./components/UpdateBanner";
import type { OpenTab, TrafficTick, VtResult } from "./types";
import { fmtBytes, fmtRate } from "./types";
import "./App.css";

const MAX_POINTS = 120;

export type Tab = "live" | "usage" | "conns" | "alerts" | "things" | "settings";
export type Theme = "dark" | "light";
export type Accent = "blue" | "violet";

const ACCENTS: Record<Accent, { down: string; up: string }> = {
  blue: { down: "#60a5fa", up: "#38bdf8" },
  violet: { down: "#a78bfa", up: "#f472b6" },
};

const TABS: { id: Tab; label: string; detail: string; icon: React.ReactNode }[] = [
  { id: "live", label: "Overview", detail: "Live traffic & history", icon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M3 17l5-6 4 3 6-8 3 4" /></svg> },
  { id: "usage", label: "Usage", detail: "Apps & hosts", icon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M4 6h16M4 12h16M4 18h10" /></svg> },
  { id: "conns", label: "Connections", detail: "Live flows", icon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="6" cy="6" r="2.5" /><circle cx="18" cy="18" r="2.5" /><path d="M8 8l8 8M18 6l-3 3M6 18l3-3" /></svg> },
  { id: "alerts", label: "Alerts", detail: "Security events", icon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 3l10 17H2z" /><path d="M12 10v4M12 17.5v.5" /></svg> },
  { id: "things", label: "Things", detail: "Devices on LAN", icon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><rect x="3" y="4" width="18" height="12" rx="2" /><path d="M9 20h6" /></svg> },
  { id: "settings", label: "Settings", detail: "Quotas, remote, keys", icon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3" /><path d="M19 12a7 7 0 01-.4 2.3l2 1.6-2 3.4-2.4-1a7 7 0 01-2 1.2L14 21h-4l-.2-2.5a7 7 0 01-2-1.2l-2.4 1-2-3.4 2-1.6A7 7 0 015 12a7 7 0 01.4-2.3l-2-1.6 2-3.4 2.4 1a7 7 0 012-1.2L10 3h4l.2 2.5a7 7 0 012 1.2l2.4-1 2 3.4-2 1.6c.2.7.4 1.5.4 2.3z" /></svg> },
];

const PAGE_META: Record<Tab, { eyebrow: string; title: string; sub: string }> = {
  live: { eyebrow: "NETWORK MONITOR", title: "Overview", sub: "Live download and upload traffic across the last two minutes." },
  usage: { eyebrow: "NETWORK MONITOR", title: "Usage", sub: "Per-application traffic with host drilldown." },
  conns: { eyebrow: "NETWORK MONITOR", title: "Connections", sub: "Every live flow, measured per-connection on Windows." },
  alerts: { eyebrow: "SECURITY", title: "Alerts", sub: "First-seen apps, threats, quotas and device joins." },
  things: { eyebrow: "LOCAL NETWORK", title: "Things", sub: "Devices discovered on your LAN." },
  settings: { eyebrow: "CONFIGURATION", title: "Settings", sub: "Appearance, quotas, VirusTotal, remote access." },
};

export default function App() {
  const [tick, setTick] = useState<TrafficTick | null>(null);
  const [series, setSeries] = useState<{ t: number[]; down: number[]; up: number[] }>({
    t: [],
    down: [],
    up: [],
  });
  const [tab, setTab] = useState<Tab>("live");
  const [selected, setSelected] = useState<string | null>(null);
  const [alertSignal, setAlertSignal] = useState(0);
  const [theme, setTheme] = useState<Theme>(
    () => (localStorage.getItem("nw-theme") as Theme) || "dark"
  );
  const [accent, setAccent] = useState<Accent>(
    () => localStorage.getItem("nw-accent") === "violet" ? "violet" : "blue"
  );
  const [vt, setVt] = useState<VtResult | null>(null);
  const [vtBusy, setVtBusy] = useState(false);
  const [remoteOn, setRemoteOn] = useState(false);
  const [remoteErr, setRemoteErr] = useState("");
  const remoteRef = useRef(false);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.dataset.accent = accent;
    localStorage.setItem("nw-theme", theme);
    localStorage.setItem("nw-accent", accent);
  }, [theme, accent]);

  function pushTick(t: TrafficTick) {
    setTick(t);
    setSeries((s) => {
      const tt = [...s.t, t.ts];
      const down = [...s.down, t.down_rate];
      const up = [...s.up, t.up_rate];
      return { t: tt.slice(-MAX_POINTS), down: down.slice(-MAX_POINTS), up: up.slice(-MAX_POINTS) };
    });
  }

  useEffect(() => {
    invoke<TrafficTick | null>("get_live_snapshot").then((t) => t && pushTick(t)).catch(() => {});
    const offs: Array<() => void> = [];
    let alive = true;
    (async () => {
      offs.push(await listen<TrafficTick>("traffic-tick", (e) => {
        if (!alive || remoteRef.current) return;
        pushTick(e.payload);
      }));
      offs.push(await listen("alert-raised", () => {
        if (!alive || remoteRef.current) return;
        setAlertSignal((n) => n + 1);
      }));
      offs.push(await listen("device-changed", () => {
        if (!alive || remoteRef.current) return;
        setAlertSignal((n) => n + 1);
      }));
      offs.push(await listen<OpenTab>("open-tab", (e) => {
        if (!alive) return;
        const valid: Tab[] = ["live", "usage", "conns", "alerts", "things", "settings"];
        if (valid.includes(e.payload.tab as Tab)) {
          if (remoteRef.current) disconnectRemote();
          setTab(e.payload.tab as Tab);
          if (e.payload.exe) setSelected(e.payload.exe);
        }
      }));
    })();
    return () => {
      alive = false;
      offs.forEach((off) => off());
    };
  }, []);

  function connectRemote(url: string, token: string) {
    disconnectRemote(true);
    setRemoteErr("");
    try {
      const ws = new WebSocket(url);
      wsRef.current = ws;
      ws.onopen = () => ws.send(token);
      ws.onmessage = (ev) => {
        try {
          const data = JSON.parse(String(ev.data));
          if (data.hello) return;
          pushTick(data as TrafficTick);
        } catch {
          /* keep-alive noise */
        }
      };
      ws.onerror = () => {
        if (remoteRef.current) setRemoteErr("Remote connection error — check URL and token.");
      };
      ws.onclose = () => {
        if (remoteRef.current && wsRef.current === ws) {
          setRemoteErr("Remote connection closed.");
          remoteRef.current = false;
          setRemoteOn(false);
        }
      };
      remoteRef.current = true;
      setRemoteOn(true);
      setTab("live");
    } catch (e) {
      setRemoteErr(String(e));
    }
  }

  function disconnectRemote(silent = false) {
    remoteRef.current = false;
    try {
      wsRef.current?.close();
    } catch {
      /* ignore */
    }
    wsRef.current = null;
    setRemoteOn(false);
    if (!silent) setSeries({ t: [], down: [], up: [] });
  }

  const selApp = useMemo(
    () => tick?.apps.find((a) => a.exe === selected) ?? null,
    [tick, selected]
  );

  const colors = useMemo(
    () => theme === "light"
      ? { ...ACCENTS[accent], grid: "rgba(15,23,42,.12)", text: "#64748b" }
      : { ...ACCENTS[accent], grid: "rgba(139,147,167,.15)", text: "#8b93a7" },
    [theme, accent]
  );

  const meta = PAGE_META[tab];
  const localPaused = remoteOn && (tab === "alerts" || tab === "things");

  return (
    <div className={`app-shell ${tab === "live" ? "overview-shell" : ""}`}>
      <div className="titlebar" data-tauri-drag-region>
        <div className="titlebar-brand" data-tauri-drag-region>
          <span className="brand-mark">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4"><path d="M3 17l5-6 4 3 6-8 3 4" /></svg>
          </span>
          NetWire
        </div>
        <div className="titlebar-caption" data-tauri-drag-region>Advanced network monitor</div>
        <div className="window-controls">
          <button
            type="button"
            className="window-control"
            aria-label="Minimize window"
            title="Minimize"
            onClick={() => { void getCurrentWindow().minimize(); }}
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true"><path d="M3.5 8h9" /></svg>
          </button>
          <button
            type="button"
            className="window-control close-control"
            aria-label="Close window"
            title="Close"
            onClick={() => { void getCurrentWindow().close(); }}
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true"><path d="m4 4 8 8M12 4l-8 8" /></svg>
          </button>
        </div>
      </div>

      <nav className="top-nav" aria-label="Main navigation">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`top-nav-button ${tab === t.id ? "selected" : ""}`}
            aria-label={t.label}
            aria-current={tab === t.id ? "page" : undefined}
            title={t.label}
            data-tooltip={t.label}
            onClick={() => setTab(t.id)}
          >
            <span aria-hidden="true">{t.icon}</span>
          </button>
        ))}
      </nav>

      <div className="app-frame">
        <div className="main-content">
          {tab !== "live" && (
            <div className="page-header">
              <div>
                <div className="eyebrow">{meta.eyebrow}</div>
                <h1>{meta.title}</h1>
                <p>{meta.sub}</p>
              </div>
            </div>
          )}

          {remoteOn && (
            <div className="notice remote">
              <span className="notice-icon">R</span>
              <span>
                Viewing a remote host live — local history, alerts and LAN data are paused.
                <button className="btn" style={{ marginLeft: 10 }} onClick={() => disconnectRemote()}>
                  Disconnect
                </button>
              </span>
            </div>
          )}
          <UpdateBanner />
          {remoteErr && (
            <div className="notice warn"><span className="notice-icon">!</span><span>{remoteErr}</span></div>
          )}
          {tab !== "live" && tick?.per_app_estimated && tick.needs_admin && !remoteOn && (
            <div className="notice warn">
              <span className="notice-icon">!</span>
              <span>
                Per-app bytes are estimates — Windows refused per-flow counters (access denied).
                Restart NetWire as administrator for measured per-connection bytes.
              </span>
            </div>
          )}
          {tab === "live" && (
            <section className="panel traffic-panel overview-traffic">
              <div className="panel-heading">
                <div><div className="eyebrow graph-eyebrow">LIVE TRAFFIC</div><h2>Network throughput</h2></div>
                <div className="row-gap">
                  <span className="speed-read" title="Current download / upload speed">
                    <span className="down">↓ {tick ? fmtRate(tick.down_rate) : "—"}</span>
                    <span className="up">↑ {tick ? fmtRate(tick.up_rate) : "—"}</span>
                  </span>
                </div>
              </div>
              <div className="overview-chart">
                <TrafficGraph down={series.down} up={series.up} labels={series.t} colors={colors} fill />
                {series.t.length === 0 && <div className="graph-empty">Waiting for live network data</div>}
              </div>
            </section>
          )}

          {tab === "usage" && (
            <>
              {!remoteOn && <HistoryBar />}
              <AppTable apps={tick?.apps ?? []} selected={selected} onSelect={setSelected} />
              <div className="panel">
                <div className="panel-head">
                  <div><h3>{selApp ? `Hosts — ${selApp.name}` : "Hosts"}</h3><p className="section-subtitle">{selApp ? `${selApp.hosts.length} remote endpoints` : "Select an app to drill in"}</p></div>
                  {selApp && !remoteOn && (
                    <button
                      className="btn"
                      disabled={vtBusy}
                      onClick={async () => {
                        setVtBusy(true);
                        setVt(null);
                        try {
                          setVt(await invoke<VtResult>("vt_lookup", { exe: selApp.exe }));
                        } catch (e) {
                          setVt({
                            exe: selApp.exe, sha256: "", malicious: 0, suspicious: 0,
                            harmless: 0, undetected: 0, permalink: "", error: String(e),
                          });
                        } finally {
                          setVtBusy(false);
                        }
                      }}
                    >
                      {vtBusy ? "Checking…" : "Check VirusTotal"}
                    </button>
                  )}
                </div>
                {vt && selApp && (
                  <div className={`vt ${vt.malicious > 0 ? "bad" : "ok"}`}>
                    {vt.error ? (
                      <span>{vt.error}</span>
                    ) : (
                      <span>
                        <b className={vt.malicious > 0 ? "bad-t" : "ok-t"}>{vt.malicious} malicious</b>
                        {" · "}{vt.suspicious} suspicious · {vt.harmless} harmless · {vt.undetected} undetected
                      </span>
                    )}
                    {vt.permalink && <div className="muted small mono">{vt.permalink}</div>}
                  </div>
                )}
                {selApp ? (
                  <div className="table-scroll"><table className="tbl">
                    <thead>
                      <tr>
                        <th>Remote</th>
                        <th>Hostname</th>
                        <th>CC</th>
                        <th className="num">Conns</th>
                        <th className="num">Bytes</th>
                      </tr>
                    </thead>
                    <tbody>
                      {selApp.hosts.map((h) => (
                        <tr key={h.ip}>
                          <td className="mono">{h.ip}</td>
                          <td>{h.hostname || <span className="muted">—</span>}</td>
                          <td>{h.country || "—"}</td>
                          <td className="num">{h.connections}</td>
                          <td className="num">{fmtBytes(h.up_bytes + h.down_bytes)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table></div>
                ) : (
                  <div className="empty-state compact"><span className="empty-icon">↗</span><div><b>No app selected</b><p>Click an application above to inspect its remote endpoints.</p></div></div>
                )}
              </div>
            </>
          )}

          {tab === "conns" && <Connections signal={alertSignal} />}

          {tab === "alerts" &&
            (localPaused
              ? <div className="panel"><div className="empty-state compact"><span className="empty-icon">!</span><div><b>Paused in remote view</b><p>Disconnect to see local alerts.</p></div></div></div>
              : <AlertsFeed signal={alertSignal} />)}

          {tab === "things" &&
            (localPaused
              ? <div className="panel"><div className="empty-state compact"><span className="empty-icon">!</span><div><b>Paused in remote view</b><p>Disconnect to see local devices.</p></div></div></div>
              : <DevicesPanel signal={alertSignal} />)}

          {tab === "settings" && (
            <Settings
              theme={theme}
              setTheme={setTheme}
              accent={accent}
              setAccent={setAccent}
              liveApps={(tick?.apps ?? []).map((a) => ({ exe: a.exe, name: a.name }))}
              onRemoteConnect={connectRemote}
              onRemoteDisconnect={() => disconnectRemote()}
              remoteOn={remoteOn}
            />
          )}

        </div>
      </div>
    </div>
  );
}
