//! Cross-platform live capture for V1 (monitor-only, no driver).
//!
//! Strategy:
//! - Totals: accurate per-interface byte counters via `sysinfo::Networks`.
//! - Per-app / per-host: socket table via `netstat2` (IP Helper on Windows,
//!   /proc on Linux, libproc on macOS) + process names via `sysinfo`.
//! - Per-app bytes: proportional estimate = iface delta split by each app's
//!   active-connection share. Honest + upgradeable: ETW/eBPF per-flow byte
//!   counters slot in here for V2 alongside the firewall.

use anyhow::Result;
use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};
use std::collections::HashMap;
use sysinfo::{Networks, ProcessesToUpdate, System};

use crate::estats::FlowSample;
use crate::models::{AppStat, HostStat};

pub struct SocketEntry {
    pub pid: u32,
    pub proto: &'static str,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_ip: String,
    pub remote_port: u16,
}

pub struct Snapshot {
    pub total_rx: u64,
    pub total_tx: u64,
    pub sockets: Vec<SocketEntry>,
    pub proc_names: HashMap<u32, (String, String)>,
}

pub struct Poller {
    sys: System,
    nets: Networks,
}

impl Poller {
    pub fn new() -> Self {
        Self {
            sys: System::new_all(),
            nets: Networks::new_with_refreshed_list(),
        }
    }

    pub fn snapshot(&mut self) -> Result<Snapshot> {
        // 1. Interface totals (skip loopback).
        self.nets.refresh(true);
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        for (_name, data) in self.nets.iter() {
            let n = _name.to_lowercase();
            if n.contains("loopback") || n.contains("lo") {
                continue;
            }
            total_rx += data.total_received();
            total_tx += data.total_transmitted();
        }

        // 2. Process names (cached refresh, cheap enough at 1Hz for V1).
        self.sys
            .refresh_processes(ProcessesToUpdate::All, true);
        let mut proc_names: HashMap<u32, (String, String)> = HashMap::new();
        for (pid, p) in self.sys.processes() {
            let pid_u32 = pid.as_u32();
            let name = p.name().to_string_lossy().to_string();
            let exe = p
                .exe()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            proc_names.insert(pid_u32, (name, exe));
        }

        // 3. Socket table.
        let mut sockets = Vec::new();
        let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
        let proto = ProtocolFlags::TCP | ProtocolFlags::UDP;
        if let Ok(infos) = netstat2::get_sockets_info(af, proto) {
            for info in infos {
                let pid = info.associated_pids.first().copied().unwrap_or(0);
                match info.protocol_socket_info {
                    ProtocolSocketInfo::Tcp(t) => {
                        // netstat2 0.11: remote_addr is IpAddr + remote_port separately.
                        let rip = t.remote_addr;
                        let is_listener = t.remote_port == 0
                            || rip.is_loopback()
                            || rip.is_unspecified();
                        if !is_listener {
                            sockets.push(SocketEntry {
                                pid,
                                proto: "TCP",
                                local_ip: t.local_addr.to_string(),
                                local_port: t.local_port,
                                remote_ip: rip.to_string(),
                                remote_port: t.remote_port,
                            });
                        }
                    }
                    ProtocolSocketInfo::Udp(u) => {
                        // netstat2 exposes no UDP remote (connectionless); attribute the
                        // bound endpoint to the app so UDP-heavy apps keep their share.
                        // Host drilldown marks these as local endpoints.
                        let lip = u.local_addr;
                        if !(lip.is_loopback() || lip.is_unspecified()) {
                            sockets.push(SocketEntry {
                                pid,
                                proto: "UDP",
                                local_ip: lip.to_string(),
                                local_port: u.local_port,
                                remote_ip: lip.to_string(),
                                remote_port: 0,
                            });
                        }
                    }
                }
            }
        }

        Ok(Snapshot {
            total_rx,
            total_tx,
            sockets,
            proc_names,
        })
    }
}

