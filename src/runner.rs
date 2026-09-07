use std::process::Command;
use std::time::{Duration, Instant};
use std::net::TcpStream;
use colored::*;
use crate::config::UserSession;
use crate::vault::{update_last_connected, ServerEntry};
use crate::cert::generate_ephemeral_certificate;

/// Prüft die TCP-Erreichbarkeit und Latenz des Servers
pub fn check_server_latency(host: &str, port: u16) -> Option<Duration> {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();
    if let Ok(stream) = TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| {
            use std::net::ToSocketAddrs;
            addr.to_socket_addrs().ok()
                .and_then(|mut iter| iter.next())
                .unwrap_or_else(|| "0.0.0.0:22".parse().unwrap())
        }),
        Duration::from_millis(1500),
    ) {
        let elapsed = start.elapsed();
        drop(stream);
        Some(elapsed)
    } else {
        None
    }
}

pub fn run_ssh_session(server: &ServerEntry, session: Option<&UserSession>) -> Result<(), String> {
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), format!("VERBINDE MIT: {}", server.name).bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());

    let username = session.map(|s| s.username.as_str()).unwrap_or("leonf");

    println!("  {} Ziel:       {}@{}:{}", "►".bright_cyan(), server.user.bright_yellow(), server.host.bright_white(), server.port);
    println!("  {} Identität:  {}", "►".bright_cyan(), username.bright_green());

    // 1. Ephemeres Zertifikat generieren
    println!("  {} Generiere kurzlebiges Ed25519-Sitzungszertifikat (8h)...", "►".bright_cyan());
    let principals = [server.user.as_str(), "root", "leonf", "ubuntu", "admin"];
    let cert_bundle = generate_ephemeral_certificate(username, &principals, 8)?;

    println!("  {} Zertifikat ausgestellt: Gültig bis {}", "✓".bright_green(), cert_bundle.valid_until.format("%H:%M:%S UTC").to_string().bright_cyan());

    // 2. SSH-Befehl zusammenstellen
    let mut cmd = Command::new("ssh");
    cmd.arg("-p").arg(server.port.to_string());

    // Zertifikat und ephemerer Key
    let cert_arg = format!("CertificateFile={}", cert_bundle.cert_path.display());
    cmd.arg("-o").arg(cert_arg);
    cmd.arg("-i").arg(&cert_bundle.private_key_path);

    // Wenn ein expliziter Key hinterlegt ist oder als Fallback fuer noch nicht auf CA umgestellte Server
    if let Some(ref id_file) = server.identity_file {
        cmd.arg("-i").arg(id_file);
    } else {
        // Lokaler Standard ed25519 Key als Fallback hinzufuegen
        let home = dirs::home_dir().unwrap_or_default();
        let fallback_key = home.join(".ssh").join("id_ed25519");
        if fallback_key.exists() {
            cmd.arg("-i").arg(fallback_key);
        }
    }

    cmd.arg("-o").arg("StrictHostKeyChecking=accept-new");
    cmd.arg(format!("{}@{}", server.user, server.host));

    println!("  {} Starte native SSH-Sitzung...\n", "►".bright_cyan());

    // Vor dem Start Zeitstempel aktualisieren
    update_last_connected(&server.name);

    let mut child = cmd.spawn().map_err(|e| format!("Fehler beim Starten des SSH-Clients: {}", e))?;
    let status = child.wait().map_err(|e| format!("Fehler während der SSH-Sitzung: {}", e))?;

    println!("\n{}", "══════════════════════════════════════════════════════════════════".bright_black());
    if status.success() {
        println!("  {} SSH-Sitzung sauber beendet.", "✓".bright_green());
    } else {
        println!("  {} SSH-Sitzung mit Statuscode {} beendet.", "i".bright_yellow(), status);
    }
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());

    Ok(())
}
