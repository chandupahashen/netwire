//! VirusTotal file lookup: SHA-256 the exe, query VT v3 API.
//! Needs a free `vt_api_key` in settings (set from the Settings tab).

use sha2::{Digest, Sha256};

use crate::models::VtResult;

pub async fn lookup(exe: &str, api_key: &str) -> VtResult {
    let blank = || VtResult {
        exe: exe.to_string(),
        sha256: String::new(),
        malicious: 0,
        suspicious: 0,
        harmless: 0,
        undetected: 0,
        permalink: String::new(),
        error: String::new(),
    };

    let mut file = match std::fs::File::open(exe) {
        Ok(f) => f,
        Err(e) => {
            let mut r = blank();
            r.error = format!("cannot open exe: {}", e);
            return r;
        }
    };
    let mut hasher = Sha256::new();
    if std::io::copy(&mut file, &mut hasher).is_err() {
        let mut r = blank();
        r.error = "hash failed".to_string();
        return r;
    }
    let sha = hex::encode(hasher.finalize());

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            let mut r = blank();
            r.sha256 = sha;
            r.error = format!("http client: {}", e);
            return r;
        }
    };
    let resp = client
        .get(format!("https://www.virustotal.com/api/v3/files/{}", sha))
        .header("x-apikey", api_key)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            let mut r = blank();
            r.sha256 = sha;
            r.error = format!("request: {}", e);
            return r;
        }
    };
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        let mut r = blank();
        r.sha256 = sha.clone();
        r.permalink = format!("https://www.virustotal.com/gui/file/{}", sha);
        r.error = "unknown to VirusTotal (never submitted)".to_string();
        return r;
    }
    if !resp.status().is_success() {
        let mut r = blank();
        r.sha256 = sha;
        r.error = format!("vt http {}", resp.status());
        return r;
    }
    let v: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            let mut r = blank();
            r.sha256 = sha;
            r.error = format!("parse: {}", e);
            return r;
        }
    };
    let stats = &v["data"]["attributes"]["last_analysis_stats"];
    let link = v["data"]["links"]["self"].as_str().unwrap_or("");
    let permalink = if link.is_empty() {
        format!("https://www.virustotal.com/gui/file/{}", sha)
    } else {
        link.to_string()
    };
    VtResult {
        exe: exe.to_string(),
        sha256: sha,
        malicious: stats["malicious"].as_i64().unwrap_or(0),
        suspicious: stats["suspicious"].as_i64().unwrap_or(0),
        harmless: stats["harmless"].as_i64().unwrap_or(0),
        undetected: stats["undetected"].as_i64().unwrap_or(0),
        permalink,
        error: String::new(),
    }
}
