import { useState } from "react";
import type { AppStat } from "../types";
import { fmtBytes, fmtRate } from "../types";

export default function AppTable({
  apps,
  selected,
  onSelect,
}: {
  apps: AppStat[];
  selected: string | null;
  onSelect: (exe: string | null) => void;
}) {
  const [q, setQ] = useState("");
  const rows = apps.filter(
    (a) =>
      q === "" ||
      a.name.toLowerCase().includes(q.toLowerCase()) ||
      a.exe.toLowerCase().includes(q.toLowerCase())
  );
  return (
    <div className="panel">
      <div className="panel-head">
        <h3>Applications ({rows.length})</h3>
        <input
          className="search"
          placeholder="Search applications"
          aria-label="Search applications"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </div>
      <div className="table-scroll"><table className="tbl">
        <thead>
          <tr>
            <th>Application</th>
            <th className="num">Download</th>
            <th className="num">Upload</th>
            <th className="num">Connections</th>
            <th className="num">Data this update</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((a) => (
            <tr
              key={a.exe || a.name}
              className={`app-row ${selected === a.exe ? "sel" : ""}`}
              onClick={() => onSelect(selected === a.exe ? null : a.exe)}
            >
              <td title={a.exe}>
                <b>{a.name}</b>
                <div className="muted small">{a.exe || `pid ${a.pid}`}</div>
              </td>
              <td className="num down">{fmtRate(a.down_rate)}</td>
              <td className="num up">{fmtRate(a.up_rate)}</td>
              <td className="num">{a.connections}</td>
              <td className="num">{fmtBytes(a.down_bytes + a.up_bytes)}</td>
            </tr>
          ))}
          {rows.length === 0 && (
            <tr>
              <td colSpan={5} className="muted">
                {q ? "No applications match your search." : "No app activity yet. Network activity will appear here as applications connect."}
              </td>
            </tr>
          )}
        </tbody>
      </table></div>
    </div>
  );
}
