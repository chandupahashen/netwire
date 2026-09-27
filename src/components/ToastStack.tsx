import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import type { ToastPayload } from "../types";

const KIND_LABEL: Record<string, string> = {
  "new-app": "New app",
  "device-join": "New device",
  "threat-ip": "Threat",
  quota: "Quota",
  "app-quota": "App quota",
};

/** Rendered inside the borderless `toast` window (?toast=1). Dumb stack:
 *  Rust owns queue/coalesce/policy; we render, auto-dismiss, and report height. */
export default function ToastStack() {
  const [toasts, setToasts] = useState<ToastPayload[]>([]);
  const hostRef = useRef<HTMLDivElement>(null);
  const timers = useRef(new Map<number, ReturnType<typeof setTimeout>>());

  useEffect(() => {
    document.documentElement.dataset.toast = "1";
    document.documentElement.dataset.theme = localStorage.getItem("nw-theme") || "dark";
    document.documentElement.dataset.accent = localStorage.getItem("nw-accent") || "teal";
    invoke<ToastPayload[]>("toast_list").then(setToasts).catch(() => {});
    let off = () => {};
    (async () => {
      off = await listen<ToastPayload[]>("notify-toast-list", (e) => setToasts(e.payload));
    })();
    return () => {
      off();
      timers.current.forEach(clearTimeout);
    };
  }, []);

  // Arm auto-dismiss per toast.
  useEffect(() => {
    const seen = new Set(toasts.map((t) => t.id));
    timers.current.forEach((_, id) => {
      if (!seen.has(id)) {
        clearTimeout(timers.current.get(id));
        timers.current.delete(id);
      }
    });
    toasts.forEach((t) => {
      if (!timers.current.has(t.id)) {
        timers.current.set(
          t.id,
          setTimeout(() => invoke("toast_dismiss", { id: t.id }).catch(() => {}), t.duration_ms)
        );
      }
    });
  }, [toasts]);

  // Report content height so Rust sizes + anchors the window.
  useEffect(() => {
    const h = hostRef.current?.scrollHeight ?? 120;
    invoke("toast_resize", { height: Math.min(520, Math.max(80, h)) }).catch(() => {});
  }, [toasts]);

  function act(id: number, action: string) {
    const timer = timers.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timers.current.delete(id);
    }
    invoke("toast_action", { id, action }).catch(() => {
      if (action !== "view") setToasts((ts) => ts.filter((t) => t.id !== id));
    });
  }

  return (
    <div ref={hostRef} className="toast-stack">
      {toasts.map((t) => (
        <div key={t.id} className={`toast sev-${t.severity}`} role="alert">
          <span className="toast-stripe" />
          <div className="toast-body">
            <div className="toast-head">
              <span className="toast-kind">{KIND_LABEL[t.kind] ?? t.kind}</span>
              {t.app && <span className="toast-app">{t.app}</span>}
            </div>
            <div className="toast-msg">{t.message}</div>
            <div className="toast-actions">
              <button className="toast-btn" onClick={() => act(t.id, "view")}>
                View
              </button>
              <button className="toast-btn ghost" onClick={() => act(t.id, "close")}>
                Dismiss
              </button>
            </div>
          </div>
          <button className="toast-x" aria-label="Dismiss" onClick={() => act(t.id, "close")}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
