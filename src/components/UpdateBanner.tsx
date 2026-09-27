import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export default function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    const timer = window.setTimeout(async () => {
      try {
        const available = await check();
        if (active && available) setUpdate(available);
      } catch {
        // Local development builds have no release endpoint; keep startup quiet.
      }
    }, 1800);

    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, []);

  if (!update) return null;

  return (
    <div className="notice update-notice" role="status">
      <span className="notice-icon">↑</span>
      <span className="update-copy">
        <b>NetWire {update.version} is ready</b>
        <span className="muted">Install the update and restart the app.</span>
        {error && <span className="update-error">{error}</span>}
      </span>
      <button
        className="btn primary"
        disabled={installing}
        onClick={async () => {
          setInstalling(true);
          setError("");
          try {
            await update.downloadAndInstall();
            await relaunch();
          } catch (e) {
            setError(`Update failed: ${String(e)}`);
            setInstalling(false);
          }
        }}
      >
        {installing ? "Installing…" : "Install and restart"}
      </button>
    </div>
  );
}
