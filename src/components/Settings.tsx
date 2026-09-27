import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { Accent, Theme } from "../App";
import type { AppQuota, RemoteConfig, RemoteStatus } from "../types";
import { fmtBytes } from "../types";

function Section({ title, sub, children }: { title: string; sub?: string; children: React.ReactNode }) {
  return (
    <div className="panel">
      <div className="panel-heading">
        <div><h2>{title}</h2>{sub && <p>{sub}</p>}</div>
      </div>
      <div className="settings-grid">{children}</div>
    </div>
  );
}

export default function Settings({
  theme,
  setTheme,
  accent,
  setAccent,
  liveApps,
  onRemoteConnect,
  onRemoteDisconnect,
  remoteOn,
}: {
  theme: Theme;
  setTheme: (t: Theme) => void;
  accent: Accent;
  setAccent: (a: Accent) => void;
  liveApps: { exe: string; name: string }[];
  onRemoteConnect: (url: string, token: string) => void;
  onRemoteDisconnect: () => void;
  remoteOn: boolean;
}) {
  const [autostart, setAutostartState] = useState(false);
  const [quotas, setQuotas] = useState<AppQuota[]>([]);
  const [newExe, setNewExe] = useState("");
  const [newMb, setNewMb] = useState("500");
  const [vtKey, setVtKey] = useState("");
  const [vtSaved, setVtSaved] = useState(false);
  const [rcfg, setRcfg] = useState<RemoteConfig>({ enabled: false, port: 17813, token: "" });
  const [rbind, setRbind] = useState("127.0.0.1");
  const [rstatus, setRstatus] = useState<RemoteStatus | null>(null);
  const [rurl, setRurl] = useState("ws://127.0.0.1:17813");
  const [rtoken, setRtoken] = useState("");
  const [msg, setMsg] = useState("");

  async function load() {
    try {
      setAutostartState(await invoke<boolean>("is_autostart"));
      setQuotas(await invoke<AppQuota[]>("get_app_quotas"));
      const k = await invoke<string>("get_setting", { key: "vt_api_key" });
      setVtSaved(k.trim().length > 0);
      const cfg = await invoke<RemoteConfig>("get_remote_config");
      setRcfg(cfg);
      setRstatus(await invoke<RemoteStatus>("remote_status"));
    } catch (e) {
      setMsg(String(e));
    }
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <>
      <Section title="Appearance" sub="Theme and accent used across the app and graphs">
        <label>
          Theme
          <select value={theme} onChange={(e) => setTheme(e.target.value as Theme)}>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </label>
        <label>
          Accent
          <select value={accent} onChange={(e) => setAccent(e.target.value as Accent)}>
            <option value="blue">Blue</option>
            <option value="violet">Violet</option>
          </select>
        </label>
      </Section>

      <Section title="Startup" sub="Launch behaviour">
        <label className="check">
          <input
            type="checkbox"
            checked={autostart}
            onChange={async (e) => {
              setAutostartState(await invoke<boolean>("set_autostart", { enabled: e.target.checked }));
            }}
          />
          Start with system
        </label>
      </Section>

      <Section title="Per-app quotas" sub="Get warned when an app blows its daily budget">
        <div className="full">
          {quotas.map((q) => (
            <div key={q.exe} className="quota-row">
              <span title={q.exe}><b>{q.name || q.exe}</b></span>
              <span className="num">{fmtBytes(q.bytes_today)} / {fmtBytes(q.bytes_per_day)}</span>
              <button
                className="btn"
                onClick={async () => {
                  setQuotas(await invoke<AppQuota[]>("delete_app_quota", { exe: q.exe }));
                }}
              >
                Remove
              </button>
            </div>
          ))}
          {quotas.length === 0 && <p className="section-subtitle">No per-app quotas yet.</p>}
          <div className="row-gap" style={{ marginTop: 10, flexWrap: "wrap" }}>
            <input
              className="search"
              style={{ width: 300 }}
              placeholder="C:\path\to\app.exe"
              aria-label="Application path"
              value={newExe}
              onChange={(e) => setNewExe(e.target.value)}
              list="nw-exes"
            />
            <datalist id="nw-exes">
              {liveApps.filter((a) => a.exe).map((a) => (
                <option key={a.exe} value={a.exe}>{a.name}</option>
              ))}
            </datalist>
            <input
              className="num-in"
              placeholder="MiB"
              aria-label="Quota in MiB"
              value={newMb}
              onChange={(e) => setNewMb(e.target.value)}
            />
            <button
              className="btn"
              onClick={async () => {
                if (!newExe) return;
                const name = liveApps.find((a) => a.exe === newExe)?.name ?? newExe;
                setQuotas(await invoke<AppQuota[]>("set_app_quota", {
                  exe: newExe, name, bytesPerDay: Math.floor(Number(newMb) * 1048576),
                }));
                setNewExe("");
              }}
            >
              Add
            </button>
          </div>
        </div>
      </Section>

      <Section title="VirusTotal" sub="Free key at virustotal.com — powers the Usage tab check">
        <label className="full">
          API key
          <input
            className="search"
            style={{ width: 340 }}
            type="password"
            placeholder={vtSaved ? "Saved — enter a new key to replace" : "Paste VT API key"}
            value={vtKey}
            onChange={(e) => setVtKey(e.target.value)}
          />
          <button
            className="btn"
            onClick={async () => {
              await invoke("set_setting", { key: "vt_api_key", value: vtKey });
              setVtKey("");
              setVtSaved(true);
            }}
          >
            Save
          </button>
        </label>
      </Section>

      <Section title="Remote monitoring — serve" sub="Expose this PC's live traffic to another NetWire">
        <label className="check">
          <input
            type="checkbox"
            checked={rcfg.enabled}
            onChange={(e) => setRcfg({ ...rcfg, enabled: e.target.checked })}
          />
          Enable server {rstatus?.running
            ? <b className="ok">● running :{rstatus.port} ({rstatus.peers} peers)</b>
            : <span className="muted">○ stopped</span>}
        </label>
        <label>
          Bind
          <input className="num-in" style={{ width: 120 }} value={rbind} onChange={(e) => setRbind(e.target.value)} />
        </label>
        <label>
          Port
          <input
            className="num-in"
            value={rcfg.port}
            onChange={(e) => setRcfg({ ...rcfg, port: Number(e.target.value) || 17813 })}
          />
        </label>
        <label>
          Token
          <input
            className="search"
            style={{ width: 200 }}
            value={rcfg.token}
            onChange={(e) => setRcfg({ ...rcfg, token: e.target.value })}
          />
          <button
            className="btn"
            onClick={async () => {
              try {
                const st = await invoke<RemoteStatus>("set_remote_config", {
                  enabled: rcfg.enabled, port: rcfg.port, token: rcfg.token, bind: rbind,
                });
                setRstatus(st);
                setRcfg(await invoke<RemoteConfig>("get_remote_config"));
                setMsg("");
              } catch (e) {
                setMsg(String(e));
              }
            }}
          >
            Apply
          </button>
        </label>
        <p className="section-subtitle full">
          Bind 127.0.0.1 for local-only, 0.0.0.0 for LAN. Clients send the token first.
          A token auto-generates if left blank.
        </p>
      </Section>

      <Section title="Remote monitoring — view" sub="Read-only live view of another NetWire instance">
        {!remoteOn ? (
          <div className="row-gap full" style={{ flexWrap: "wrap" }}>
            <input className="search" style={{ width: 220 }} value={rurl} onChange={(e) => setRurl(e.target.value)} aria-label="Remote URL" />
            <input className="search" style={{ width: 180 }} placeholder="token" value={rtoken} onChange={(e) => setRtoken(e.target.value)} aria-label="Remote token" />
            <button className="btn" onClick={() => onRemoteConnect(rurl, rtoken)}>Connect</button>
          </div>
        ) : (
          <div className="row-gap full">
            <b className="ok">Connected (remote view)</b>
            <button className="btn" onClick={onRemoteDisconnect}>Disconnect</button>
          </div>
        )}
        <p className="section-subtitle full">History, alerts and LAN tabs pause while connected.</p>
      </Section>

      {msg && <div className="notice warn"><span className="notice-icon">!</span><span>{msg}</span></div>}
    </>
  );
}
