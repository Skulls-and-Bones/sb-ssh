mod cli;
mod config;
mod vault;
mod cert;
mod auth;
mod runner;
mod tui;

use clap::Parser;
use colored::*;
use cli::{Cli, Commands};
use config::{clear_session, load_session};
use vault::{add_server, find_server, load_vault, remove_server, ServerEntry};
use runner::{check_server_latency, run_ssh_session};
use cert::{generate_ephemeral_certificate, get_ca_public_key_string};
use auth::{login_local_dev, run_oauth_flow};
use tui::{run_tui, TuiAction};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => {
            handle_tui().await;
        }
        Some(Commands::Connect { target, user, port }) => {
            handle_connect(&target, user, port).await;
        }
        Some(Commands::Login { provider, dev }) => {
            if let Some(dev_user) = dev {
                match login_local_dev(&dev_user, 8) {
                    Ok(sess) => {
                        println!("  {} Lokaler Entwickler-Login erfolgreich: {}", "✓".bright_green(), sess.username.bold());
                        println!("  {} Gültigkeit: {}", "►".bright_cyan(), sess.time_remaining_str().bright_yellow());
                    }
                    Err(e) => eprintln!("  {} Fehler beim Login: {}", "✗".bright_red(), e),
                }
            } else {
                match run_oauth_flow(&provider).await {
                    Ok(res) => println!("\n  {} Authentifiziert als: {}", "✓".bright_green(), res.username.bold()),
                    Err(e) => eprintln!("\n  {} Authentifizierungsfehler: {}", "✗".bright_red(), e),
                }
            }
        }
        Some(Commands::Logout) => {
            match clear_session() {
                Ok(_) => println!("  {} Sitzung beendet. Lokaler OAuth-Token wurde gelöscht.", "✓".bright_green()),
                Err(e) => eprintln!("  {} Fehler beim Logout: {}", "✗".bright_red(), e),
            }
        }
        Some(Commands::Status) => {
            handle_status();
        }
        Some(Commands::Add { name, host, user, port, tags, desc }) => {
            let tags_vec = tags
                .map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default();

            let entry = ServerEntry {
                name: name.clone(),
                host,
                port,
                user,
                tags: tags_vec,
                identity_file: None,
                description: desc,
                last_connected: None,
            };

            match add_server(entry) {
                Ok(_) => println!("  {} Server '{}' erfolgreich zum Tresor hinzugefügt!", "✓".bright_green(), name.bold()),
                Err(e) => eprintln!("  {} Fehler: {}", "✗".bright_red(), e),
            }
        }
        Some(Commands::List { tag }) => {
            handle_list(tag.as_deref());
        }
        Some(Commands::Remove { name }) => {
            match remove_server(&name) {
                Ok(true) => println!("  {} Server '{}' erfolgreich entfernt.", "✓".bright_green(), name.bold()),
                Ok(false) => println!("  {} Server '{}' nicht im Tresor gefunden.", "i".bright_yellow(), name),
                Err(e) => eprintln!("  {} Fehler: {}", "✗".bright_red(), e),
            }
        }
        Some(Commands::Cert { hours }) => {
            let session = load_session();
            let user = session.as_ref().map(|s| s.username.as_str()).unwrap_or("leonf");
            println!("  {} Erzeuge ephemeres OpenSSH Ed25519-Zertifikat für '{}' ({}h)...", "►".bright_cyan(), user.bold(), hours);
            match generate_ephemeral_certificate(user, &[user, "root", "admin"], hours) {
                Ok(bundle) => {
                    println!("  {} Zertifikat erfolgreich ausgestellt!", "✓".bright_green());
                    println!("  {} Pfad:         {}", "►".bright_cyan(), bundle.cert_path.display().to_string().bright_yellow());
                    println!("  {} Gültig bis:   {}", "►".bright_cyan(), bundle.valid_until.to_rfc3339().bright_cyan());
                    println!("\n  {} OpenSSH Public Key:", "i".bright_black());
                    println!("    {}", bundle.public_key_openssh.bright_white());
                }
                Err(e) => eprintln!("  {} Fehler bei Zertifikatserstellung: {}", "✗".bright_red(), e),
            }
        }
        Some(Commands::ServerInit) => {
            handle_server_init();
        }
    }
}

