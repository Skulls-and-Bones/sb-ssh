use std::path::{Path, PathBuf};
use std::time::Instant;
use colored::*;
use tokio::io::AsyncWriteExt;
use russh_sftp::client::SftpSession;

use crate::config::load_session;
use crate::vault::{find_server, load_vault, ServerEntry};
use crate::native_ssh::connect_and_auth;

pub async fn push_file(target_query: &str, local_path_str: &str, remote_path_opt: Option<&str>) -> Result<(), String> {
    let local_path = Path::new(local_path_str);
    if !local_path.exists() {
        return Err(format!("Lokale Datei '{}' existiert nicht!", local_path_str));
    }
    if local_path.is_dir() {
        return Err("Verzeichnis-Upload wird über SFTP derzeit nicht unterstützt. Bitte als Archiv (.tar.gz / .zip) übertragen.".to_string());
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
        .map(|s| {
            if s.ends_with('/') {
                format!("{}{}", s, filename)
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| format!("./{}", filename));

    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "NATIVER SFTP-UPLOAD (PUSH)".bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Quelle:     {} ({})", "►".bright_cyan(), local_path.display().to_string().bold(), size_str.bright_yellow());
    println!("  {} Zielserver: {} ({}@{}:{})", "►".bright_cyan(), server.name.bold(), server.user, server.host, server.port);
    println!("  {} Zielpfad:   {}", "►".bright_cyan(), remote_dest.bright_white().bold());
    println!("  {} Transport:  {}", "►".bright_cyan(), "Autarker Pure-Rust SFTP Client (russh-sftp)".bright_magenta());

    let start = Instant::now();

    // 1. Verbinden & Authentifizieren
    let handle = connect_and_auth(&server, session.as_ref()).await?;

    // 2. SSH-Kanal öffnen & SFTP-Subsystem anfordern
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("Fehler beim Öffnen des SSH-Kanals: {}", e))?;

    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("SFTP-Subsystem fehlgeschlagen: {}", e))?;

    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| format!("SFTP-Sitzungsinitialisierung fehlgeschlagen: {}", e))?;

    // 3. Lokale Datei öffnen & entfernte Datei erzeugen
    let mut local_file = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| format!("Konnte lokale Datei nicht lesen: {}", e))?;

    let mut remote_file = sftp
        .create(&remote_dest)
        .await
        .map_err(|e| format!("Konnte Zieldatei '{}' auf dem Server nicht erstellen: {}", remote_dest, e))?;

    // 4. Daten streamen
    let copied_bytes = tokio::io::copy(&mut local_file, &mut remote_file)
        .await
        .map_err(|e| format!("Übertragungsfehler: {}", e))?;

    remote_file
        .flush()
        .await
        .map_err(|e| format!("Fehler beim Abschließen der Remote-Datei: {}", e))?;

    let duration = start.elapsed();
    let speed_mb_s = if duration.as_secs_f64() > 0.0 {
        (copied_bytes as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
    } else {
        0.0
    };

    println!("\n  {} Nativer Upload erfolgreich abgeschlossen!", "✓".bright_green().bold());
    println!(
        "  {} Übertragen: {}  |  Dauer: {:.2}s  |  Rate: {:.2} MB/s",
        "►".bright_cyan(),
        format_bytes(copied_bytes).bright_yellow(),
        duration.as_secs_f64(),
        speed_mb_s
    );
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    Ok(())
}

pub async fn pull_file(target_query: &str, remote_path_str: &str, local_path_opt: Option<&str>) -> Result<(), String> {
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

    // Lokalen Zielpfad auflösen
    let remote_path = Path::new(remote_path_str);
    let remote_filename = remote_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("downloaded.bin");

    let dest_path: PathBuf = match local_path_opt {
        Some(path_str) => {
            let p = Path::new(path_str);
            if p.is_dir() {
                p.join(remote_filename)
            } else {
                p.to_path_buf()
            }
        }
        None => PathBuf::from(remote_filename),
    };

    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "NATIVER SFTP-DOWNLOAD (PULL)".bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Quellserver: {} ({}@{}:{})", "►".bright_cyan(), server.name.bold(), server.user, server.host, server.port);
    println!("  {} Remote-Pfad: {}", "►".bright_cyan(), remote_path_str.bright_yellow());
    println!("  {} Lokales Ziel: {}", "►".bright_cyan(), dest_path.display().to_string().bright_white().bold());
    println!("  {} Transport:   {}", "►".bright_cyan(), "Autarker Pure-Rust SFTP Client (russh-sftp)".bright_magenta());

    let start = Instant::now();

    // 1. Verbinden & Authentifizieren
    let handle = connect_and_auth(&server, session.as_ref()).await?;

    // 2. SSH-Kanal öffnen & SFTP-Subsystem anfordern
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("Fehler beim Öffnen des SSH-Kanals: {}", e))?;

    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("SFTP-Subsystem fehlgeschlagen: {}", e))?;

    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| format!("SFTP-Sitzungsinitialisierung fehlgeschlagen: {}", e))?;

    // 3. Remote-Datei öffnen
    let mut remote_file = sftp
        .open(remote_path_str)
        .await
        .map_err(|e| format!("Konnte entfernte Datei '{}' nicht öffnen: {}", remote_path_str, e))?;

    // 4. Lokale Zieldatei erzeugen & streamen
    let mut local_file = tokio::fs::File::create(&dest_path)
        .await
        .map_err(|e| format!("Konnte lokale Datei '{}' nicht erstellen: {}", dest_path.display(), e))?;

    let copied_bytes = tokio::io::copy(&mut remote_file, &mut local_file)
        .await
        .map_err(|e| format!("Download-Übertragungsfehler: {}", e))?;

    local_file
        .flush()
        .await
        .map_err(|e| format!("Fehler beim Schreiben der lokalen Datei: {}", e))?;

    let duration = start.elapsed();
    let speed_mb_s = if duration.as_secs_f64() > 0.0 {
        (copied_bytes as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
    } else {
        0.0
    };

    println!("\n  {} Nativer Download erfolgreich abgeschlossen!", "✓".bright_green().bold());
    println!(
        "  {} Empfangen:   {}  |  Dauer: {:.2}s  |  Rate: {:.2} MB/s",
        "►".bright_cyan(),
        format_bytes(copied_bytes).bright_yellow(),
        duration.as_secs_f64(),
        speed_mb_s
    );
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    Ok(())
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

