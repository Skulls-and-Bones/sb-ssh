use std::sync::Arc;
use std::time::Duration;
use colored::*;
use russh::client::{Config, Handle, Handler};
use russh::keys::PrivateKeyWithHashAlg;
use russh::keys::PublicKeyOrCertificate;
use russh::keys::ssh_key::{Certificate, PrivateKey};
use tokio::io::AsyncWriteExt;

use crate::cert::{generate_ephemeral_certificate, EphemeralCertBundle};
use crate::config::UserSession;
use crate::vault::ServerEntry;

/// Russh Client-Handler zur Prüfung des Server-Host-Keys.
pub struct ClientHandler;

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // TOFU: Akzeptiert den Server-Key (kann bei Bedarf gegen ~/.ssh/known_hosts abgeglichen werden)
        Ok(true)
    }
}

/// Stellt eine authentifizierte native SSH2-Sitzung zum Zielserver her (direkt oder kaskadiert via Bastion).
pub async fn connect_and_auth(
    server: &ServerEntry,
    session: Option<&UserSession>,
) -> Result<Handle<ClientHandler>, String> {
    let username = session.map(|s| s.username.as_str()).unwrap_or("leonf");

    // 1. Ephemeres Ed25519-Zertifikat erzeugen
    let principals = [server.user.as_str(), "root", "leonf", "ubuntu", "admin"];
    let bundle: EphemeralCertBundle = generate_ephemeral_certificate(username, &principals, 8)
        .map_err(|e| format!("Zertifikatsfehler: {}", e))?;

    // 2. Client-Konfiguration
    let config = Arc::new(Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        keepalive_interval: Some(Duration::from_secs(15)),
        ..Default::default()
    });

    // 3. TCP-Verbindung herstellen (Direkt oder kaskadiert über Jump-Host)
    let mut handle = if let Some(ref jump_name) = server.jump_host {
        let vault = crate::vault::load_vault();
        let jump_server = if let Some(found) = crate::vault::find_server(&vault, jump_name) {
            found.clone()
        } else {
            ServerEntry {
                name: jump_name.clone(),
                host: jump_name.clone(),
                port: 22,
                user: server.user.clone(),
                tags: vec!["bastion".to_string()],
                identity_file: server.identity_file.clone(),
                description: Some("Ad-hoc Bastion Host".to_string()),
                last_connected: None,
                jump_host: None,
            }
        };

        println!("  {} Kaskadiere über Bastion-Host '{}' ({}:{})...", "►".bright_cyan(), jump_server.name.bold(), jump_server.host, jump_server.port);
        let jump_handle = Box::pin(connect_and_auth(&jump_server, session)).await
            .map_err(|e| format!("Bastion-Verbindung fehlgeschlagen: {}", e))?;

        let channel = jump_handle
            .channel_open_direct_tcpip(&server.host, server.port as u32, "127.0.0.1", 0)
            .await
            .map_err(|e| format!("Konnte Tunnel-Kanal auf Bastion nicht öffnen: {}", e))?;

        let stream = channel.into_stream();
        russh::client::connect_stream(config, stream, ClientHandler)
            .await
            .map_err(|e| format!("SSH2-Handshake über Bastion fehlgeschlagen: {}", e))?
    } else {
        let addr = format!("{}:{}", server.host, server.port);
        russh::client::connect(config, addr.as_str(), ClientHandler)
            .await
            .map_err(|e| format!("Verbindung zu {} fehlgeschlagen: {}", addr, e))?
    };

    // 4. Zertifikat und ephemeren Key einlesen
    let priv_str = std::fs::read_to_string(&bundle.private_key_path)
        .map_err(|e| format!("Konnte ephemeren Key nicht lesen: {}", e))?;
    let priv_key = PrivateKey::from_openssh(&priv_str)
        .map_err(|e| format!("Konnte ephemeren Key nicht parsen: {}", e))?;

    let cert_str = std::fs::read_to_string(&bundle.cert_path)
        .map_err(|e| format!("Konnte Zertifikat nicht lesen: {}", e))?;
    let cert = Certificate::from_openssh(&cert_str)
        .map_err(|e| format!("Konnte Zertifikat nicht parsen: {}", e))?;

    // 5. OpenSSH Zertifikats-Authentifizierung
    let auth_res = handle
        .authenticate_openssh_cert(&server.user, Arc::new(priv_key), cert)
        .await
        .map_err(|e| format!("Authentifizierungsfehler: {}", e))?;

    if auth_res.success() {
        return Ok(handle);
    }

    // Fallback: Wenn expliziter Identity-Key hinterlegt ist oder ~/.ssh/id_ed25519 existiert
    if let Some(ref id_path) = server.identity_file {
        if let Ok(key_content) = std::fs::read_to_string(id_path) {
            if let Ok(fallback_key) = PrivateKey::from_openssh(&key_content) {
                let fb_res = handle
                    .authenticate_publickey(&server.user, PrivateKeyWithHashAlg::new(Arc::new(fallback_key), None))
                    .await
                    .map_err(|e| format!("Fallback-Auth fehlgeschlagen: {}", e))?;
                if fb_res.success() {
                    return Ok(handle);
                }
            }
        }
    }

    let home = dirs::home_dir().unwrap_or_default();
    let default_key_path = home.join(".ssh").join("id_ed25519");
    if default_key_path.exists() {
        if let Ok(key_content) = std::fs::read_to_string(&default_key_path) {
            if let Ok(fallback_key) = PrivateKey::from_openssh(&key_content) {
                let fb_res = handle
                    .authenticate_publickey(&server.user, PrivateKeyWithHashAlg::new(Arc::new(fallback_key), None))
                    .await
                    .map_err(|e| format!("Standard-Key Auth fehlgeschlagen: {}", e))?;
                if fb_res.success() {
                    return Ok(handle);
                }
            }
        }
    }

    Err(format!(
        "Authentifizierung für Benutzer '{}' an {} abgewiesen.",
        server.user, server.host
    ))
}

