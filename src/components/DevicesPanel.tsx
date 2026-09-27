import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { DeviceItem } from "../types";

export default function DevicesPanel({ signal }: { signal: number }) {
  const [devices, setDevices] = useState<DeviceItem[]>([]);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState(false);

  async function load() {
    try {
      setDevices(await invoke<DeviceItem[]>("get_devices"));
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
        <div><h3>Devices on your network</h3><p className="section-subtitle">{devices.length} {devices.length === 1 ? "device" : "devices"} found on this local network</p></div>
        <button
          className="btn"
          disabled={scanning}
          onClick={async () => {
            setScanning(true);
            setScanError(false);
            try {
              setDevices(await invoke<DeviceItem[]>("scan_lan"));
            } catch {
              setScanError(true);
            } finally {
              setScanning(false);
            }
          }}
        >
          {scanning ? "Scanning…" : "Rescan"}
        </button>
      </div>
      <table className="tbl">
        <thead>
          <tr>
            <th>IP</th>
            <th>MAC</th>
            <th>Vendor</th>
            <th style={{ textAlign: "right" }}>Last seen</th>
          </tr>
        </thead>
        <tbody>
          {devices.map((d) => (
            <tr key={d.mac}>
              <td>{d.ip}</td>
              <td className="mono">{d.mac}</td>
              <td>{d.vendor}</td>
              <td className="num">{new Date(d.last_seen * 1000).toLocaleString()}</td>
            </tr>
          ))}
          {devices.length === 0 && (
            <tr>
              <td colSpan={4} className="empty-cell">
                {scanError ? "The network scan failed. Check your connection and try again." : "No devices found yet. Scan your local network to look for connected devices."}
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <p className="table-note">Scanning reads your system’s local network table and does not require administrator access.</p>
    </div>
  );
}