async fn handle_tui() {
    loop {
        match run_tui() {
            Ok(Some(TuiAction::Connect(server))) => {
                let session = load_session();
                let _ = run_ssh_session(&server, session.as_ref());
                println!("\nDrücke [ENTER] um zur TUI zurückzukehren...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
            }
            Ok(Some(TuiAction::TriggerLogin)) => {
                let _ = run_oauth_flow("github").await;
            }
            Ok(Some(TuiAction::Quit)) | Ok(None) => break,
            Err(e) => {
                eprintln!("TUI Fehler: {}", e);
                break;
            }
        }
    }
}

async fn handle_connect(target: &str, user_override: Option<String>, port_override: Option<u16>) {
    let vault = load_vault();
    let session = load_session();

    let server = if let Some(found) = find_server(&vault, target) {
        let mut s = found.clone();
        if let Some(u) = user_override { s.user = u; }
        if let Some(p) = port_override { s.port = p; }
        s
    } else {
        // Ziel direkt als Host/IP oder user@host parsen
        let (parsed_user, parsed_host) = if target.contains('@') {
            let mut parts = target.split('@');
            (parts.next().unwrap_or("leonf").to_string(), parts.next().unwrap_or("").to_string())
        } else {
            (user_override.unwrap_or_else(|| "leonf".to_string()), target.to_string())
        };

        ServerEntry {
            name: target.to_string(),
            host: parsed_host,
            port: port_override.unwrap_or(22),
            user: parsed_user,
            tags: vec!["ad-hoc".to_string()],
            identity_file: None,
            description: Some("Ad-hoc Verbindung".to_string()),
            last_connected: None,
        }
    };

    if let Err(e) = run_ssh_session(&server, session.as_ref()) {
        eprintln!("  {} Verbindungsfehler: {}", "✗".bright_red(), e);
    }
}

fn handle_status() {
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "SYSTEM- & AUTH-STATUS".bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());

    if let Some(sess) = load_session() {
        println!("  {} OAuth-Sitzung:   {}", "✓".bright_green(), "AKTIV".bright_green().bold());
        println!("  {} Identität:       {} ({})", "►".bright_cyan(), sess.username.bold(), sess.email.as_deref().unwrap_or("Keine E-Mail"));
        println!("  {} Provider:        {}", "►".bright_cyan(), sess.provider.bright_cyan());
        println!("  {} Gültig bis:      {} ({})", "►".bright_cyan(), sess.expires_at.format("%d.%m.%Y %H:%M:%S UTC"), sess.time_remaining_str().bright_yellow());
    } else {
        println!("  {} OAuth-Sitzung:   {}", "!".bright_yellow(), "INAKTIV / ABGELAUFEN".bright_yellow().bold());
        println!("  {} Tipp: Führe '{}' aus, um eine frische Sitzung zu starten.", "i".bright_black(), "sb-ssh login".bright_cyan());
    }

    println!("\n  {} SSH Certificate Authority (CA):", "►".bright_cyan());
    match get_ca_public_key_string() {
        Ok(ca_pub) => {
            println!("  {} Status:          {}", "✓".bright_green(), "BEREIT & INITIALISIERT".bright_green());
            println!("  {} CA Public Key:   {}", "►".bright_cyan(), ca_pub.trim().bright_white());
        }
        Err(e) => {
            println!("  {} CA Status:       Fehler ({})", "✗".bright_red(), e);
        }
    }
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
}

fn handle_list(tag_filter: Option<&str>) {
    let vault = load_vault();
    let servers: Vec<&ServerEntry> = if let Some(tag) = tag_filter {
        vault.servers.iter().filter(|s| s.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))).collect()
    } else {
        vault.servers.iter().collect()
    };

    println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), format!("SERVER TRESOR ({} Einträge)", servers.len()).bold());
    println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());

    println!("  {:<18} {:<24} {:<12} {:<12} {:<20}", "NAME", "HOST:PORT", "USER", "LATENZ", "TAGS");
    println!("  {}", "─".repeat(84).bright_black());

    for s in servers {
        let lat_str = match check_server_latency(&s.host, s.port) {
            Some(d) => format!("{:.1}ms", d.as_secs_f64() * 1000.0).bright_green().to_string(),
            None => "Offline / ---".bright_red().to_string(),
        };

        println!(
            "  {:<18} {:<24} {:<12} {:<12} {:<20}",
            s.name.bold(),
            format!("{}:{}", s.host, s.port).bright_white(),
            s.user.bright_yellow(),
            lat_str,
            s.tags.join(", ").bright_blue()
        );
    }
    println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());
}

fn handle_server_init() {
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "SERVER CA EINRICHTUNG (1-KLICK)".bold());
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());

    let ca_pub = match get_ca_public_key_string() {
        Ok(k) => k.trim().to_string(),
        Err(e) => {
            eprintln!("Fehler: {}", e);
            return;
        }
    };

    println!("  Um einen Zielserver für den passwortlosen, schlüsselfreien OAuth-Zugriff");
    println!("  mit kurzlebigen S&B-Zertifikaten zu autorisieren, führe auf dem Server aus:\n");

    println!("{}", "  # 1. S&B CA Public Key hinterlegen:".bright_yellow());
    println!("  sudo mkdir -p /etc/ssh");
    println!("  echo '{}' | sudo tee /etc/ssh/sb_ca.pub > /dev/null\n", ca_pub.bright_cyan());

    println!("{}", "  # 2. In /etc/ssh/sshd_config eintragen:".bright_yellow());
    println!("  echo 'TrustedUserCAKeys /etc/ssh/sb_ca.pub' | sudo tee -a /etc/ssh/sshd_config > /dev/null");
    println!("  sudo sshd -t && sudo systemctl reload ssh\n");

    println!("{}", "  # 3. Fertig!".bright_green().bold());
    println!("  Danach benötigt kein Benutzer mehr statische Keys in ~/.ssh/authorized_keys.");
    println!("  Jeder Zugriff wird über den OAuth-Token und das 8h-Zertifikat autorisiert.\n");
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
}
