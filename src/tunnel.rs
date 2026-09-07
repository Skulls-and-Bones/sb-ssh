use std::process::{Command, Stdio};
use colored::*;
use crate::config::load_session;
use crate::vault::{find_server, load_vault, ServerEntry};
use crate::cert::generate_ephemeral_certificate;

pub struct TunnelConfig {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

pub fn parse_forward_arg(arg: &str) -> Result<TunnelConfig, String> {
    let parts: Vec<&str> = arg.split(':').collect();
    match parts.len() {
        2 => {
            let local_port = parts[0].trim().parse::<u16>()
                .map_err(|_| format!("Ungültiger lokaler Port: '{}'", parts[0]))?;
            let remote_port = parts[1].trim().parse::<u16>()
                .map_err(|_| format!("Ungültiger entfernter Port: '{}'", parts[1]))?;
            Ok(TunnelConfig {
                local_port,
                remote_host: "127.0.0.1".to_string(),
                remote_port,
            })
        }
        3 => {
            let local_port = parts[0].trim().parse::<u16>()
                .map_err(|_| format!("Ungültiger lokaler Port: '{}'", parts[0]))?;
            let remote_host = parts[1].trim().to_string();
            let remote_port = parts[2].trim().parse::<u16>()
                .map_err(|_| format!("Ungültiger entfernter Port: '{}'", parts[2]))?;
            Ok(TunnelConfig {
                local_port,
                remote_host,
                remote_port,
            })
        }
        _ => Err("Ungültiges Format. Erwartet: '<local_port>:<remote_port>' (z.B. 8080:80) oder '<local_port>:<remote_host>:<remote_port>'".to_string()),
    }
}

pub fn run_tunnel(target_query: &str, forward_arg: &str) -> Result<(), String> {
    let tunnel_cfg = parse_forward_arg(forward_arg)?;
    let vault = load_vault();
    let session = load_session();

    let server = if let Some(found) = find_server(&vault, target_query) {
        found.clone()
    } else {
        // Ad-hoc
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
            description: Some("Ad-hoc Tunnel".to_string()),
            last_connected: None,
        }
    };

    let username = session.as_ref().map(|s| s.username.as_str()).unwrap_or("leonf");
    let principals = [server.user.as_str(), "root", "leonf", "admin"];
    let cert_bundle = generate_ephemeral_certificate(username, &principals, 8)?;

    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "SSH PORT-FORWARDING TUNNEL".bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Zielserver:        {} ({}:{})", "►".bright_cyan(), server.name.bold(), server.host, server.port);
    println!("  {} Lokaler Endpunkt:  {}", "►".bright_cyan(), format!("http://127.0.0.1:{}", tunnel_cfg.local_port).bright_yellow().bold());
    println!("  {} Remote Endpunkt:   {}", "►".bright_cyan(), format!("{}:{}", tunnel_cfg.remote_host, tunnel_cfg.remote_port).bright_white());
    println!("  {} Authentifizierung: {} (8h Ephemeral Cert)", "✓".bright_green(), username.bright_green());
    println!("  {} Tunnel-Status:     {}", "●".bright_green(), "AKTIV & BEREIT".bright_green().bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Drücke {} um den Tunnel sauber zu schließen.\n", "i".bright_cyan(), "[STRG + C]".bright_yellow().bold());

    let forward_spec = format!("{}:{}:{}", tunnel_cfg.local_port, tunnel_cfg.remote_host, tunnel_cfg.remote_port);
    let cert_arg = format!("CertificateFile={}", cert_bundle.cert_path.display());

    let mut cmd = Command::new("ssh");
    cmd.arg("-N") // Kein Remote-Kommando ausführen
       .arg("-L").arg(&forward_spec)
       .arg("-p").arg(server.port.to_string())
       .arg("-o").arg(cert_arg)
       .arg("-i").arg(&cert_bundle.private_key_path)
       .arg("-o").arg("ServerAliveInterval=30")
       .arg("-o").arg("ServerAliveCountMax=3")
       .arg("-o").arg("ExitOnForwardFailure=yes");

    if let Some(ref id_file) = server.identity_file {
        cmd.arg("-i").arg(id_file);
    }

    let target_dest = format!("{}@{}", server.user, server.host);
    cmd.arg(&target_dest);

    cmd.stdin(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| format!("Konnte SSH-Tunnel Prozess nicht starten: {}", e))?;
    let status = child.wait().map_err(|e| format!("Fehler beim Warten auf SSH-Tunnel: {}", e))?;

    if status.success() {
        println!("\n  {} Tunnel ordnungsgemäß geschlossen.", "✓".bright_green());
    } else {
        println!("\n  {} Tunnel beendet (Exit-Code: {:?})", "!".bright_yellow(), status.code());
    }

    Ok(())
}
