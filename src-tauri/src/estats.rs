//! Byte-accurate per-connection accounting.
//!
//! Windows: polls `GetPerTcpConnectionEStats` (IP Helper, no driver/admin)
//! for every established IPv4 TCP connection. First sight enables collection,
//! subsequent ticks read `DataBytesIn/Out` deltas — real per-flow bytes.
//! UDP has no OS per-socket counters: it keeps the proportional share of the
//! residual (iface delta minus measured TCP). IPv6 TCP falls back the same way.
//! Non-Windows: stub (all estimates); eBPF fast-path is a V2-follow-up.

use std::collections::{HashMap, HashSet};

use crate::monitor::SocketEntry;

/// Per-tick measured TCP deltas, keyed by flow key.
pub struct FlowSample {
    pub active: bool,
    /// True when the OS refused per-flow counters (Windows: elevation required).
    pub needs_admin: bool,
    pub deltas: HashMap<String, (u64, u64)>, // key -> (in_delta, out_delta)
    pub tcp_rx: u64,
    pub tcp_tx: u64,
}

pub fn flow_key(s: &SocketEntry) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        s.proto, s.local_ip, s.local_port, s.remote_ip, s.remote_port
    )
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::net::Ipv4Addr;
    use windows::Win32::Foundation::BOOLEAN;
    use windows::Win32::NetworkManagement::IpHelper::{
        GetPerTcpConnectionEStats, SetPerTcpConnectionEStats, MIB_TCPROW_LH, MIB_TCPROW_LH_0,
        MIB_TCP_STATE_ESTAB, TCP_ESTATS_DATA_ROD_v0, TCP_ESTATS_DATA_RW_v0, TcpConnectionEstatsData,
    };

    pub struct FlowTracker {
        prev: HashMap<String, (u64, u64)>,
        enabled: HashSet<String>,
        logged_active: bool,
        logged_rc: bool,
        access_denied: bool,
    }

    impl FlowTracker {
        pub fn new() -> Self {
            Self {
                prev: HashMap::new(),
                enabled: HashSet::new(),
                logged_active: false,
                logged_rc: false,
                access_denied: false,
            }
        }

        fn make_row(s: &SocketEntry, lip: Ipv4Addr, rip: Ipv4Addr) -> MIB_TCPROW_LH {
            MIB_TCPROW_LH {
                // Union literal construction happens inside the unsafe blocks below.
                Anonymous: MIB_TCPROW_LH_0 { State: MIB_TCP_STATE_ESTAB },
                dwLocalAddr: u32::from_ne_bytes(lip.octets()),
                // Ports in MIB_TCPROW are network-byte-order.
                dwLocalPort: s.local_port.to_be() as u32,
                dwRemoteAddr: u32::from_ne_bytes(rip.octets()),
                dwRemotePort: s.remote_port.to_be() as u32,
            }
        }

        pub fn tick(&mut self, sockets: &[SocketEntry]) -> FlowSample {
            let mut deltas = HashMap::new();
            let mut tcp_rx = 0u64;
            let mut tcp_tx = 0u64;
            let mut seen = HashSet::new();

            for s in sockets {
                if s.proto != "TCP" {
                    continue;
                }
                let (Ok(lip), Ok(rip)) = (
                    s.local_ip.parse::<Ipv4Addr>(),
                    s.remote_ip.parse::<Ipv4Addr>(),
                ) else {
                    continue; // IPv6 → estimate path
                };
                if s.remote_port == 0 {
                    continue;
                }
                let key = flow_key(s);
                seen.insert(key.clone());
                let row = Self::make_row(s, lip, rip);
                unsafe {
                    if !self.enabled.contains(&key) {
                        let rw = TCP_ESTATS_DATA_RW_v0 {
                            EnableCollection: BOOLEAN(1),
                        };
                        let bytes = std::slice::from_raw_parts(
                            &rw as *const _ as *const u8,
                            std::mem::size_of::<TCP_ESTATS_DATA_RW_v0>(),
                        );
                        let rc = SetPerTcpConnectionEStats(
                            &row,
                            TcpConnectionEstatsData,
                            bytes,
                            0,
                            0,
                        );
                        if rc == 0 {
                            self.enabled.insert(key);
                        } else {
                            // rc=5 (ACCESS_DENIED) is the norm unelevated: the
                            // whole measured path needs admin on Windows.
                            if rc == 5 {
                                self.access_denied = true;
                            }
                            if !self.logged_rc {
                                self.logged_rc = true;
                                eprintln!("[netwire] EStats enable failed, rc={} (need admin?)", rc);
                            }
                        }
                        continue; // counters start now; deltas next tick
                    }
                    let mut rod: TCP_ESTATS_DATA_ROD_v0 = std::mem::zeroed();
                    let buf = std::slice::from_raw_parts_mut(
                        &mut rod as *mut _ as *mut u8,
                        std::mem::size_of::<TCP_ESTATS_DATA_ROD_v0>(),
                    );
                    let rc = GetPerTcpConnectionEStats(
                        &row,
                        TcpConnectionEstatsData,
                        None,
                        0,
                        None,
                        0,
                        Some(buf),
                        0,
                    );
                    if rc != 0 {
                        if !self.logged_rc {
                            self.logged_rc = true;
                            eprintln!("[netwire] EStats read failed, rc={}", rc);
                        }
                        continue;
                    }
                    let cur = (rod.DataBytesIn, rod.DataBytesOut);
                    if let Some(&(pin, pout)) = self.prev.get(&key) {
                        let di = cur.0.saturating_sub(pin);
                        let do_ = cur.1.saturating_sub(pout);
                        // Clamp absurd jumps (counter reset / row reuse).
                        if di < 1_000_000_000 && do_ < 1_000_000_000 {
                            deltas.insert(key.clone(), (di, do_));
                            tcp_rx += di;
                            tcp_tx += do_;
                        }
                    }
                    self.prev.insert(key, cur);
                }
            }

            // Drop dead flows so maps can't grow without bound.
            self.prev.retain(|k, _| seen.contains(k));
            // Keep `enabled` for reuse, but cap size.
            if self.enabled.len() > 4096 {
                self.enabled.retain(|k| seen.contains(k));
            }

            if !self.logged_active && !deltas.is_empty() {
                self.logged_active = true;
                eprintln!(
                    "[netwire] EStats per-flow accounting active ({} measured TCP flows)",
                    deltas.len()
                );
            }

            FlowSample {
                active: true,
                needs_admin: self.access_denied && deltas.is_empty(),
                deltas,
                tcp_rx,
                tcp_tx,
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub struct FlowTracker;

    impl FlowTracker {
        pub fn new() -> Self {
            Self
        }

        pub fn tick(&mut self, _sockets: &[SocketEntry]) -> FlowSample {
            FlowSample {
                active: false,
                needs_admin: false,
                deltas: HashMap::new(),
                tcp_rx: 0,
                tcp_tx: 0,
            }
        }
    }
}

pub use imp::FlowTracker;
