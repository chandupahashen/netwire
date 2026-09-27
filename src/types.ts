export interface HostStat {
  ip: string;
  hostname: string;
  country: string;
  connections: number;
  up_bytes: number;
  down_bytes: number;
}

export interface AppStat {
  pid: number;
  name: string;
  exe: string;
  connections: number;
  up_bytes: number;
  down_bytes: number;
  up_rate: number;
  down_rate: number;
  hosts: HostStat[];
}

export interface TrafficTick {
  ts: number;
  up_rate: number;
  down_rate: number;
  total_up: number;
  total_down: number;
  apps: AppStat[];
  per_app_estimated: boolean;
  needs_admin: boolean;
}

export interface HistoryPoint {
  ts: number;
  app: string;
  up_bytes: number;
  down_bytes: number;
}

export interface AlertItem {
  id: number;
  ts: number;
  kind: string;
  app: string;
  message: string;
  severity: string;
  read: boolean;
}

export interface DeviceItem {
  mac: string;
  ip: string;
  vendor: string;
  last_seen: number;
}

export interface QuotaStatus {
  bytes_today: number;
  limit_bytes: number;
  exceeded: boolean;
}

export interface ConnectionStat {
  proto: string;
  local: string;
  remote: string;
  pid: number;
  app: string;
  exe: string;
  state: string;
  up_bytes: number;
  down_bytes: number;
  up_rate: number;
  down_rate: number;
  measured: boolean;
}

export interface AppQuota {
  exe: string;
  name: string;
  bytes_per_day: number;
  bytes_today: number;
}

export interface VtResult {
  exe: string;
  sha256: string;
  malicious: number;
  suspicious: number;
  harmless: number;
  undetected: number;
  permalink: string;
  error: string;
}

export interface RemoteConfig {
  enabled: boolean;
  port: number;
  token: string;
}

export interface RemoteStatus {
  running: boolean;
  port: number;
  peers: number;
  token_set: boolean;
}

export interface ToastPayload {
  id: number;
  kind: string;
  app: string;
  message: string;
  severity: string;
  ts: number;
  duration_ms: number;
  view_tab: string;
  view_exe: string;
}

export interface OpenTab {
  tab: string;
  exe: string;
}

export function fmtRate(bps: number): string {
  if (bps >= 1e9) return `${(bps / 1e9).toFixed(2)} GB/s`;
  if (bps >= 1e6) return `${(bps / 1e6).toFixed(2)} MB/s`;
  if (bps >= 1e3) return `${(bps / 1e3).toFixed(1)} KB/s`;
  return `${bps.toFixed(0)} B/s`;
}

export function fmtBytes(b: number): string {
  if (b >= 1 << 30) return `${(b / (1 << 30)).toFixed(2)} GiB`;
  if (b >= 1 << 20) return `${(b / (1 << 20)).toFixed(1)} MiB`;
  if (b >= 1 << 10) return `${(b / (1 << 10)).toFixed(1)} KiB`;
  return `${b} B`;
}
