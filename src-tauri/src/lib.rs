mod alerts;
mod enrich;
mod estats;
mod models;
mod monitor;
mod remote;
mod store;
mod vt;

use models::{
    AlertItem, AppQuota, ConnectionStat, DeviceItem, HistoryPoint, QuotaStatus, RemoteConfig,
    RemoteStatus, TrafficTick, VtResult,
};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const TICK_SECS: u64 = 1;
const DB_WRITE_EVERY_TICKS: u64 = 2;
const KEEP_DAYS: i64 = 90;
const DEFAULT_QUOTA: u64 = 5 * 1024 * 1024 * 1024; // 5 GiB/day
const DEFAULT_REMOTE_PORT: u16 = 17813;

struct AppState {
    store: Arc<Mutex<store::Store>>,
    last_tick: parking_lot::RwLock<Option<TrafficTick>>,
    last_conns: parking_lot::RwLock<Vec<ConnectionStat>>,
    quota_bytes_per_day: AtomicU64,
    known_threat_hits: parking_lot::RwLock<HashSet<String>>,
    quota_hits: parking_lot::RwLock<HashSet<(String, i64)>>,
    tray: Mutex<Option<tauri::tray::TrayIcon>>,
    remote_tx: tokio::sync::broadcast::Sender<String>,
    remote_handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    remote_peers: Arc<AtomicUsize>,
    running_port: Mutex<Option<u16>>,
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn day_start_ts() -> i64 {
    let now = chrono::Utc::now();
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|d| d.and_utc().timestamp())
        .unwrap_or(now.timestamp() - 86400)
}

fn human_rate(bps: f64) -> String {
    if bps >= 1e9 {
        format!("{:.2} GB/s", bps / 1e9)
    } else if bps >= 1e6 {
        format!("{:.2} MB/s", bps / 1e6)
    } else if bps >= 1e3 {
        format!("{:.1} KB/s", bps / 1e3)
    } else {
        format!("{:.0} B/s", bps)
    }
}

fn default_token() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    (
        std::process::id(),
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default(),
    )
        .hash(&mut h);
    format!("{:016x}", h.finish())
}

// ---------- Commands ----------

#[tauri::command]
fn get_live_snapshot(state: tauri::State<'_, AppState>) -> Option<TrafficTick> {
    state.last_tick.read().clone()
}

#[tauri::command]
fn get_connections(state: tauri::State<'_, AppState>) -> Vec<ConnectionStat> {
    state.last_conns.read().clone()
}

