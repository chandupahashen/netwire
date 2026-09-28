//! Custom desktop notification center.
//!
//! Replaces raw OS toasts with branded NetWire toast windows (bottom-right
//! stack, themed, click-through + View action). Falls back to the native OS
//! toast only if the toast window cannot be created.
//!
//! Policy per dispatch:
//! 1. Per-kind toggle (`ntf_<kind>`, default on).
//! 2. Quiet hours (`ntf_quiet_from/to` "HH:MM", empty = off, wrap-aware).
//! 3. Burst coalescing: ≥3 of a kind within 15s collapse into one summary.
//! 4. Stack cap: max 4 visible, oldest `info` evicted first.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub const MAX_VISIBLE: usize = 4;
pub const BURST_WINDOW_SECS: i64 = 15;
pub const BURST_THRESHOLD: u64 = 3;
pub const TOAST_WIDTH: f64 = 372.0;
pub const TOAST_MAX_HEIGHT: f64 = 520.0;
pub const TOAST_MARGIN: f64 = 16.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToastPayload {
    pub id: u64,
    pub kind: String,
    pub app: String,
    pub message: String,
    pub severity: String,
    pub ts: i64,
    pub duration_ms: u64,
    /// Deep-link target for the View action.
    pub view_tab: String,
    pub view_exe: String,
}

pub struct ToastCenter {
    next_id: AtomicU64,
    active: parking_lot::Mutex<Vec<ToastPayload>>,
    burst: parking_lot::Mutex<HashMap<String, (i64, u64)>>,
    height: AtomicU32,
}

impl ToastCenter {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            active: parking_lot::Mutex::new(Vec::new()),
            burst: parking_lot::Mutex::new(HashMap::new()),
            height: AtomicU32::new(120),
        }
    }

    pub fn list(&self) -> Vec<ToastPayload> {
        self.active.lock().clone()
    }

    /// Push, returning the payload to broadcast (None when coalesced away).
    pub fn push(&self, mut t: ToastPayload, now: i64) -> Option<ToastPayload> {
        // Burst coalescing.
        {
            let mut burst = self.burst.lock();
            let e = burst.entry(t.kind.clone()).or_insert((now, 0));
            if now - e.0 > BURST_WINDOW_SECS {
                *e = (now, 0);
            }
            e.1 += 1;
            if e.1 >= BURST_THRESHOLD {
                // Collapse same-kind actives into one summary.
                let mut active = self.active.lock();
                active.retain(|x| x.kind != t.kind);
                t.id = self.next_id.fetch_add(1, Ordering::SeqCst);
                t.app = String::new();
                t.message = summary_text(&t.kind, e.1);
                active.push(t.clone());
                return Some(t);
            }
        }
        t.id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut active = self.active.lock();
        active.push(t.clone());
        // Cap: evict oldest info first, else oldest overall.
        while active.len() > MAX_VISIBLE {
            if let Some(i) = active.iter().position(|x| x.severity == "info") {
                active.remove(i);
            } else {
                active.remove(0);
            }
        }
        Some(t)
    }

    /// Remove by id, returning the removed payload (for action handling).
    pub fn remove(&self, id: u64) -> Option<ToastPayload> {
        let mut active = self.active.lock();
        active
            .iter()
            .position(|x| x.id == id)
            .map(|i| active.remove(i))
    }

    pub fn set_height(&self, h: u32) {
        self.height.store(h, Ordering::SeqCst);
    }

    pub fn height(&self) -> u32 {
        self.height
            .load(Ordering::SeqCst)
            .clamp(80, TOAST_MAX_HEIGHT as u32)
    }
}

impl Default for ToastCenter {
    fn default() -> Self {
        Self::new()
    }
}

/// "3 new apps accessed the network" style summaries.
pub fn summary_text(kind: &str, count: u64) -> String {
    match kind {
        "new-app" => format!("{} new apps accessed the network", count),
        "device-join" => format!("{} new devices joined the LAN", count),
        "threat-ip" => format!("{} flagged-IP contacts", count),
        "quota" => "Data quota exceeded (repeated)".to_string(),
        "app-quota" => "App quotas exceeded (repeated)".to_string(),
        _ => format!("{} × {}", count, kind),
    }
}

pub fn view_tab_for(kind: &str) -> &'static str {
    match kind {
        "new-app" | "threat-ip" | "app-quota" => "usage",
        "device-join" => "things",
        "quota" => "live",
        _ => "alerts",
    }
}

pub fn duration_for(severity: &str, scale: f32) -> u64 {
    let base = match severity {
        "high" => 15_000,
        "warn" => 9_000,
        _ => 6_000,
    };
    (base as f32 * scale).max(2_000.0) as u64
}

pub fn duration_scale(setting: &str) -> f32 {
    match setting {
        "short" => 0.6,
        "long" => 1.6,
        _ => 1.0,
    }
}

