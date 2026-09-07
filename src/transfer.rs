use std::path::Path;
use std::process::Command;
use std::time::Instant;
use colored::*;
use crate::config::load_session;
use crate::vault::{find_server, load_vault, ServerEntry};
use crate::cert::generate_ephemeral_certificate;

pub fn push_file(target_query: &str, local_path_str: &str, remote_path_opt: Option<&str>) -> Result<(), String> {
    let local_path = Path::new(local_path_str);
    if !local_path.exists() {
        return Err(format!("Lokale Datei '{}' existiert nicht!", local_path_str));
    }

    let file_size_bytes = std::fs::metadata(local_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let size_str = format_bytes(file_size_bytes);

    let vault = load_vault();
    let session = load_session();

    let server = if let Some(found) = find_server(&vault, target_query) {
        found.clone()
    } else {
        let (user, host) = if target_query.contains('@') {
            let mut parts = target_query.split('@');
            (parts.next().unwrap_or("leonf").to_string(), parts.next().unwrap_or("").to_string())
        } else {
            ("leonf".to_string(), target_query.to_string())
        };
        ServerEntry {
            name: target_query.to_string(),
            host,
            port: 22,
            user,
            tags: vec!["ad-hoc".to_string()],
            identity_file: None,
            description: Some("Ad-hoc Transfer".to_string()),
            last_connected: None,
        }
    };

    let filename = local_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload.bin");
    let remote_dest = remote_path_opt
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("./{}", filename));

    let username = session.as_ref().map(|s| s.username.as_str()).unwrap_or("leonf");
    let principals = [server.user.as_str(), "root", "leonf", "admin"];
    let cert_bundle = generate_ephemeral_certificate(username, &principals, 8)?;

    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "DATEI-UPLOAD (PUSH)".bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Quelle:     {} ({})", "►".bright_cyan(), local_path.display().to_string().bold(), size_str.bright_yellow());
    println!("  {} Zielserver: {} ({}@{}:{})", "►".bright_cyan(), server.name.bold(), server.user, server.host, server.port);
    println!("  {} Zielpfad:   {}", "►".bright_cyan(), remote_dest.bright_white().bold());
    println!("  {} Übertrage Daten via kurzem Ephemeral-Zertifikat...", "►".bright_cyan());

    let cert_arg = format!("CertificateFile={}", cert_bundle.cert_path.display());
    let remote_full = format!("{}@{}:{}", server.user, server.host, remote_dest);

    let start = Instant::now();
    let mut cmd = Command::new("scp");
    cmd.arg("-P").arg(server.port.to_string())
       .arg("-o").arg(cert_arg)
       .arg("-i").arg(&cert_bundle.private_key_path)
       .arg("-o").arg("StrictHostKeyChecking=accept-new");

    if local_path.is_dir() {
        cmd.arg("-r");
    }

    if let Some(ref id_file) = server.identity_file {
        cmd.arg("-i").arg(id_file);
    }

    cmd.arg(local_path_str).arg(&remote_full);

    let status = cmd.status().map_err(|e| format!("Konnte scp-Befehl nicht ausführen: {}", e))?;
    let duration = start.elapsed();

    if status.success() {
        let speed_mb_s = if duration.as_secs_f64() > 0.0 {
            (file_size_bytes as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
        } else {
            0.0
        };
        println!("\n  {} Upload erfolgreich abgeschlossen!", "✓".bright_green().bold());
        println!("  {} Dauer: {:.2}s  |  Geschwindigkeit: {:.2} MB/s", "►".bright_cyan(), duration.as_secs_f64(), speed_mb_s);
        println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        Ok(())
    } else {
        Err(format!("Upload fehlgeschlagen (Exit-Code: {:?})", status.code()))
    }
}

pub fn pull_file(target_query: &str, remote_path_str: &str, local_path_opt: Option<&str>) -> Result<(), String> {
    let vault = load_vault();
    let session = load_session();

    let server = if let Some(found) = find_server(&vault, target_query) {
        found.clone()
    } else {
        let (user, host) = if target_query.contains('@') {
            let mut parts = target_query.split('@');
            (parts.next().unwrap_or("leonf").to_string(), parts.next().unwrap_or("").to_string())
        } else {
            ("leonf".to_string(), target_query.to_string())
        };
        ServerEntry {
            name: target_query.to_string(),
            host,
            port: 22,
            user,
            tags: vec!["ad-hoc".to_string()],
            identity_file: None,
            description: Some("Ad-hoc Transfer".to_string()),
            last_connected: None,
        }
    };

    let local_dest = local_path_opt.unwrap_or(".");
    let username = session.as_ref().map(|s| s.username.as_str()).unwrap_or("leonf");
    let principals = [server.user.as_str(), "root", "leonf", "admin"];
    let cert_bundle = generate_ephemeral_certificate(username, &principals, 8)?;

    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "DATEI-DOWNLOAD (PULL)".bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Quellserver: {} ({}@{}:{})", "►".bright_cyan(), server.name.bold(), server.user, server.host, server.port);
    println!("  {} Remote-Pfad: {}", "►".bright_cyan(), remote_path_str.bright_yellow());
    println!("  {} Lokales Ziel: {}", "►".bright_cyan(), local_dest.bright_white().bold());
    println!("  {} Lade Datei herunter via kurzem Ephemeral-Zertifikat...", "►".bright_cyan());

    let cert_arg = format!("CertificateFile={}", cert_bundle.cert_path.display());
    let remote_full = format!("{}@{}:{}", server.user, server.host, remote_path_str);

    let start = Instant::now();
    let mut cmd = Command::new("scp");
    cmd.arg("-P").arg(server.port.to_string())
       .arg("-o").arg(cert_arg)
       .arg("-i").arg(&cert_bundle.private_key_path)
       .arg("-o").arg("StrictHostKeyChecking=accept-new");

    if let Some(ref id_file) = server.identity_file {
        cmd.arg("-i").arg(id_file);
    }

    cmd.arg(&remote_full).arg(local_dest);

    let status = cmd.status().map_err(|e| format!("Konnte scp-Befehl nicht ausführen: {}", e))?;
    let duration = start.elapsed();

    if status.success() {
        println!("\n  {} Download erfolgreich abgeschlossen!", "✓".bright_green().bold());
        println!("  {} Dauer: {:.2}s", "►".bright_cyan(), duration.as_secs_f64());
        println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        Ok(())
    } else {
        Err(format!("Download fehlgeschlagen (Exit-Code: {:?})", status.code()))
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} Bytes", bytes)
    }
}
