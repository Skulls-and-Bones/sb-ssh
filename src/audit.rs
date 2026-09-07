use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use colored::*;
use crate::config::get_sb_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<u64>,
    pub user: String,
    pub target_name: String,
    pub target_host: String,
    pub target_port: u16,
    pub target_user: String,
    pub auth_method: String,
    pub cert_serial: Option<u64>,
    pub jump_host: Option<String>,
    pub recording_file: Option<String>,
    pub status: String,
}

pub fn get_audit_dir() -> PathBuf {
    get_sb_dir().join("audit")
}

pub fn get_audit_log_path() -> PathBuf {
    get_audit_dir().join("sessions.jsonl")
}

pub fn new_session_id() -> String {
    let now = Utc::now();
    let rand_num: u16 = rand::random();
    format!("sb-{}-{:04x}", now.format("%Y%m%d-%H%M%S"), rand_num)
}

pub fn log_session_start(
    session_id: &str,
    user: &str,
    target_name: &str,
    target_host: &str,
    target_port: u16,
    target_user: &str,
    auth_method: &str,
    cert_serial: Option<u64>,
    jump_host: Option<&str>,
    recording_file: Option<&str>,
) -> AuditRecord {
    AuditRecord {
        session_id: session_id.to_string(),
        started_at: Utc::now(),
        ended_at: None,
        duration_secs: None,
        user: user.to_string(),
        target_name: target_name.to_string(),
        target_host: target_host.to_string(),
        target_port,
        target_user: target_user.to_string(),
        auth_method: auth_method.to_string(),
        cert_serial,
        jump_host: jump_host.map(|s| s.to_string()),
        recording_file: recording_file.map(|s| s.to_string()),
        status: "Active".to_string(),
    }
}

pub fn log_session_end(mut record: AuditRecord, status: &str) {
    let end_time = Utc::now();
    let duration = (end_time - record.started_at).num_seconds().max(0) as u64;
    record.ended_at = Some(end_time);
    record.duration_secs = Some(duration);
    record.status = status.to_string();

    let audit_dir = get_audit_dir();
    let _ = create_dir_all(&audit_dir);
    let path = get_audit_log_path();

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(json_str) = serde_json::to_string(&record) {
            let _ = writeln!(file, "{}", json_str);
        }
    }
}

pub fn list_audit_records(limit: usize) -> Vec<AuditRecord> {
    let path = get_audit_log_path();
    if !path.exists() {
        return Vec::new();
    }

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for line in reader.lines().flatten() {
        if let Ok(rec) = serde_json::from_str::<AuditRecord>(&line) {
            records.push(rec);
        }
    }

    records.reverse();
    if records.len() > limit {
        records.truncate(limit);
    }
    records
}

pub fn print_audit_table(limit: usize, as_json: bool) {
    let records = list_audit_records(limit);
    if as_json {
        for rec in &records {
            if let Ok(s) = serde_json::to_string(rec) {
                println!("{}", s);
            }
        }
        return;
    }

    println!("\n{}", "── S&B NETGATE // REVISIONSSICHERES SESSION-AUDIT (BSI IT-GRUNDSCHUTZ) ──".bright_cyan().bold());
    if records.is_empty() {
        println!("  Keine Sitzungsprotokolle vorhanden in: {}\n", get_audit_log_path().display());
        return;
    }

    println!(
        "  {:<16} {:<22} {:<12} {:<24} {:<10} {:<10} {}",
        "STARTZEIT".bold(),
        "SESSION ID".bold(),
        "BENUTZER".bold(),
        "ZIEL (SERVER:PORT)".bold(),
        "DAUER".bold(),
        "STATUS".bold(),
        "AUFZEICHNUNG".bold()
    );
    println!("{}", "  ──────────────────────────────────────────────────────────────────────────────────────────────────────".bright_black());

    for rec in &records {
        let time_str = rec.started_at.format("%d.%m %H:%M:%S").to_string();
        let target_str = format!("{}@{}", rec.target_user, rec.target_name);
        let duration_str = rec.duration_secs.map(|d| {
            let m = d / 60;
            let s = d % 60;
            if m > 0 { format!("{}m {}s", m, s) } else { format!("{}s", s) }
        }).unwrap_or_else(|| "aktiv".to_string());

        let status_badge = match rec.status.as_str() {
            "Success" | "Disconnected" => "✓ OK".bright_green(),
            "Active" => "● AKTIV".bright_yellow(),
            _ => "✗ FEHLER".bright_red(),
        };

        let rec_badge = if let Some(ref rf) = rec.recording_file {
            rf.bright_magenta()
        } else {
            "—".bright_black()
        };

        println!(
            "  {:<16} {:<22} {:<12} {:<24} {:<10} {:<10} {}",
            time_str.bright_white(),
            rec.session_id.bright_black(),
            rec.user.bright_yellow(),
            target_str.bright_cyan(),
            duration_str.bright_white(),
            status_badge,
            rec_badge
        );
    }
    println!("\n  {} Speicherort: {}\n", "i".bright_blue(), get_audit_log_path().display().to_string().bright_black());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_record_creation_and_json() {
        let sid = new_session_id();
        assert!(sid.starts_with("sb-"));

        let record = log_session_start(
            &sid,
            "testuser",
            "srv-prod",
            "1.2.3.4",
            22,
            "root",
            "OpenSSH-Cert",
            Some(12345),
            Some("bastion"),
            Some("rec.cast"),
        );

        assert_eq!(record.session_id, sid);
        assert_eq!(record.user, "testuser");
        assert_eq!(record.jump_host.as_deref(), Some("bastion"));

        let json = serde_json::to_string(&record).expect("serialization failed");
        let deserialized: AuditRecord = serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(deserialized.target_host, "1.2.3.4");
        assert_eq!(deserialized.target_port, 22);
    }
}