/// Quiet-hours check. `from`/`to` are "HH:MM" (local); empty = off.
/// Handles overnight wraps (22:00 → 07:00).
pub fn in_quiet_hours(now_min: u16, from: &str, to: &str) -> bool {
    let parse = |s: &str| -> Option<u16> {
        let mut it = s.split(':');
        let h: u16 = it.next()?.trim().parse().ok()?;
        let m: u16 = it.next()?.trim().parse().ok()?;
        if h > 23 || m > 59 {
            return None;
        }
        Some(h * 60 + m)
    };
    let (Some(f), Some(t)) = (parse(from), parse(to)) else {
        return false;
    };
    if f == t {
        return false;
    }
    if f < t {
        now_min >= f && now_min < t
    } else {
        now_min >= f || now_min < t
    }
}

/// Minutes since local midnight.
pub fn local_now_min() -> u16 {
    let now = chrono::Local::now();
    use chrono::Timelike;
    (now.hour() * 60 + now.minute()) as u16
}

// ---------- Toast window (created lazily on first popup) ----------

/// Ensure the toast window exists, is sized/positioned, and visible.
/// Never steals focus (built `focused(false)`, only `show()`n).
pub fn show_toasts(app: &tauri::AppHandle, height: u32) -> anyhow::Result<()> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
    let win = match app.get_webview_window("toast") {
        Some(w) => w,
        None => {
            WebviewWindowBuilder::new(app, "toast", WebviewUrl::App("index.html?toast=1".into()))
                .title("NetWire")
                .inner_size(TOAST_WIDTH, height.max(80) as f64)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .visible(false)
                .build()?
        }
    };
    place(&win, app, height)?;
    win.show()?;
    Ok(())
}

fn place(win: &tauri::WebviewWindow, app: &tauri::AppHandle, height: u32) -> anyhow::Result<()> {
    if let Ok(Some(mon)) = app.primary_monitor() {
        let scale = mon.scale_factor();
        let sz = mon.size();
        let w = (TOAST_WIDTH * scale) as i32;
        let h = ((height.max(80) as f64) * scale) as i32;
        let m = (TOAST_MARGIN * scale) as i32;
        win.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: sz.width as i32 - w - m,
            y: sz.height as i32 - h - m,
        }))?;
    }
    Ok(())
}

/// Resize + reposition after content change; hide when the stack empties.
pub fn sync_toast_window(app: &tauri::AppHandle, height: u32, count: usize) {
    use tauri::Manager;
    let Some(w) = app.get_webview_window("toast") else {
        return;
    };
    if count == 0 {
        let _ = w.hide();
        return;
    }
    let h = height.max(80) as f64;
    let _ = w.set_size(tauri::LogicalSize {
        width: TOAST_WIDTH,
        height: h,
    });
    let _ = place(&w, app, height);
    let _ = w.show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_hours_day_window() {
        assert!(in_quiet_hours(12 * 60, "09:00", "17:00"));
        assert!(!in_quiet_hours(8 * 60 + 59, "09:00", "17:00"));
        assert!(!in_quiet_hours(17 * 60, "09:00", "17:00"));
    }

    #[test]
    fn quiet_hours_overnight_wrap() {
        assert!(in_quiet_hours(23 * 60, "22:00", "07:00"));
        assert!(in_quiet_hours(3 * 60, "22:00", "07:00"));
        assert!(!in_quiet_hours(12 * 60, "22:00", "07:00"));
    }

    #[test]
    fn quiet_hours_off_and_invalid() {
        assert!(!in_quiet_hours(12 * 60, "", ""));
        assert!(!in_quiet_hours(12 * 60, "09:00", "09:00"));
        assert!(!in_quiet_hours(12 * 60, "xx", "17:00"));
    }

    #[test]
    fn burst_coalesces_on_third() {
        let c = ToastCenter::new();
        let mk = |app: &str| ToastPayload {
            id: 0,
            kind: "new-app".into(),
            app: app.into(),
            message: "x".into(),
            severity: "info".into(),
            ts: 100,
            duration_ms: 6000,
            view_tab: "usage".into(),
            view_exe: String::new(),
        };
        assert!(c.push(mk("a"), 100).is_some());
        assert!(c.push(mk("b"), 101).is_some());
        let third = c.push(mk("c"), 102).unwrap();
        assert_eq!(c.list().len(), 1, "burst should collapse to summary");
        assert!(
            third.message.contains("3 new apps"),
            "got: {}",
            third.message
        );
    }

    #[test]
    fn stack_caps_at_four_info_first() {
        let c = ToastCenter::new();
        for i in 0..6 {
            c.push(
                ToastPayload {
                    id: 0,
                    kind: format!("k{}", i),
                    app: String::new(),
                    message: "m".into(),
                    severity: if i == 5 { "high".into() } else { "info".into() },
                    ts: 100 + i,
                    duration_ms: 6000,
                    view_tab: "alerts".into(),
                    view_exe: String::new(),
                },
                100 + i,
            );
        }
        let list = c.list();
        assert_eq!(list.len(), MAX_VISIBLE);
        assert!(
            list.iter().any(|t| t.severity == "high"),
            "high must survive"
        );
    }
}