/// Guard für das automatische Zurücksetzen des Raw-Modus bei Verlassen der PTY-Session.
struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self, std::io::Error> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Führt eine vollwertige interaktive SSH2 PTY-Sitzung nativ in Rust aus.
pub async fn run_interactive_shell(
    server: &ServerEntry,
    session: Option<&UserSession>,
    record: bool,
) -> Result<(), String> {
    let session_id = crate::audit::new_session_id();
    let rec_file_opt = if record { Some(format!("{}.cast", session_id)) } else { None };

    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!(
        "  {} {} {}",
        "S&B NETGATE".bright_blue().bold(),
        "//".bright_black(),
        format!("NATIVE VERBINDUNG: {}", server.name).bold()
    );
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());

    let username = session.map(|s| s.username.as_str()).unwrap_or("leonf");
    println!("  {} Ziel:       {}@{}:{}", "►".bright_cyan(), server.user.bright_yellow(), server.host.bright_white(), server.port);
    if let Some(ref jh) = server.jump_host {
        println!("  {} Bastion:    {}", "►".bright_cyan(), jh.bright_magenta());
    }
    println!("  {} Identität:  {}", "►".bright_cyan(), username.bright_green());
    println!("  {} Transport:  {}", "►".bright_cyan(), "Autarker Pure-Rust SSH2 Client (russh)".bright_magenta());

    let audit_entry = crate::audit::log_session_start(
        &session_id,
        username,
        &server.name,
        &server.host,
        server.port,
        &server.user,
        "Ephemeral Ed25519 Cert",
        None,
        server.jump_host.as_deref(),
        rec_file_opt.as_deref(),
    );

    let handle = match connect_and_auth(server, session).await {
        Ok(h) => h,
        Err(e) => {
            crate::audit::log_session_end(audit_entry, "AuthFailed");
            return Err(e);
        }
    };

    println!("  {} Authentifizierung erfolgreich (OpenSSH Ed25519-Cert)", "✓".bright_green());

    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| {
            crate::audit::log_session_end(audit_entry.clone(), "ChannelFailed");
            format!("Fehler beim Öffnen des SSH-Kanals: {}", e)
        })?;

    // Terminalgröße abfragen
    let (mut cols, mut rows) = crossterm::terminal::size().unwrap_or((80, 24));
    channel
        .request_pty(false, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
        .await
        .map_err(|e| format!("PTY-Anforderung fehlgeschlagen: {}", e))?;

    channel
        .request_shell(true)
        .await
        .map_err(|e| format!("Shell-Anforderung fehlgeschlagen: {}", e))?;

    let mut recorder = if record {
        match crate::recorder::SessionRecorder::new(&session_id, &format!("Session to {}", server.name), cols, rows) {
            Ok(rec) => {
                println!("  {} Session-Recording aktiv: ~/.sb-ssh/recordings/{}.cast", "●".bright_magenta(), session_id);
                Some(rec)
            }
            Err(e) => {
                eprintln!("  {} Recording-Warnung: {}", "!".bright_yellow(), e);
                None
            }
        }
    } else {
        None
    };

    println!("  {} Terminal PTY alloziiert. Wechsle in Raw-Modus...\n", "►".bright_cyan());

    // Update Verbindungs-Historie
    crate::vault::update_last_connected(&server.name);

    // Terminal in den Raw-Modus setzen
    let _guard = RawModeGuard::enter()
        .map_err(|e| format!("Konnte Terminal nicht in den Raw-Modus schalten: {}", e))?;

    // Thread für Stdin-Eingabe
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let stdin_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut stdout = tokio::io::stdout();
    let mut resize_interval = tokio::time::interval(Duration::from_millis(500));

    loop {
        tokio::select! {
            Some(input_chunk) = rx.recv() => {
                if channel.data_bytes(input_chunk).await.is_err() {
                    break;
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(russh::ChannelMsg::Data { data }) => {
                        let _ = stdout.write_all(&data).await;
                        let _ = stdout.flush().await;
                        if let Some(ref mut rec) = recorder {
                            rec.record_output(&data);
                        }
                    }
                    Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                        let _ = stdout.write_all(&data).await;
                        let _ = stdout.flush().await;
                        if let Some(ref mut rec) = recorder {
                            rec.record_output(&data);
                        }
                    }
                    Some(russh::ChannelMsg::ExitStatus { .. }) => {
                        // Remote-Prozess beendet
                    }
                    Some(russh::ChannelMsg::Close) | None => {
                        break;
                    }
                    _ => {}
                }
            }
            _ = resize_interval.tick() => {
                if let Ok((new_cols, new_rows)) = crossterm::terminal::size() {
                    if new_cols != cols || new_rows != rows {
                        cols = new_cols;
                        rows = new_rows;
                        let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                    }
                }
            }
        }
    }

    drop(_guard);
    drop(stdin_handle);

    crate::audit::log_session_end(audit_entry, "Disconnected");

    println!("\n{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Native SSH-Sitzung sauber beendet.", "✓".bright_green());
    if recorder.is_some() {
        println!("  {} Aufzeichnung gespeichert in: ~/.sb-ssh/recordings/{}.cast", "✓".bright_magenta(), session_id);
        println!("  {} Abspielen mit: sb-ssh replay {}", "i".bright_blue(), session_id);
    }
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());

    Ok(())
}