#[tauri::command]
fn get_history(
    state: tauri::State<'_, AppState>,
    from_ts: i64,
    to_ts: i64,
    granularity_s: i64,
) -> Result<Vec<HistoryPoint>, String> {
    let store = state.store.lock();
    store
        .history(from_ts, to_ts, granularity_s.max(1))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_alerts(
    state: tauri::State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<AlertItem>, String> {
    let store = state.store.lock();
    store
        .recent_alerts(limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn mark_alerts_read(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let store = state.store.lock();
    store.mark_alerts_read().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_devices(state: tauri::State<'_, AppState>) -> Result<Vec<DeviceItem>, String> {
    let store = state.store.lock();
    store.devices(200).map_err(|e| e.to_string())
}

#[tauri::command]
fn scan_lan(app: AppHandle) -> Result<Vec<DeviceItem>, String> {
    let rows = enrich::parse_arp_table();
    let now = now_ts();
    let state: tauri::State<'_, AppState> = app.state();
    let mut fresh = 0;
    {
        let store = state.store.lock();
        for (ip, mac) in &rows {
            let vendor = enrich::vendor_guess(mac);
            if let Ok(is_new) = store.upsert_device(mac, ip, &vendor, now) {
                if is_new {
                    fresh += 1;
                    let _ = store.push_alert(
                        now,
                        "device-join",
                        "",
                        &format!("New device on LAN: {} ({})", ip, mac),
                        "info",
                    );
                }
            }
        }
        let devices = store.devices(200).map_err(|e| e.to_string())?;
        let _ = app.emit("device-changed", &devices);
        if fresh > 0 {
            let _ = app.emit(
                "alert-raised",
                serde_json::json!({"kind":"device-join","count":fresh}),
            );
        }
        Ok(devices)
    }
}

#[tauri::command]
fn resolve_host(ip: String) -> String {
    if let Ok(addr) = ip.parse::<std::net::IpAddr>() {
        if let Ok(name) = dns_lookup::lookup_addr(&addr) {
            return name;
        }
    }
    String::new()
}

#[tauri::command]
fn get_quota(state: tauri::State<'_, AppState>) -> Result<QuotaStatus, String> {
    let store = state.store.lock();
    let today = store.bytes_today(day_start_ts()).map_err(|e| e.to_string())?;
    let limit = state.quota_bytes_per_day.load(Ordering::Relaxed);
    Ok(QuotaStatus {
        bytes_today: today,
        limit_bytes: limit,
        exceeded: today >= limit,
    })
}

#[tauri::command]
fn set_quota(state: tauri::State<'_, AppState>, bytes_per_day: u64) -> Result<QuotaStatus, String> {
    state
        .quota_bytes_per_day
        .store(bytes_per_day, Ordering::Relaxed);
    let today = {
        let store = state.store.lock();
        let _ = store.set_setting("quota_bytes_per_day", &bytes_per_day.to_string());
        store.bytes_today(day_start_ts()).unwrap_or(0)
    };
    Ok(QuotaStatus {
        bytes_today: today,
        limit_bytes: bytes_per_day,
        exceeded: today >= bytes_per_day,
    })
}

// ---------- Per-app quotas ----------

#[tauri::command]
fn get_app_quotas(state: tauri::State<'_, AppState>) -> Result<Vec<AppQuota>, String> {
    let store = state.store.lock();
    store
        .list_app_quotas(day_start_ts())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_app_quota(
    state: tauri::State<'_, AppState>,
    exe: String,
    name: String,
    bytes_per_day: u64,
) -> Result<Vec<AppQuota>, String> {
    {
        let store = state.store.lock();
        store
            .set_app_quota(&exe, &name, bytes_per_day)
            .map_err(|e| e.to_string())?;
    }
    get_app_quotas(state)
}

#[tauri::command]
fn delete_app_quota(state: tauri::State<'_, AppState>, exe: String) -> Result<Vec<AppQuota>, String> {
    {
        let store = state.store.lock();
        store.delete_app_quota(&exe).map_err(|e| e.to_string())?;
    }
    get_app_quotas(state)
}

// ---------- Generic settings (allowlisted) ----------

const SETTING_KEYS: &[&str] = &[
    "vt_api_key",
    "remote_enabled",
    "remote_port",
    "remote_token",
    "remote_bind",
];

#[tauri::command]
fn get_setting(state: tauri::State<'_, AppState>, key: String) -> Result<String, String> {
    if !SETTING_KEYS.contains(&key.as_str()) {
        return Err("unknown setting".into());
    }
    let store = state.store.lock();
    store
        .get_setting(&key)
        .map(|v| v.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_setting(state: tauri::State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    if !SETTING_KEYS.contains(&key.as_str()) {
        return Err("unknown setting".into());
    }
    let store = state.store.lock();
    store.set_setting(&key, &value).map_err(|e| e.to_string())
}

// ---------- Autostart ----------

#[tauri::command]
fn is_autostart(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    let r = if enabled { al.enable() } else { al.disable() };
    r.map_err(|e| e.to_string())?;
    Ok(is_autostart(app))
}

// ---------- VirusTotal ----------

#[tauri::command]
async fn vt_lookup(state: tauri::State<'_, AppState>, exe: String) -> Result<VtResult, String> {
    let key = {
        let store = state.store.lock();
        store
            .get_setting("vt_api_key")
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    };
    if key.trim().is_empty() {
        return Err("no-key: set your VirusTotal API key in Settings first".into());
    }
    Ok(vt::lookup(&exe, key.trim()).await)
}

// ---------- Remote monitoring ----------

fn remote_cfg(state: &AppState) -> RemoteConfig {
    let store = state.store.lock();
    let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
    let enabled = get("remote_enabled") == "1";
    let port = get("remote_port")
        .parse::<u16>()
        .unwrap_or(DEFAULT_REMOTE_PORT);
    let mut token = get("remote_token");
    if token.is_empty() {
        token = default_token();
        let _ = store.set_setting("remote_token", &token);
    }
    RemoteConfig { enabled, port, token }
}

fn remote_status_of(state: &AppState) -> RemoteStatus {
    let cfg = remote_cfg(state);
    RemoteStatus {
        running: state.running_port.lock().is_some(),
        port: cfg.port,
        peers: state.remote_peers.load(Ordering::SeqCst),
        token_set: !cfg.token.is_empty(),
    }
}

fn start_remote(app: &AppHandle, cfg: &RemoteConfig) -> Result<(), String> {
    let state: tauri::State<'_, AppState> = app.state();
    if let Some(h) = state.remote_handle.lock().take() {
        h.abort();
    }
    *state.running_port.lock() = None;
    if !cfg.enabled {
        return Ok(());
    }
    let bind = {
        let store = state.store.lock();
        store
            .get_setting("remote_bind")
            .ok()
            .flatten()
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| "127.0.0.1".to_string())
    };
    // Pre-flight sync bind check so we can report "port in use" honestly.
    let addr = format!("{}:{}", bind, cfg.port);
    let probe = std::net::TcpListener::bind(&addr).map_err(|e| format!("bind {}: {}", addr, e))?;
    drop(probe);
    let tx = state.remote_tx.clone();
    let peers = state.remote_peers.clone();
    let token = cfg.token.clone();
    let port = cfg.port;
    let h = tauri::async_runtime::spawn(async move {
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => remote::serve(l, token, tx, peers).await,
            Err(e) => eprintln!("[netwire] remote bind failed: {}", e),
        }
    });
    *state.remote_handle.lock() = Some(h);
    *state.running_port.lock() = Some(port);
    Ok(())
}

#[tauri::command]
fn get_remote_config(state: tauri::State<'_, AppState>) -> RemoteConfig {
    remote_cfg(&state)
}

#[tauri::command]
fn set_remote_config(
    app: AppHandle,
    enabled: bool,
    port: u16,
    token: String,
    bind: String,
) -> Result<RemoteStatus, String> {
    {
        let state: tauri::State<'_, AppState> = app.state();
        let store = state.store.lock();
        store
            .set_setting("remote_enabled", if enabled { "1" } else { "0" })
            .map_err(|e| e.to_string())?;
        store
            .set_setting("remote_port", &port.to_string())
            .map_err(|e| e.to_string())?;
        let token = if token.trim().is_empty() {
            default_token()
        } else {
            token.trim().to_string()
        };
        store
            .set_setting("remote_token", &token)
            .map_err(|e| e.to_string())?;
        let bind = if bind.trim().is_empty() {
            "127.0.0.1".to_string()
        } else {
            bind.trim().to_string()
        };
        store
            .set_setting("remote_bind", &bind)
            .map_err(|e| e.to_string())?;
    }
    let state: tauri::State<'_, AppState> = app.state();
    let cfg = remote_cfg(&state);
    start_remote(&app, &cfg)?;
    Ok(remote_status_of(&state))
}

#[tauri::command]
fn remote_status(state: tauri::State<'_, AppState>) -> RemoteStatus {
    remote_status_of(&state)
}

// ---------- Export (Rust-side save dialog: no JS capability needed) ----------

#[tauri::command]
fn export_history_csv(
    app: AppHandle,
    from_ts: i64,
    to_ts: i64,
) -> Result<String, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let state: tauri::State<'_, AppState> = app.state();
    let rows = {
        let store = state.store.lock();
        store.export_rows(from_ts, to_ts).map_err(|e| e.to_string())?
    };
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("CSV", &["csv"])
        .set_file_name("netwire-history.csv")
        .blocking_save_file()
    else {
        return Err("cancelled".into());
    };
    let dest: std::path::PathBuf = match &path {
        FilePath::Path(p) => p.clone().into(),
        _ => return Err("unsupported path".into()),
    };
    let mut csv = String::from("timestamp,app,up_bytes,down_bytes\n");
    for (ts, app_name, up, down) in rows {
        let app_name = app_name.replace('"', "'");
        csv.push_str(&format!("{},\"{}\",{},{}\n", ts, app_name, up, down));
    }
    std::fs::write(&dest, csv).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
fn export_connections_csv(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let state: tauri::State<'_, AppState> = app.state();
    let conns = state.last_conns.read().clone();
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("CSV", &["csv"])
        .set_file_name("netwire-connections.csv")
        .blocking_save_file()
    else {
        return Err("cancelled".into());
    };
    let dest: std::path::PathBuf = match &path {
        FilePath::Path(p) => p.clone().into(),
        _ => return Err("unsupported path".into()),
    };
    let mut csv = String::from("proto,local,remote,pid,app,state,up_bytes,down_bytes,measured\n");
    for c in conns {
        csv.push_str(&format!(
            "{},{},{},{},\"{}\",{},{},{},{}\n",
            c.proto,
            c.local,
            c.remote,
            c.pid,
            c.app.replace('"', "'"),
            c.state,
            c.up_bytes,
            c.down_bytes,
            c.measured
        ));
    }
    std::fs::write(&dest, csv).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
fn get_geo_status() -> &'static str {
    "missing-mmdb"
}

// ---------- Tray (sparkline mini-graph) ----------

fn tray_sparkline(history: &[f64]) -> tauri::image::Image<'_> {
    const W: u32 = 64;
    const H: u32 = 20;
    let mut px = vec![11u8; (W * H * 4) as usize];
    for i in 0..(W * H) as usize {
        px[i * 4] = 11;
        px[i * 4 + 1] = 15;
        px[i * 4 + 2] = 22;
        px[i * 4 + 3] = 255;
    }
    let max = history.iter().cloned().fold(1.0f64, f64::max);
    let n = history.len().max(1);
    let denom = n.saturating_sub(1).max(1);
    let mut prev_y = H - 1;
    for (i, v) in history.iter().enumerate() {
        let x = ((i * (W as usize - 1) / denom).min(W as usize - 1)) as u32;
        let y = H - 1 - ((v / max * (H - 1) as f64) as u32).min(H - 1);
        // vertical bar from y to bottom (area) dim + bright line at y
        for yy in y..H {
            let idx = ((yy * W + x) * 4) as usize;
            px[idx] = 14;
            px[idx + 1] = 60;
            px[idx + 2] = 90;
        }
        let idx = ((y * W + x) * 4) as usize;
        px[idx] = 56;
        px[idx + 1] = 189;
        px[idx + 2] = 248;
        // connect from previous point (simple vertical stitch)
        let (a, b) = if prev_y < y { (prev_y, y) } else { (y, prev_y) };
        for yy in a..=b {
            let idx = ((yy * W + x) * 4) as usize;
            px[idx] = 56;
            px[idx + 1] = 189;
            px[idx + 2] = 248;
        }
        prev_y = y;
    }
    tauri::image::Image::new_owned(px, W, H)
}

fn build_tray(app: &AppHandle) -> anyhow::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(app, "show", "Show NetWire", true, None::<&str>)?;
    let rescan = MenuItem::with_id(app, "rescan", "Rescan LAN", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit NetWire", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &rescan, &quit])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no default icon"))?;

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("NetWire")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "rescan" => {
                let h = app.clone();
                std::thread::spawn(move || {
                    let _ = scan_lan(h);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(true) {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
        })
        .build(app)?;
    let state: tauri::State<'_, AppState> = app.state();
    *state.tray.lock() = Some(tray);
    Ok(())
}

// ---------- Background monitor loop ----------

fn spawn_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let mut poller = monitor::Poller::new();
        let app_data = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let mut enricher = enrich::Enricher::new(&app_data);
        let mut flows = estats::FlowTracker::new();

        let mut prev_rx = 0u64;
        let mut prev_tx = 0u64;
        let mut prev_t = Instant::now();
        let mut first = true;
        let mut tick_n = 0u64;
        let mut quota_alerted_day: i64 = -1;
        let mut rate_history: Vec<f64> = Vec::with_capacity(48);

        loop {
            let loop_start = Instant::now();
            let now = now_ts();

            match poller.snapshot() {
                Ok(snap) => {
                    let dt = prev_t.elapsed().as_secs_f64().max(0.2);
                    let (drx, dtx) = if first {
                        (0, 0)
                    } else {
                        (
                            snap.total_rx.saturating_sub(prev_rx),
                            snap.total_tx.saturating_sub(prev_tx),
                        )
                    };
                    first = false;
                    prev_rx = snap.total_rx;
                    prev_tx = snap.total_tx;
                    prev_t = Instant::now();

                    // Byte-accurate TCP flows (Windows EStats) or estimate fallback.
                    let flow = flows.tick(&snap.sockets);

                    // rDNS best-effort for up to 4 unknown remotes per tick.
                    let mut need: Vec<String> = Vec::new();
                    for s in snap.sockets.iter().take(40) {
                        if s.proto != "TCP" {
                            continue;
                        }
                        if !enricher.rdns_cache.contains_key(&s.remote_ip) && need.len() < 4 {
                            need.push(s.remote_ip.clone());
                        }
                    }
                    if !need.is_empty() {
                        enricher.resolve_batch(&need, 4);
                    }
                    let geo: HashMap<String, String> = need
                        .iter()
                        .map(|ip| (ip.clone(), enricher.country_for(ip)))
                        .collect();

                    let apps = monitor::build_app_stats(
                        &snap,
                        drx,
                        dtx,
                        dt,
                        &flow,
                        &enricher.rdns_cache,
                        &geo,
                    );

                    let tick = TrafficTick {
                        ts: now,
                        up_rate: if dt > 0.0 { dtx as f64 / dt } else { 0.0 },
                        down_rate: if dt > 0.0 { drx as f64 / dt } else { 0.0 },
                        total_up: snap.total_tx,
                        total_down: snap.total_rx,
                        apps,
                        per_app_estimated: !flow.active,
                        needs_admin: flow.needs_admin,
                    };

                    // Live connections snapshot (for the Conns tab + CSV export).
                    {
                        use crate::estats::flow_key;
                        let udp_count = snap
                            .sockets
                            .iter()
                            .filter(|s| !(flow.active && s.proto == "TCP"))
                            .count()
                            .max(1) as f64;
                        let res_rx =
                            drx.saturating_sub(flow.tcp_rx.min(drx)) as f64 / udp_count;
                        let res_tx =
                            dtx.saturating_sub(flow.tcp_tx.min(dtx)) as f64 / udp_count;
                        let mut conns: Vec<ConnectionStat> = snap
                            .sockets
                            .iter()
                            .map(|s| {
                                let (pname, pexe) = snap
                                    .proc_names
                                    .get(&s.pid)
                                    .cloned()
                                    .unwrap_or_else(|| ("unknown".into(), String::new()));
                                let (down, up, measured) =
                                    if flow.active && s.proto == "TCP" {
                                        match flow.deltas.get(&flow_key(s)) {
                                            Some(&(di, do_)) => (di, do_, true),
                                            None => (0, 0, true),
                                        }
                                    } else {
                                        (res_rx as u64, res_tx as u64, false)
                                    };
                                ConnectionStat {
                                    proto: s.proto.to_string(),
                                    local: format!("{}:{}", s.local_ip, s.local_port),
                                    remote: if s.proto == "TCP" {
                                        format!("{}:{}", s.remote_ip, s.remote_port)
                                    } else {
                                        format!("{}:{} (local)", s.local_ip, s.local_port)
                                    },
                                    pid: s.pid,
                                    app: pname,
                                    exe: pexe,
                                    state: if s.proto == "TCP" {
                                        "ACTIVE".to_string()
                                    } else {
                                        "—".to_string()
                                    },
                                    up_bytes: up,
                                    down_bytes: down,
                                    up_rate: if dt > 0.0 { up as f64 / dt } else { 0.0 },
                                    down_rate: if dt > 0.0 { down as f64 / dt } else { 0.0 },
                                    measured,
                                }
                            })
                            .collect();
                        conns.sort_by(|a, b| {
                            (b.down_rate + b.up_rate)
                                .partial_cmp(&(a.down_rate + a.up_rate))
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        conns.truncate(300);
                        let state: tauri::State<'_, AppState> = app.state();
                        *state.last_conns.write() = conns;
                    }

                    {
                        let state: tauri::State<'_, AppState> = app.state();
                        // First-seen + threat checks (bounded to top 10 apps/tick).
                        for a in tick.apps.iter().take(10) {
                            if a.exe.is_empty() {
                                continue;
                            }
                            let mut s = state.store.lock();
                            if let Ok(true) = s.is_new_app(&a.exe, now) {
                                let _ = s.push_alert(
                                    now,
                                    "new-app",
                                    &a.name,
                                    &format!(
                                        "{} accessed the network for the first time",
                                        a.name
                                    ),
                                    "info",
                                );
                                let _ = app.emit(
                                    "alert-raised",
                                    serde_json::json!({"kind":"new-app","app":a.name}),
                                );
                            }
                            for h in &a.hosts {
                                if let Ok(true) = s.is_threat(&h.ip) {
                                    let seen =
                                        state.known_threat_hits.read().contains(&h.ip);
                                    if !seen {
                                        state.known_threat_hits.write().insert(h.ip.clone());
                                        let _ = s.push_alert(
                                            now,
                                            "threat-ip",
                                            &a.name,
                                            &format!(
                                                "{} talked to flagged IP {}",
                                                a.name, h.ip
                                            ),
                                            "high",
                                        );
                                        notify(
                                            &app,
                                            "NetWire threat",
                                            &format!("{} → flagged {}", a.name, h.ip),
                                        );
                                        let _ = app.emit(
                                            "alert-raised",
                                            serde_json::json!({"kind":"threat-ip","app":a.name,"ip":h.ip}),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    tick_n += 1;
                    {
                        let state: tauri::State<'_, AppState> = app.state();
                        let mut s = state.store.lock();
                        if tick_n % DB_WRITE_EVERY_TICKS == 0 {
                            let _ = s.insert_tick(&tick);
                        }
                        let limit = state.quota_bytes_per_day.load(Ordering::Relaxed);
                        if let Ok(today) = s.bytes_today(day_start_ts()) {
                            let day = now / 86400;
                            if today >= limit && quota_alerted_day != day {
                                quota_alerted_day = day;
                                let _ = s.push_alert(
                                    now,
                                    "quota",
                                    "",
                                    &format!(
                                        "Daily data quota exceeded ({:.2} GiB)",
                                        today as f64 / 1073741824.0
                                    ),
                                    "warn",
                                );
                                notify(&app, "NetWire quota", "Daily data quota exceeded");
                                let _ =
                                    app.emit("alert-raised", serde_json::json!({"kind":"quota"}));
                            }
                        }
                        // Per-app quotas (every 10 ticks is plenty).
                        if tick_n % 10 == 0 {
                            if let Ok(quotas) = s.list_app_quotas(day_start_ts()) {
                                let day = now / 86400;
                                for q in quotas {
                                    if q.bytes_today >= q.bytes_per_day
                                        && !state
                                            .quota_hits
                                            .read()
                                            .contains(&(q.exe.clone(), day))
                                    {
                                        state
                                            .quota_hits
                                            .write()
                                            .insert((q.exe.clone(), day));
                                        let _ = s.push_alert(
                                            now,
                                            "app-quota",
                                            &q.name,
                                            &format!(
                                                "{} exceeded its daily quota ({:.1} MiB)",
                                                if q.name.is_empty() { &q.exe } else { &q.name },
                                                q.bytes_today as f64 / 1048576.0
                                            ),
                                            "warn",
                                        );
                                        notify(
                                            &app,
                                            "NetWire app quota",
                                            &format!("{} over quota", q.name),
                                        );
                                        let _ = app.emit(
                                            "alert-raised",
                                            serde_json::json!({"kind":"app-quota","app":q.exe}),
                                        );
                                    }
                                }
                            }
                        }
                        if tick_n % 3600 == 0 {
                            let _ = s.prune(KEEP_DAYS * 86400, now);
                        }
                    }

                    {
                        let state: tauri::State<'_, AppState> = app.state();
                        *state.last_tick.write() = Some(tick.clone());
                        // Tray tooltip + sparkline (every 2s for the icon).
                        rate_history.push(tick.down_rate);
                        if rate_history.len() > 48 {
                            rate_history.remove(0);
                        }
                        if let Some(tray) = state.tray.lock().as_ref() {
                            let _ = tray.set_tooltip(Some(format!(
                                "NetWire ↓{} ↑{}",
                                human_rate(tick.down_rate),
                                human_rate(tick.up_rate)
                            )));
                            if tick_n % 2 == 0 {
                                let _ =
                                    tray.set_icon(Some(tray_sparkline(&rate_history)));
                            }
                        }
                        // Remote subscribers.
                        if state.running_port.lock().is_some() {
                            if let Ok(json) = serde_json::to_string(&tick) {
                                let _ = state.remote_tx.send(json);
                            }
                        }
                    }
                    let _ = app.emit("traffic-tick", &tick);
                }
                Err(e) => {
                    eprintln!("[netwire] snapshot failed: {:#}", e);
                }
            }

            let elapsed = loop_start.elapsed();
            if elapsed < Duration::from_secs(TICK_SECS) {
                std::thread::sleep(Duration::from_secs(TICK_SECS) - elapsed);
            }
        }
    });
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

// ---------- Entry ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tmp = std::env::temp_dir().join("netwire-bootstrap.db");
    let bootstrap = store::Store::open(&tmp).expect("bootstrap db");
    let quota = bootstrap
        .get_setting("quota_bytes_per_day")
        .ok()
        .flatten()
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(DEFAULT_QUOTA);
    let (remote_tx, _rx) = tokio::sync::broadcast::channel::<String>(32);

    tauri::Builder::default()
        .manage(AppState {
            store: Arc::new(Mutex::new(bootstrap)),
            last_tick: parking_lot::RwLock::new(None),
            last_conns: parking_lot::RwLock::new(Vec::new()),
            quota_bytes_per_day: AtomicU64::new(quota),
            known_threat_hits: parking_lot::RwLock::new(HashSet::new()),
            quota_hits: parking_lot::RwLock::new(HashSet::new()),
            tray: Mutex::new(None),
            remote_tx,
            remote_handle: Mutex::new(None),
            remote_peers: Arc::new(AtomicUsize::new(0)),
            running_port: Mutex::new(None),
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            // Reopen DB in the real app-data dir and swap it in.
            if let Ok(dir) = app.path().app_data_dir() {
                let db = dir.join("netwire.db");
                if let Ok(fresh) = store::Store::open(&db) {
                    if let Ok(v) = fresh.get_setting("quota_bytes_per_day") {
                        if let Some(n) = v.and_then(|x| x.parse::<u64>().ok()) {
                            app.state::<AppState>()
                                .quota_bytes_per_day
                                .store(n, Ordering::Relaxed);
                        }
                    }
                    *app.state::<AppState>().store.lock() = fresh;
                }
            }
            if let Err(e) = build_tray(app.handle()) {
                eprintln!("[netwire] tray unavailable: {:#}", e);
            }
            // Resume remote server if it was left enabled.
            {
                let state = app.state::<AppState>();
                let cfg = remote_cfg(&state);
                if cfg.enabled {
                    if let Err(e) = start_remote(app.handle(), &cfg) {
                        eprintln!("[netwire] remote resume failed: {}", e);
                    }
                }
            }
            let handle = app.handle().clone();
            spawn_monitor(handle);
            let h = app.handle().clone();
            std::thread::spawn(move || {
                let _ = scan_lan(h);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_live_snapshot,
            get_connections,
            get_history,
            get_alerts,
            mark_alerts_read,
            get_devices,
            scan_lan,
            resolve_host,
            get_quota,
            set_quota,
            get_app_quotas,
            set_app_quota,
            delete_app_quota,
            get_setting,
            set_setting,
            is_autostart,
            set_autostart,
            vt_lookup,
            get_remote_config,
            set_remote_config,
            remote_status,
            export_history_csv,
            export_connections_csv,
            get_geo_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running NetWire");
}
