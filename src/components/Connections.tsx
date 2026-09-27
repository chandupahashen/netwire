import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { ConnectionStat } from "../types";
import { fmtRate } from "../types";

export default function Connections({ signal }: { signal: number }) {
  const [conns, setConns] = useState<ConnectionStat[]>([]);
  const [q, setQ] = useState("");
  const [saved, setSaved] = useState("");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(25);

  async function load() {
    try {
      setConns(await invoke<ConnectionStat[]>("get_connections"));
    } catch {
      /* backend not ready */
    }
  }

  useEffect(() => {
    load();
    const t = setInterval(load, 2000);
    return () => clearInterval(t);
  }, [signal]);

  const rows = conns.filter(
    (c) =>
      q === "" ||
      c.app.toLowerCase().includes(q.toLowerCase()) ||
      c.remote.toLowerCase().includes(q.toLowerCase()) ||
      c.exe.toLowerCase().includes(q.toLowerCase())
  );
  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize));
  const currentPage = Math.min(page, pageCount);
  const pageRows = rows.slice((currentPage - 1) * pageSize, currentPage * pageSize);
  const firstRow = rows.length === 0 ? 0 : (currentPage - 1) * pageSize + 1;
  const lastRow = Math.min(currentPage * pageSize, rows.length);

  useEffect(() => {
    if (page > pageCount) setPage(pageCount);
  }, [page, pageCount]);

  return (
    <div className="panel">
      <div className="panel-head">
        <div>
          <h3>Live connections</h3>
          <p className="section-subtitle">{rows.length} flows · ● measured by OS counters, ~ estimated</p>
        </div>
        <div className="row-gap">
          <input
            className="search"
            placeholder="Filter app / remote…"
            aria-label="Filter connections"
            value={q}
            onChange={(e) => { setQ(e.target.value); setPage(1); }}
          />
          <button
            className="btn"
            onClick={async () => {
              setSaved("");
              try {
                const p = await invoke<string>("export_connections_csv");
                setSaved(`Saved ${p}`);
              } catch (e) {
                if (!String(e).includes("cancelled")) setSaved(String(e));
              }
            }}
          >
            Export CSV
          </button>
        </div>
      </div>
      {saved && <p className="section-subtitle">{saved}</p>}
      <div className="table-scroll"><table className="tbl">
        <thead>
          <tr>
            <th>Application</th>
            <th>Proto</th>
            <th>Remote</th>
            <th className="num">Download</th>
            <th className="num">Upload</th>
            <th>Src</th>
          </tr>
        </thead>
        <tbody>
          {pageRows.map((c, i) => (
            <tr key={`${c.proto}-${c.local}-${c.remote}-${i}`}>
              <td title={c.exe}>
                <b>{c.app}</b>
                <div className="muted small">pid {c.pid}</div>
              </td>
              <td>{c.proto}</td>
              <td className="mono">{c.remote}</td>
              <td className="num down">{fmtRate(c.down_rate)}</td>
              <td className="num up">{fmtRate(c.up_rate)}</td>
              <td title={c.measured ? "Measured by OS per-flow counters" : "Proportional estimate"}>
                {c.measured ? "●" : <span className="muted">~</span>}
              </td>
            </tr>
          ))}
          {rows.length === 0 && (
            <tr>
              <td colSpan={6} className="empty-cell">
                {q ? "No connections match your filter." : "No connections right now. Run as admin for full socket visibility."}
              </td>
            </tr>
          )}
        </tbody>
      </table></div>
      {rows.length > 0 && (
        <div className="pagination">
          <span>Showing {firstRow}–{lastRow} of {rows.length}</span>
          <div className="pagination-controls">
            <label className="pagination-size">
              Rows
              <select aria-label="Rows per page" value={pageSize} onChange={(e) => { setPageSize(Number(e.target.value)); setPage(1); }}>
                <option value={25}>25</option>
                <option value={50}>50</option>
                <option value={100}>100</option>
              </select>
            </label>
            <button className="btn" disabled={currentPage <= 1} onClick={() => setPage(currentPage - 1)} aria-label="Previous page">Previous</button>
            <span className="pagination-page">Page {currentPage} of {pageCount}</span>
            <button className="btn" disabled={currentPage >= pageCount} onClick={() => setPage(currentPage + 1)} aria-label="Next page">Next</button>
          </div>
        </div>
      )}
    </div>
  );
}