/// Führt ein Remote-Kommando nativ über SSH2 aus und gibt (ExitCode, Stdout, Stderr) zurück.
pub async fn run_remote_command(
    server: &ServerEntry,
    command: &str,
    session: Option<&UserSession>,
) -> Result<(i32, String, String), String> {
    let handle = connect_and_auth(server, session).await?;
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("Fehler beim Öffnen des Kanals: {}", e))?;

    channel
        .exec(true, command)
        .await
        .map_err(|e| format!("Fehler beim Ausführen von '{}': {}", command, e))?;

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let mut exit_code = 0;

    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => {
                stdout_buf.extend_from_slice(&data);
            }
            russh::ChannelMsg::ExtendedData { data, .. } => {
                stderr_buf.extend_from_slice(&data);
            }
            russh::ChannelMsg::ExitStatus { exit_status } => {
                exit_code = exit_status as i32;
            }
            russh::ChannelMsg::Close => {
                break;
            }
            _ => {}
        }
    }

    let stdout_str = String::from_utf8_lossy(&stdout_buf).to_string();
    let stderr_str = String::from_utf8_lossy(&stderr_buf).to_string();

    Ok((exit_code, stdout_str, stderr_str))
}

/// Führt ein Remote-Kommando aus und streamt Ein- und Ausgaben direkt (ideal für git, rsync, scp, non-interactive CI).
pub async fn run_streaming_command(
    server: &ServerEntry,
    command: &str,
    session: Option<&UserSession>,
) -> Result<i32, String> {
    use tokio::io::AsyncWriteExt;

    let handle = connect_and_auth(server, session).await?;
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("Fehler beim Öffnen des Kanals: {}", e))?;

    channel
        .exec(true, command)
        .await
        .map_err(|e| format!("Fehler beim Ausführen von '{}': {}", command, e))?;

    let mut exit_code = 0;
    let mut stdout_writer = tokio::io::stdout();
    let mut stderr_writer = tokio::io::stderr();

    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => {
                let _ = stdout_writer.write_all(&data).await;
                let _ = stdout_writer.flush().await;
            }
            russh::ChannelMsg::ExtendedData { data, .. } => {
                let _ = stderr_writer.write_all(&data).await;
                let _ = stderr_writer.flush().await;
            }
            russh::ChannelMsg::ExitStatus { exit_status } => {
                exit_code = exit_status as i32;
            }
            russh::ChannelMsg::Close => {
                break;
            }
            _ => {}
        }
    }

    Ok(exit_code)
}

