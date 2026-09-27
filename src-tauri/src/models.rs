use serde::{Deserialize, Serialize};

/// One app's live usage within a tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStat {
    pub pid: u32,
    pub name: String,
    pub exe: String,
    pub connections: u32,
    /// Estimated bytes (see monitor.rs) — accurate totals, estimated split.
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub up_rate: f64,
    pub down_rate: f64,
    pub hosts: Vec<HostStat>,
}

/// One remote endpoint rollup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostStat {
    pub ip: String,
    pub hostname: String,
    pub country: String,
    pub connections: u32,
    pub up_bytes: u64,
    pub down_bytes: u64,
}

/// 1-second aggregated tick broadcast as `traffic-tick`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficTick {
    pub ts: i64,
    pub up_rate: f64,
    pub down_rate: f64,
    pub total_up: u64,
    pub total_down: u64,
    pub apps: Vec<AppStat>,
    /// True when per-app bytes are proportional estimates (no driver).
    pub per_app_estimated: bool,
    /// True when measured counters were refused (Windows: run as admin).
    pub needs_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub ts: i64,
    pub app: String,
    pub up_bytes: u64,
    pub down_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertItem {
    pub id: i64,
    pub ts: i64,
    pub kind: String,
    pub app: String,
    pub message: String,
    pub severity: String,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceItem {
    pub mac: String,
    pub ip: String,
    pub vendor: String,
    pub last_seen: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaStatus {
    pub bytes_today: u64,
    pub limit_bytes: u64,
    pub exceeded: bool,
}

/// One live transport connection (Live Connections view).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStat {
    pub proto: String,
    pub local: String,
    pub remote: String,
    pub pid: u32,
    pub app: String,
    pub exe: String,
    pub state: String,
    pub up_bytes: u64,
    pub down_bytes: u64,
    pub up_rate: f64,
    pub down_rate: f64,
    /// True when bytes came from OS per-flow counters (EStats), not estimates.
    pub measured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppQuota {
    pub exe: String,
    pub name: String,
    pub bytes_per_day: u64,
    pub bytes_today: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VtResult {
    pub exe: String,
    pub sha256: String,
    pub malicious: i64,
    pub suspicious: i64,
    pub harmless: i64,
    pub undetected: i64,
    pub permalink: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStatus {
    pub running: bool,
    pub port: u16,
    pub peers: usize,
    pub token_set: bool,
}