/// Build per-app stats.
///
/// When `flow.active` (Windows EStats): TCP bytes are measured per-flow;
/// UDP takes the residual (iface delta minus measured TCP, clamped ≥ 0)
/// split across UDP sockets. Otherwise the legacy proportional estimate.
pub fn build_app_stats(
    snap: &Snapshot,
    delta_rx: u64,
    delta_tx: u64,
    dt: f64,
    flow: &FlowSample,
    rdns: &HashMap<String, String>,
    geo: &HashMap<String, String>,
) -> Vec<AppStat> {
    use crate::estats::flow_key;
    use std::collections::hash_map::Entry;

    struct HostAgg {
        conns: u32,
        up: u64,
        down: u64,
    }
    struct Agg {
        pid: u32,
        name: String,
        exe: String,
        conns: u32,
        tcp_up: u64,
        tcp_down: u64,
        udp_sockets: u32,
        hosts: HashMap<String, HostAgg>,
    }
    let mut by_app: HashMap<String, Agg> = HashMap::new();
    let mut udp_total = 0u32;

    for s in &snap.sockets {
        let (pname, pexe) = snap
            .proc_names
            .get(&s.pid)
            .cloned()
            .unwrap_or_else(|| ("unknown".into(), String::new()));
        let key = if pexe.is_empty() {
            pname.clone()
        } else {
            pexe.clone()
        };
        let agg = match by_app.entry(key) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => v.insert(Agg {
                pid: s.pid,
                name: pname,
                exe: pexe,
                conns: 0,
                tcp_up: 0,
                tcp_down: 0,
                udp_sockets: 0,
                hosts: HashMap::new(),
            }),
        };
        agg.conns += 1;
        let h = agg.hosts.entry(s.remote_ip.clone()).or_insert(HostAgg {
            conns: 0,
            up: 0,
            down: 0,
        });
        h.conns += 1;
        if flow.active && s.proto == "TCP" {
            if let Some(&(di, do_)) = flow.deltas.get(&flow_key(s)) {
                agg.tcp_down += di;
                agg.tcp_up += do_;
                h.down += di;
                h.up += do_;
            }
        } else {
            agg.udp_sockets += 1;
            udp_total += 1;
        }
    }

    // Residual for the unmeasured (UDP / IPv6) share.
    let res_rx = delta_rx.saturating_sub(flow.tcp_rx.min(delta_rx));
    let res_tx = delta_tx.saturating_sub(flow.tcp_tx.min(delta_tx));

    let total_conns: u32 = by_app.values().map(|a| a.conns).sum();
    let mut apps: Vec<AppStat> = Vec::new();

    for (_key, a) in by_app {
        let (mut up, mut down) = (a.tcp_up, a.tcp_down);
        if !flow.active {
            // Legacy proportional estimate.
            let share = if total_conns > 0 {
                a.conns as f64 / total_conns as f64
            } else {
                0.0
            };
            down = (delta_rx as f64 * share) as u64;
            up = (delta_tx as f64 * share) as u64;
        } else if udp_total > 0 && a.udp_sockets > 0 {
            down += (res_rx as f64 * a.udp_sockets as f64 / udp_total as f64) as u64;
            up += (res_tx as f64 * a.udp_sockets as f64 / udp_total as f64) as u64;
        }

        let mut hosts: Vec<HostStat> = a
            .hosts
            .into_iter()
            .map(|(ip, h)| {
                // Legacy path: split app bytes across hosts. Flow path: hosts
                // already hold measured TCP bytes; UDP residual is added below.
                let (hu, hd) = if !flow.active && a.conns > 0 {
                    let hs = h.conns as f64 / a.conns as f64;
                    ((up as f64 * hs) as u64, (down as f64 * hs) as u64)
                } else {
                    (h.up, h.down)
                };
                HostStat {
                    hostname: rdns.get(&ip).cloned().unwrap_or_default(),
                    country: geo.get(&ip).cloned().unwrap_or_default(),
                    ip,
                    connections: h.conns,
                    up_bytes: hu,
                    down_bytes: hd,
                }
            })
            .collect();

        // Distribute this app's UDP residual across its UDP-attributed hosts.
        if flow.active && udp_total > 0 && a.udp_sockets > 0 {
            let app_res_down =
                (res_rx as f64 * a.udp_sockets as f64 / udp_total as f64) as u64;
            let app_res_up = (res_tx as f64 * a.udp_sockets as f64 / udp_total as f64) as u64;
            let udp_hosts: Vec<&mut HostStat> = hosts.iter_mut().collect();
            // Hosts that only have measured TCP keep it; UDP share goes to hosts
            // proportionally to connection count (simple + stable).
            let tot: u32 = udp_hosts.iter().map(|h| h.connections).sum::<u32>().max(1);
            for h in udp_hosts {
                h.down_bytes += (app_res_down as f64 * h.connections as f64 / tot as f64) as u64;
                h.up_bytes += (app_res_up as f64 * h.connections as f64 / tot as f64) as u64;
            }
            // Note: TCP hosts also receive a slice of the residual here; acceptable
            // blend — totals stay exact, per-host TCP bytes stay measured-minimum.
        }

        hosts.sort_by(|x, y| (y.down_bytes + y.up_bytes).cmp(&(x.down_bytes + x.up_bytes)));
        hosts.truncate(12);

        apps.push(AppStat {
            pid: a.pid,
            name: a.name,
            exe: a.exe,
            connections: a.conns,
            up_bytes: up,
            down_bytes: down,
            up_rate: if dt > 0.0 { up as f64 / dt } else { 0.0 },
            down_rate: if dt > 0.0 { down as f64 / dt } else { 0.0 },
            hosts,
        });
    }

    apps.sort_by(|a, b| {
        (b.down_bytes + b.up_bytes).cmp(&(a.down_bytes + a.up_bytes))
    });
    apps.truncate(60);
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_does_not_panic() {
        let mut p = Poller::new();
        let snap = p.snapshot().unwrap();
        // Totals are cumulative counters; just assert the call path works.
        let _ = (snap.total_rx, snap.total_tx);
    }

    #[test]
    fn proportional_split_sums_to_delta() {
        use crate::estats::{FlowSample, FlowTracker};
        let snap = Snapshot {
            total_rx: 0,
            total_tx: 0,
            sockets: vec![
                SocketEntry { pid: 1, proto: "TCP", local_ip: "10.0.0.2".into(), local_port: 5001, remote_ip: "1.1.1.1".into(), remote_port: 443 },
                SocketEntry { pid: 1, proto: "TCP", local_ip: "10.0.0.2".into(), local_port: 5002, remote_ip: "1.1.1.1".into(), remote_port: 443 },
                SocketEntry { pid: 2, proto: "TCP", local_ip: "10.0.0.2".into(), local_port: 5003, remote_ip: "2.2.2.2".into(), remote_port: 443 },
                SocketEntry { pid: 2, proto: "TCP", local_ip: "10.0.0.2".into(), local_port: 5004, remote_ip: "2.2.2.2".into(), remote_port: 443 },
            ],
            proc_names: [(1, ("a".into(), "a.exe".into())), (2, ("b".into(), "b.exe".into()))]
                .into_iter()
                .collect(),
        };
        // Inactive flow → legacy proportional path.
        let flow = FlowSample { active: false, needs_admin: false, deltas: HashMap::new(), tcp_rx: 0, tcp_tx: 0 };
        let apps = build_app_stats(&snap, 300, 300, 1.0, &flow, &HashMap::new(), &HashMap::new());
        assert_eq!(apps.len(), 2);
        let total: u64 = apps.iter().map(|a| a.down_bytes).sum();
        assert!(total <= 300 && total >= 298, "total={}", total);
        let _ = FlowTracker::new();
    }
}
