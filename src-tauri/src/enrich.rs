//! Best-effort enrichment: rDNS cache, GeoIP (local MMDB if present),
//! LAN device discovery via `arp -a`, OUI vendor guess, threat-list stub.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{mpsc::Receiver, Arc};

/// Shared rDNS cache: written by the resolver thread, read by the monitor
/// loop. Keeps blocking DNS (multi-second stalls on timeout) off the 1s tick.
pub type RdnsCache = Arc<parking_lot::RwLock<HashMap<String, String>>>;

/// Dedicated resolver thread: drains batches, resolves misses (with negative
/// caching), never blocks the monitor loop. Drops batches when it falls
/// behind — the next tick re-queues anything still unknown.
pub fn spawn_resolver(rx: Receiver<Vec<String>>, cache: RdnsCache) {
    std::thread::Builder::new()
        .name("netwire-rdns".into())
        .spawn(move || {
            while let Ok(batch) = rx.recv() {
                // Drain any backlog; only the freshest batch matters.
                let mut latest = batch;
                while let Ok(next) = rx.try_recv() {
                    latest = next;
                }
                let mut done = 0;
                for ip in latest {
                    if done >= 8 {
                        break;
                    }
                    if cache.read().contains_key(&ip) {
                        continue;
                    }
                    let name = ip
                        .parse::<IpAddr>()
                        .ok()
                        .and_then(|a| dns_lookup::lookup_addr(&a).ok())
                        .unwrap_or_default();
                    cache.write().insert(ip, name);
                    done += 1;
                }
            }
        })
        .expect("rdns thread");
}

pub struct Enricher {
    geo_reader: Option<maxminddb::Reader<Vec<u8>>>,
}

impl Enricher {
    pub fn new(app_data: &std::path::Path) -> Self {
        // Look for GeoLite2-Country.mmdb next to the DB or in src-tauri/.
        let candidates = [
            app_data.join("GeoLite2-Country.mmdb"),
            PathBuf::from("GeoLite2-Country.mmdb"),
        ];
        let mut geo_reader = None;
        for c in candidates {
            if c.exists() {
                if let Ok(bytes) = std::fs::read(&c) {
                    if let Ok(r) = maxminddb::Reader::from_source(bytes) {
                        geo_reader = Some(r);
                        break;
                    }
                }
            }
        }
        Self { geo_reader }
    }

    pub fn geo_status(&self) -> &'static str {
        if self.geo_reader.is_some() {
            "ready"
        } else {
            "missing-mmdb"
        }
    }

    pub fn country_for(&self, ip: &str) -> String {
        if let Some(r) = &self.geo_reader {
            if let Ok(addr) = ip.parse::<IpAddr>() {
                let res: Result<maxminddb::geoip2::Country, _> = r.lookup(addr);
                if let Ok(country) = res {
                    if let Some(inner) = country.country {
                        if let Some(iso) = inner.iso_code {
                            return iso.to_string();
                        }
                    }
                }
            }
        }
        String::new()
    }
}

/// Parse `arp -a` output on Win/Linux/macOS into (ip, mac) pairs.
pub fn parse_arp_table() -> Vec<(String, String)> {
    let out = std::process::Command::new(if cfg!(windows) { "arp" } else { "arp" })
        .arg("-a")
        .output();
    let Ok(out) = out else { return vec![] };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut rows = vec![];
    for line in text.lines() {
        // Matches: 192.168.1.1  aa-bb-cc-dd-ee-ff  dynamic  OR  (192.168.1.1) at aa:bb:...
        let lower = line.to_lowercase();
        let ip = extract_ip(line);
        let mac = extract_mac(&lower);
        if let (Some(ip), Some(mac)) = (ip, mac) {
            if !ip.starts_with("224.") && !ip.starts_with("239.") && !ip.starts_with("255.") {
                rows.push((ip, mac));
            }
        }
    }
    rows
}

fn extract_ip(s: &str) -> Option<String> {
    // find first dotted quad
    let mut current = String::new();
    let mut dots = 0;
    let mut found: Option<String> = None;
    for ch in s.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if ch == '.' {
            current.push(ch);
            dots += 1;
        } else {
            if dots == 3 && current.split('.').all(|p| p.parse::<u8>().is_ok() || p.len() <= 3) {
                // validate quickly
                let parts: Vec<&str> = current.split('.').collect();
                if parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok()) {
                    found = Some(current.clone());
                    break;
                }
            }
            current.clear();
            dots = 0;
        }
    }
    found
}

fn extract_mac(s: &str) -> Option<String> {
    // accept aa:bb:cc:dd:ee:ff or aa-bb-cc-dd-ee-ff
    for token in s.split_whitespace() {
        let t = token.trim_matches(|c| c == '(' || c == ')' || c == ',');
        let sep = if t.contains(':') { ':' } else if t.contains('-') { '-' } else { continue };
        let parts: Vec<&str> = t.split(sep).collect();
        if parts.len() == 6
            && parts
                .iter()
                .all(|p| p.len() == 2 && u8::from_str_radix(p, 16).is_ok())
        {
            return Some(t.replace('-', ":").to_lowercase());
        }
    }
    None
}

/// Tiny built-in OUI map for common vendors (full IEEE list is 30k+ rows — ship as data file later).
pub fn vendor_guess(mac: &str) -> String {
    let prefix = mac.replace(':', "").to_uppercase().chars().take(6).collect::<String>();
    match prefix.as_str() {
        "3C7C3F" | "F4D108" | "B0B98A" => "TP-Link",
        "D850E6" | "14CC20" => "ASUSTek",
        "3C22FB" | "F0B4D2" => "Apple",
        "ACDE48" | "001B21" => "Intel",
        "508140" | "E091F5" => "Xiaomi",
        "D4DA21" | "A4B1C1" => "Huawei",
        "F0DEF1" | "E8B4AE" => "Samsung",
        "001A11" | "70CD0D" => "Google",
        "B8E62E" | "5CC5D4" => "Dell",
        "3C4A92" | "948824" => "Hewlett Packard",
        _ => "Unknown",
    }
    .to_string()
}