/// Öffnet einen nativen SSH2 Direct-TCPIP Port-Forwarding Tunnel.
pub async fn run_tunnel(
    server: &ServerEntry,
    local_port: u16,
    remote_host: &str,
    remote_port: u16,
    session: Option<&UserSession>,
) -> Result<(), String> {
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!(
        "  {} {} {}",
        "S&B NETGATE".bright_blue().bold(),
        "//".bright_black(),
        "NATIVES SSH2 PORT-FORWARDING".bold()
    );
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Server:         {} ({}:{})", "►".bright_cyan(), server.name.bold(), server.host, server.port);
    println!("  {} Lokaler Port:   {}", "►".bright_cyan(), format!("127.0.0.1:{}", local_port).bright_yellow().bold());
    println!("  {} Remote-Ziel:    {}", "►".bright_cyan(), format!("{}:{}", remote_host, remote_port).bright_green().bold());
    println!("  {} Transport:      {}", "►".bright_cyan(), "Natives Direct-TCPIP Tunneling (russh)".bright_magenta());

    let handle = Arc::new(connect_and_auth(server, session).await?);
    println!("  {} Tunnel-Verbindung aktiv! Drücke [Ctrl+C] zum Beenden.\n", "✓".bright_green());

    let bind_addr = format!("127.0.0.1:{}", local_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| format!("Konnte Port {} nicht binden: {}", local_port, e))?;

    loop {
        let (mut client_socket, peer_addr) = listener
            .accept()
            .await
            .map_err(|e| format!("Fehler beim Annehmen der TCP-Verbindung: {}", e))?;

        println!("  {} Eingehende Verbindung von {}", "►".bright_cyan(), peer_addr);

        let handle_clone = Arc::clone(&handle);
        let remote_host_owned = remote_host.to_string();

        tokio::spawn(async move {
            match handle_clone
                .channel_open_direct_tcpip(
                    remote_host_owned,
                    remote_port as u32,
                    "127.0.0.1",
                    local_port as u32,
                )
                .await
            {
                Ok(channel) => {
                    let mut channel_stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut client_socket, &mut channel_stream).await;
                }
                Err(e) => {
                    eprintln!("  {} Tunnel-Weiterleitungsfehler: {}", "✗".bright_red(), e);
                }
            }
        });
    }
}
