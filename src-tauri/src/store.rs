//! SQLite store: samples (rollup-friendly), apps, alerts, devices, threats, settings.

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::{AlertItem, AppQuota, DeviceItem, HistoryPoint, TrafficTick};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let s = Self { conn };
        s.init()?;
        Ok(s)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS samples (
                ts INTEGER NOT NULL,
                app TEXT NOT NULL,
                exe TEXT NOT NULL DEFAULT '',
                up_bytes INTEGER NOT NULL DEFAULT 0,
                down_bytes INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_samples_ts ON samples(ts);
            CREATE INDEX IF NOT EXISTS idx_samples_app ON samples(app);

            CREATE TABLE IF NOT EXISTS totals (
                ts INTEGER PRIMARY KEY,
                up_bytes INTEGER NOT NULL DEFAULT 0,
                down_bytes INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS apps (
                exe TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                first_seen INTEGER NOT NULL,
                last_seen INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS alerts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                kind TEXT NOT NULL,
                app TEXT NOT NULL DEFAULT '',
                message TEXT NOT NULL,
                severity TEXT NOT NULL DEFAULT 'info',
                read INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS devices (
                mac TEXT PRIMARY KEY,
                ip TEXT NOT NULL DEFAULT '',
                vendor TEXT NOT NULL DEFAULT '',
                last_seen INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS threats (
                ip TEXT PRIMARY KEY,
                note TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_quotas (
                exe TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                bytes_per_day INTEGER NOT NULL DEFAULT 0
            );
            ",
        )?;
        Ok(())
    }

    /// Insert one tick. Called every TICK_SECS from the monitor loop.
    pub fn insert_tick(&mut self, tick: &TrafficTick) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare(
                "INSERT INTO samples (ts, app, exe, up_bytes, down_bytes) VALUES (?,?,?,?,?)",
            )?;
            for a in &tick.apps {
                st.execute(params![
                    tick.ts,
                    a.name,
                    a.exe,
                    a.up_bytes as i64,
                    a.down_bytes as i64
                ])?;
            }
        }
        {
            let mut st = tx.prepare(
                "INSERT OR REPLACE INTO totals (ts, up_bytes, down_bytes) VALUES (?,?,?)",
            )?;
            st.execute(params![tick.ts, tick.total_up as i64, tick.total_down as i64])?;
        }
        {
            let mut st = tx.prepare(
                "INSERT INTO apps (exe, name, first_seen, last_seen) VALUES (?,?,?,?)
                 ON CONFLICT(exe) DO UPDATE SET last_seen=excluded.last_seen, name=excluded.name",
            )?;
            for a in &tick.apps {
                if !a.exe.is_empty() {
                    st.execute(params![a.exe, a.name, tick.ts, tick.ts])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn is_new_app(&mut self, exe: &str, now: i64) -> Result<bool> {
        let cnt: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM apps WHERE exe = ?",
            params![exe],
            |r| r.get(0),
        )?;
        if cnt == 0 && !exe.is_empty() {
            self.conn.execute(
                "INSERT OR IGNORE INTO apps (exe, name, first_seen, last_seen) VALUES (?,?,?,?)",
                params![exe, exe, now, now],
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn push_alert(
        &self,
        ts: i64,
        kind: &str,
        app: &str,
        message: &str,
        severity: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO alerts (ts, kind, app, message, severity, read) VALUES (?,?,?,?,?,0)",
            params![ts, kind, app, message, severity],
        )?;
        Ok(())
    }

    pub fn recent_alerts(&self, limit: i64) -> Result<Vec<AlertItem>> {
        let mut st = self.conn.prepare(
            "SELECT id, ts, kind, app, message, severity, read FROM alerts ORDER BY id DESC LIMIT ?",
        )?;
        let rows = st.query_map(params![limit], |r| {
            Ok(AlertItem {
                id: r.get(0)?,
                ts: r.get(1)?,
                kind: r.get(2)?,
                app: r.get(3)?,
                message: r.get(4)?,
                severity: r.get(5)?,
                read: r.get::<_, i64>(6)? != 0,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn mark_alerts_read(&self) -> Result<()> {
        self.conn.execute("UPDATE alerts SET read = 1", [])?;
        Ok(())
    }

    /// History with server-side downsampling.
    pub fn history(&self, from: i64, to: i64, bucket: i64) -> Result<Vec<HistoryPoint>> {
        let b = bucket.max(1);
        let mut st = self.conn.prepare(
            "SELECT (ts / ?) * ?, app, SUM(up_bytes), SUM(down_bytes)
             FROM samples WHERE ts >= ? AND ts <= ? GROUP BY 1, 2 ORDER BY 1 ASC LIMIT 20000",
        )?;
        let rows = st.query_map(params![b, b, from, to], |r| {
            Ok(HistoryPoint {
                ts: r.get(0)?,
                app: r.get(1)?,
                up_bytes: r.get::<_, i64>(2)? as u64,
                down_bytes: r.get::<_, i64>(3)? as u64,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn bytes_today(&self, day_start: i64) -> Result<u64> {
        let v: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(up_bytes + down_bytes), 0) FROM samples WHERE ts >= ?",
            params![day_start],
            |r| r.get(0),
        )?;
        Ok(v as u64)
    }

    pub fn upsert_device(&self, mac: &str, ip: &str, vendor: &str, now: i64) -> Result<bool> {
        let cnt: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM devices WHERE mac = ?",
            params![mac],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO devices (mac, ip, vendor, last_seen) VALUES (?,?,?,?)
             ON CONFLICT(mac) DO UPDATE SET ip=excluded.ip, vendor=excluded.vendor, last_seen=excluded.last_seen",
            params![mac, ip, vendor, now],
        )?;
        Ok(cnt == 0)
    }

    pub fn devices(&self, limit: i64) -> Result<Vec<DeviceItem>> {
        let mut st = self.conn.prepare(
            "SELECT mac, ip, vendor, last_seen FROM devices ORDER BY last_seen DESC LIMIT ?",
        )?;
        let rows = st.query_map(params![limit], |r| {
            Ok(DeviceItem {
                mac: r.get(0)?,
                ip: r.get(1)?,
                vendor: r.get(2)?,
                last_seen: r.get(3)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn is_threat(&self, ip: &str) -> Result<bool> {
        let cnt: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM threats WHERE ip = ?",
            params![ip],
            |r| r.get(0),
        )?;
        Ok(cnt > 0)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut st = self.conn.prepare("SELECT value FROM settings WHERE key = ?")?;
        let mut rows = st.query_map(params![key], |r| r.get::<_, String>(0))?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?,?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
    pub fn prune(&self, keep_secs: i64, now: i64) -> Result<()> {        self.conn.execute(
            "DELETE FROM samples WHERE ts < ?",
            params![now - keep_secs],
        )?;
        self.conn.execute(
            "DELETE FROM totals WHERE ts < ?",
            params![now - keep_secs],
        )?;
        Ok(())
    }

    // ---------- Per-app quotas ----------

    pub fn set_app_quota(&self, exe: &str, name: &str, bytes_per_day: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO app_quotas (exe, name, bytes_per_day) VALUES (?,?,?)
             ON CONFLICT(exe) DO UPDATE SET name=excluded.name, bytes_per_day=excluded.bytes_per_day",
            params![exe, name, bytes_per_day as i64],
        )?;
        Ok(())
    }

    pub fn delete_app_quota(&self, exe: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM app_quotas WHERE exe = ?", params![exe])?;
        Ok(())
    }

    pub fn bytes_for_app_today(&self, exe: &str, day_start: i64) -> Result<u64> {
        let v: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(up_bytes + down_bytes), 0) FROM samples WHERE exe = ? AND ts >= ?",
            params![exe, day_start],
            |r| r.get(0),
        )?;
        Ok(v as u64)
    }

    pub fn list_app_quotas(&self, day_start: i64) -> Result<Vec<AppQuota>> {
        let mut st = self
            .conn
            .prepare("SELECT exe, name, bytes_per_day FROM app_quotas ORDER BY name ASC")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u64,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows.filter_map(|r| r.ok()) {
            let today = self.bytes_for_app_today(&r.0, day_start).unwrap_or(0);
            out.push(AppQuota {
                exe: r.0,
                name: r.1,
                bytes_per_day: r.2,
                bytes_today: today,
            });
        }
        Ok(out)
    }

    /// Raw rows for CSV export.
    pub fn export_rows(&self, from: i64, to: i64) -> Result<Vec<(i64, String, u64, u64)>> {
        let mut st = self.conn.prepare(
            "SELECT ts, app, SUM(up_bytes), SUM(down_bytes) FROM samples
             WHERE ts >= ? AND ts <= ? GROUP BY ts, app ORDER BY ts ASC LIMIT 50000",
        )?;
        let rows = st.query_map(params![from, to], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, i64>(3)? as u64,
            ))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrafficTick;

    fn mem_store() -> Store {
        let conn = Connection::open_in_memory().unwrap();
        let s = Store { conn };
        s.init().unwrap();
        s
    }

    #[test]
    fn tick_roundtrip_and_history() {
        let mut s = mem_store();
        let tick = TrafficTick {
            ts: 1_700_000_000,
            up_rate: 10.0,
            down_rate: 20.0,
            total_up: 100,
            total_down: 200,
            apps: vec![crate::models::AppStat {
                pid: 1234,
                name: "test.exe".into(),
                exe: "C:\\test.exe".into(),
                connections: 2,
                up_bytes: 100,
                down_bytes: 200,
                up_rate: 10.0,
                down_rate: 20.0,
                hosts: vec![],
            }],
            per_app_estimated: true,
            needs_admin: false,
        };
        s.insert_tick(&tick).unwrap();
        let hist = s.history(1_700_000_000 - 60, 1_700_000_000 + 60, 5).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].app, "test.exe");
        assert_eq!(hist[0].down_bytes, 200);
        assert!(s.bytes_today(1_700_000_000 - 86400).unwrap() >= 300);
    }

    #[test]
    fn alerts_and_devices() {
        let s = mem_store();
        s.push_alert(1, "new-app", "a.exe", "hello", "info").unwrap();
        let alerts = s.recent_alerts(10).unwrap();
        assert_eq!(alerts.len(), 1);
        assert!(!alerts[0].read);
        s.mark_alerts_read().unwrap();
        assert!(s.recent_alerts(10).unwrap()[0].read);

        assert!(s.upsert_device("aa:bb:cc:dd:ee:ff", "192.168.1.5", "Test", 1).unwrap());
        assert!(!s.upsert_device("aa:bb:cc:dd:ee:ff", "192.168.1.5", "Test", 2).unwrap());
        assert_eq!(s.devices(10).unwrap().len(), 1);
    }
}
