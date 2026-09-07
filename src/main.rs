mod cli;
mod config;
mod vault;
mod cert;
mod auth;
mod runner;
mod tui;
mod tunnel;
mod probe;
mod transfer;
mod exec;
mod native_ssh;

use std::io::{stdin, stdout, Write};
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

    // Direktverbindung wenn ein Ziel übergeben wurde (Drop-in ssh replacement)
    if let Some(target) = cli.target {
        handle_connect(&target, None, None).await;
        return;
    }

    match cli.command {
        None => {
            handle_interactive_selector().await;
        }
        Some(Commands::Tui) => {
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
        Some(Commands::Tunnel { target, forward }) => {
            if let Err(e) = tunnel::run_tunnel(&target, &forward).await {
                eprintln!("  {} Tunnel-Fehler: {}", "✗".bright_red(), e);
            }
        }
        Some(Commands::Info { target }) => {
            probe::print_server_health(&target).await;
        }
        Some(Commands::Push { target, local_path, remote_path }) => {
            if let Err(e) = transfer::push_file(&target, &local_path, remote_path.as_deref()).await {
                eprintln!("  {} Upload-Fehler: {}", "✗".bright_red(), e);
            }
        }
        Some(Commands::Pull { target, remote_path, local_path }) => {
            if let Err(e) = transfer::pull_file(&target, &remote_path, local_path.as_deref()).await {
                eprintln!("  {} Download-Fehler: {}", "✗".bright_red(), e);
            }
        }
        Some(Commands::Exec { command_to_run, target, tag }) => {
            let target_sel = if target.eq_ignore_ascii_case("all") { None } else { Some(target.as_str()) };
            if let Err(e) = exec::run_broadcast_exec(target_sel, &command_to_run, tag.as_deref()).await {
                eprintln!("  {} Broadcast-Fehler: {}", "✗".bright_red(), e);
            }
        }
        Some(Commands::ServerInit) => {
            handle_server_init();
        }
    }
}

async fn handle_interactive_selector() {
    loop {
        let vault = load_vault();
        let session = load_session();

        println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());
        println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "VERFÜGBARE SERVER & ZIELE".bold());
        println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());

        if vault.servers.is_empty() {
            println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
            println!("  Tipp: Drücke [+] um einen Server hinzuzufügen.");
        } else {
            println!(
                "  {:<6} {:<20} {:<24} {:<10} {:<12} {}",
                "NR.".bold(),
                "NAME".bold(),
                "ZIEL (HOST:PORT)".bold(),
                "BENUTZER".bold(),
                "LATENZ".bold(),
                "TAGS".bold()
            );
            println!("  {}", "─".repeat(82).bright_black());

            for (idx, srv) in vault.servers.iter().enumerate() {
                let num_str = format!("[{}]", idx + 1);
                let latency_str = match check_server_latency(&srv.host, srv.port) {
                    Some(d) => format!("{:.1}ms", d.as_secs_f64() * 1000.0).bright_green().to_string(),
                    None => "OFFLINE".bright_red().to_string(),
                };
                let tags_str = srv.tags.join(", ");
                let host_port = format!("{}:{}", srv.host, srv.port);

                println!(
                    "  {:<6} {:<20} {:<24} {:<10} {:<12} {}",
                    num_str.bright_yellow().bold(),
                    srv.name.bright_white().bold(),
                    host_port,
                    srv.user.bright_cyan(),
                    latency_str,
                    tags_str.bright_black()
                );
            }
        }

        println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());
        println!(
            "  {} {}   {} {}   {} {}\n  {} {}   {} {}   {} {}\n  {} {}   {} {}   {} {}\n  {} {}   {} {}",
            "[1-N / Enter]".bright_yellow().bold(), "Verbinden",
            "[+]".bright_cyan().bold(), "Server hinzufügen",
            "[-]".bright_red().bold(), "Server löschen",
            "[e]".bright_magenta().bold(), "Bearbeiten",
            "[u]".bright_blue().bold(), "SSH-Tunnel",
            "[i]".bright_green().bold(), "Health-Probe",
            "[p]".bright_magenta().bold(), "Transfer (SCP)",
            "[x]".bright_yellow().bold(), "Broadcast (Exec)",
            "[r]".bright_yellow().bold(), "Neu messen",
            "[t]".bright_cyan().bold(), "Vollbild-TUI",
            "[q]".bright_black().bold(), "Beenden"
        );
        print!("\n  {} Befehl oder Server wählen [1-{}, Name oder Aktion] (Standard: [1]): ", "►".bright_cyan(), vault.servers.len().max(1));
        let _ = stdout().flush();

        let mut input = String::new();
        if stdin().read_line(&mut input).is_err() {
            break;
        }
        let choice = input.trim();

        if choice.eq_ignore_ascii_case("q") || choice.eq_ignore_ascii_case("exit") {
            println!("  Auf Wiedersehen!");
            break;
        }

        if choice.eq_ignore_ascii_case("r") || choice.eq_ignore_ascii_case("refresh") {
            continue;
        }

        if choice.eq_ignore_ascii_case("u") || choice.eq_ignore_ascii_case("tunnel") {
            handle_interactive_tunnel().await;
            continue;
        }

        if choice.eq_ignore_ascii_case("i") || choice.eq_ignore_ascii_case("info") {
            handle_interactive_info().await;
            continue;
        }

        if choice.eq_ignore_ascii_case("p") || choice.eq_ignore_ascii_case("transfer") || choice.eq_ignore_ascii_case("scp") {
            handle_interactive_transfer().await;
            continue;
        }

        if choice.eq_ignore_ascii_case("x") || choice.eq_ignore_ascii_case("exec") {
            handle_interactive_exec().await;
            continue;
        }

        if choice.eq_ignore_ascii_case("t") || choice.eq_ignore_ascii_case("tui") {
            handle_tui().await;
            continue;
        }

        if choice.eq_ignore_ascii_case("s") || choice.eq_ignore_ascii_case("status") {
            handle_status();
            println!("\nDrücke [ENTER] um fortzufahren...");
            let mut _b = String::new();
            let _ = std::io::stdin().read_line(&mut _b);
            continue;
        }

        if choice == "+" || choice.eq_ignore_ascii_case("add") {
            handle_interactive_add();
            continue;
        }

        if choice == "-" || choice.eq_ignore_ascii_case("rm") || choice.eq_ignore_ascii_case("del") || choice.eq_ignore_ascii_case("delete") {
            handle_interactive_delete();
            continue;
        }

        if choice.eq_ignore_ascii_case("e") || choice.eq_ignore_ascii_case("edit") {
            handle_interactive_edit();
            continue;
        }

        // Nummer wählen oder Standard [1]
        let selected_server = if choice.is_empty() {
            vault.servers.first().cloned()
        } else if let Ok(num) = choice.parse::<usize>() {
            if num >= 1 && num <= vault.servers.len() {
                vault.servers.get(num - 1).cloned()
            } else {
                eprintln!("  {} Ungültige Nummer: {}", "✗".bright_red(), num);
                continue;
            }
        } else {
            // Nach Name oder IP suchen
            vault.servers.iter().find(|s| s.name.eq_ignore_ascii_case(choice) || s.host.eq_ignore_ascii_case(choice)).cloned()
        };

        if let Some(target) = selected_server {
            println!("\n  {} Starte Verbindung zu '{}'...", "►".bright_cyan(), target.name.bold());
            let _ = run_ssh_session(&target, session.as_ref()).await;
            println!("\nDrücke [ENTER] um zur Serverliste zurückzukehren...");
            let mut _b = String::new();
            let _ = stdin().read_line(&mut _b);
        } else {
            eprintln!("  {} Server '{}' nicht gefunden.", "✗".bright_red(), choice);
        }
    }
}

fn handle_interactive_add() {
    println!("\n{}", "── NEUEN SERVER ZUM TRESOR HINZUFÜGEN ──".bright_cyan());

    print!("  Server-Alias / Name (z.B. vps-backup): ");
    let _ = stdout().flush();
    let mut name = String::new();
    let _ = stdin().read_line(&mut name);
    let name = name.trim().to_string();
    if name.is_empty() { return; }

    print!("  Host / IP-Adresse: ");
    let _ = stdout().flush();
    let mut host = String::new();
    let _ = stdin().read_line(&mut host);
    let host = host.trim().to_string();
    if host.is_empty() { return; }

    print!("  SSH Benutzer (Standard: leonf): ");
    let _ = stdout().flush();
    let mut user = String::new();
    let _ = stdin().read_line(&mut user);
    let user = user.trim();
    let user = if user.is_empty() { "leonf".to_string() } else { user.to_string() };

    print!("  SSH Port (Standard: 22): ");
    let _ = stdout().flush();
    let mut port_str = String::new();
    let _ = stdin().read_line(&mut port_str);
    let port = port_str.trim().parse::<u16>().unwrap_or(22);

    print!("  Tags (kommagetrennt, z.B. prod, vps): ");
    let _ = stdout().flush();
    let mut tags_str = String::new();
    let _ = stdin().read_line(&mut tags_str);
    let tags = tags_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let entry = ServerEntry {
        name: name.clone(),
        host,
        port,
        user,
        tags,
        identity_file: None,
        description: Some("Manuell hinzugefügt".to_string()),
        last_connected: None,
    };

    match add_server(entry) {
        Ok(_) => println!("  {} Server '{}' erfolgreich gespeichert!\n", "✓".bright_green(), name.bold()),
        Err(e) => eprintln!("  {} Fehler: {}\n", "✗".bright_red(), e),
    }
}

fn handle_interactive_delete() {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server zum Löschen vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── SERVER AUS DEM TRESOR ENTFERNEN ──".bright_red());
    print!("  Welchen Server möchtest du löschen? [1-{}, Name oder Enter zum Abbrechen]: ", vault.servers.len());
    let _ = stdout().flush();

    let mut input = String::new();
    if stdin().read_line(&mut input).is_err() { return; }
    let choice = input.trim();
    if choice.is_empty() { return; }

    let target_name = if let Ok(num) = choice.parse::<usize>() {
        if num >= 1 && num <= vault.servers.len() {
            vault.servers.get(num - 1).map(|s| s.name.clone())
        } else {
            eprintln!("  {} Ungültige Nummer: {}", "✗".bright_red(), num);
            return;
        }
    } else {
        vault.servers.iter().find(|s| s.name.eq_ignore_ascii_case(choice)).map(|s| s.name.clone())
    };

    if let Some(name) = target_name {
        print!("  Server '{}' wirklich löschen? [j/N]: ", name.bold());
        let _ = stdout().flush();
        let mut confirm = String::new();
        let _ = stdin().read_line(&mut confirm);
        if confirm.trim().eq_ignore_ascii_case("j") || confirm.trim().eq_ignore_ascii_case("y") {
            match remove_server(&name) {
                Ok(true) => println!("  {} Server '{}' wurde erfolgreich gelöscht!\n", "✓".bright_green(), name.bold()),
                Ok(false) => println!("  {} Server nicht gefunden.\n", "i".bright_yellow()),
                Err(e) => eprintln!("  {} Fehler beim Löschen: {}\n", "✗".bright_red(), e),
            }
        } else {
            println!("  Löschen abgebrochen.\n");
        }
    } else {
        eprintln!("  {} Server '{}' nicht im Tresor gefunden.\n", "✗".bright_red(), choice);
    }
}

fn handle_interactive_edit() {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server zum Bearbeiten vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── SERVER BEARBEITEN ──".bright_cyan());
    print!("  Welchen Server möchtest du bearbeiten? [1-{}, Name oder Enter zum Abbrechen]: ", vault.servers.len());
    let _ = stdout().flush();

    let mut input = String::new();
    if stdin().read_line(&mut input).is_err() { return; }
    let choice = input.trim();
    if choice.is_empty() { return; }

    let target = if let Ok(num) = choice.parse::<usize>() {
        if num >= 1 && num <= vault.servers.len() {
            vault.servers.get(num - 1).cloned()
        } else {
            eprintln!("  {} Ungültige Nummer: {}", "✗".bright_red(), num);
            return;
        }
    } else {
        vault.servers.iter().find(|s| s.name.eq_ignore_ascii_case(choice)).cloned()
    };

    if let Some(mut srv) = target {
        let old_name = srv.name.clone();
        println!("  (Drücke einfach [ENTER], um den bisherigen Wert beizubehalten)\n");

        print!("  Name [{}]: ", srv.name.bold());
        let _ = stdout().flush();
        let mut buf = String::new();
        let _ = stdin().read_line(&mut buf);
        if !buf.trim().is_empty() { srv.name = buf.trim().to_string(); }

        print!("  Host / IP [{}]: ", srv.host.bold());
        let _ = stdout().flush();
        buf.clear();
        let _ = stdin().read_line(&mut buf);
        if !buf.trim().is_empty() { srv.host = buf.trim().to_string(); }

        print!("  Benutzer [{}]: ", srv.user.bold());
        let _ = stdout().flush();
        buf.clear();
        let _ = stdin().read_line(&mut buf);
        if !buf.trim().is_empty() { srv.user = buf.trim().to_string(); }

        print!("  Port [{}]: ", srv.port);
        let _ = stdout().flush();
        buf.clear();
        let _ = stdin().read_line(&mut buf);
        if let Ok(p) = buf.trim().parse::<u16>() { srv.port = p; }

        print!("  Tags [{}]: ", srv.tags.join(", ").bold());
        let _ = stdout().flush();
        buf.clear();
        let _ = stdin().read_line(&mut buf);
        if !buf.trim().is_empty() {
            srv.tags = buf.trim().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }

        match crate::vault::update_server(&old_name, srv) {
            Ok(_) => println!("  {} Server erfolgreich aktualisiert!\n", "✓".bright_green()),
            Err(e) => eprintln!("  {} Fehler: {}\n", "✗".bright_red(), e),
        }
    } else {
        eprintln!("  {} Server '{}' nicht gefunden.\n", "✗".bright_red(), choice);
    }
}

async fn handle_interactive_tunnel() {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── SSH PORT-FORWARDING TUNNEL ÖFFNEN ──".bright_blue());
    print!("  Zielserver auswählen [1-{}, Name oder Enter für Standard [1]]: ", vault.servers.len());
    let _ = stdout().flush();
    let mut choice = String::new();
    let _ = stdin().read_line(&mut choice);
    let choice = choice.trim();

    let target = if choice.is_empty() {
        vault.servers.first().map(|s| s.name.clone())
    } else if let Ok(num) = choice.parse::<usize>() {
        if num >= 1 && num <= vault.servers.len() {
            vault.servers.get(num - 1).map(|s| s.name.clone())
        } else {
            eprintln!("  {} Ungültige Nummer: {}", "✗".bright_red(), num);
            return;
        }
    } else {
        Some(choice.to_string())
    };

    if let Some(target_server) = target {
        print!("  Port-Weiterleitung (z.B. '8080:80' oder '5432:5432'): ");
        let _ = stdout().flush();
        let mut forward = String::new();
        let _ = stdin().read_line(&mut forward);
        let forward = forward.trim();
        if forward.is_empty() { return; }

        if let Err(e) = tunnel::run_tunnel(&target_server, forward).await {
            eprintln!("  {} Tunnel-Fehler: {}", "✗".bright_red(), e);
        }
    }
}

async fn handle_interactive_info() {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── REMOTE SERVER HEALTH PROBE ──".bright_green());
    print!("  Server für Health-Probe auswählen [1-{}, Name oder Enter für Standard [1]]: ", vault.servers.len());
    let _ = stdout().flush();
    let mut choice = String::new();
    let _ = stdin().read_line(&mut choice);
    let choice = choice.trim();

    let target = if choice.is_empty() {
        vault.servers.first().map(|s| s.name.clone())
    } else if let Ok(num) = choice.parse::<usize>() {
        if num >= 1 && num <= vault.servers.len() {
            vault.servers.get(num - 1).map(|s| s.name.clone())
        } else {
            eprintln!("  {} Ungültige Nummer: {}", "✗".bright_red(), num);
            return;
        }
    } else {
        Some(choice.to_string())
    };

    if let Some(target_server) = target {
        probe::print_server_health(&target_server).await;
        println!("\nDrücke [ENTER] um fortzufahren...");
        let mut _b = String::new();
        let _ = stdin().read_line(&mut _b);
    }
}

async fn handle_interactive_transfer() {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── DATEI-TRANSFER (SCP) ──".bright_magenta());
    println!("  [1] Upload (Push):   Lokale Datei -> Remote Server");
    println!("  [2] Download (Pull): Remote Datei -> Lokaler Rechner");
    print!("  Aktion wählen [1/2 oder Enter zum Abbrechen]: ");
    let _ = stdout().flush();
    let mut act = String::new();
    let _ = stdin().read_line(&mut act);
    let act = act.trim();

    if act == "1" || act.eq_ignore_ascii_case("push") {
        print!("  Zielserver [1-{}, Name oder Enter für Standard [1]]: ", vault.servers.len());
        let _ = stdout().flush();
        let mut target = String::new();
        let _ = stdin().read_line(&mut target);
        let target = target.trim();
        let target_srv = if target.is_empty() {
            vault.servers.first().map(|s| s.name.clone())
        } else if let Ok(num) = target.parse::<usize>() {
            vault.servers.get(num.saturating_sub(1)).map(|s| s.name.clone())
        } else {
            Some(target.to_string())
        };

        if let Some(srv_name) = target_srv {
            print!("  Lokaler Dateipfad: ");
            let _ = stdout().flush();
            let mut local = String::new();
            let _ = stdin().read_line(&mut local);
            let local = local.trim();
            if local.is_empty() { return; }

            print!("  Entfernter Zielpfad (z.B. '/tmp/' oder Enter für Home): ");
            let _ = stdout().flush();
            let mut remote = String::new();
            let _ = stdin().read_line(&mut remote);
            let remote = remote.trim();
            let remote_opt = if remote.is_empty() { None } else { Some(remote) };

            if let Err(e) = transfer::push_file(&srv_name, local, remote_opt).await {
                eprintln!("  {} Fehler: {}\n", "✗".bright_red(), e);
            }
        }
    } else if act == "2" || act.eq_ignore_ascii_case("pull") {
        print!("  Quellserver [1-{}, Name oder Enter für Standard [1]]: ", vault.servers.len());
        let _ = stdout().flush();
        let mut target = String::new();
        let _ = stdin().read_line(&mut target);
        let target = target.trim();
        let target_srv = if target.is_empty() {
            vault.servers.first().map(|s| s.name.clone())
        } else if let Ok(num) = target.parse::<usize>() {
            vault.servers.get(num.saturating_sub(1)).map(|s| s.name.clone())
        } else {
            Some(target.to_string())
        };

        if let Some(srv_name) = target_srv {
            print!("  Entfernter Dateipfad (z.B. '/var/log/nginx/access.log'): ");
            let _ = stdout().flush();
            let mut remote = String::new();
            let _ = stdin().read_line(&mut remote);
            let remote = remote.trim();
            if remote.is_empty() { return; }

            print!("  Lokaler Zielpfad (Enter für aktuelles Verzeichnis '.'): ");
            let _ = stdout().flush();
            let mut local = String::new();
            let _ = stdin().read_line(&mut local);
            let local = local.trim();
            let local_opt = if local.is_empty() { None } else { Some(local) };

            if let Err(e) = transfer::pull_file(&srv_name, remote, local_opt).await {
                eprintln!("  {} Fehler: {}\n", "✗".bright_red(), e);
            }
        }
    }
}

async fn handle_interactive_exec() {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── MULTI-SERVER BROADCAST EXECUTION ──".bright_yellow());
    print!("  Zielgruppe wählen ['all', Tag oder Server-Name] (Standard: 'all'): ");
    let _ = stdout().flush();
    let mut target = String::new();
    let _ = stdin().read_line(&mut target);
    let target = target.trim();
    let target_opt = if target.is_empty() || target.eq_ignore_ascii_case("all") { None } else { Some(target) };

    print!("  Auszuführender Remote-Befehl (z.B. 'uptime' oder 'df -h /'): ");
    let _ = stdout().flush();
    let mut cmd_str = String::new();
    let _ = stdin().read_line(&mut cmd_str);
    let cmd_str = cmd_str.trim();
    if cmd_str.is_empty() { return; }

    if let Err(e) = exec::run_broadcast_exec(target_opt, cmd_str, None).await {
        eprintln!("  {} Broadcast-Fehler: {}\n", "✗".bright_red(), e);
    }
}

async fn handle_tui() {
    loop {
        match run_tui() {
            Ok(Some(TuiAction::Connect(server))) => {
                let session = load_session();
                let _ = run_ssh_session(&server, session.as_ref()).await;
                println!("\nDrücke [ENTER] um zur TUI zurückzukehren...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
            }
            Ok(Some(TuiAction::TriggerLogin)) => {
                let _ = run_oauth_flow("github").await;
            }
            Ok(Some(TuiAction::TriggerAdd)) => {
                handle_interactive_add();
            }
            Ok(Some(TuiAction::TriggerInfo(server))) => {
                probe::print_server_health(&server.name).await;
                println!("\nDrücke [ENTER] um zur TUI zurückzukehren...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
            }
            Ok(Some(TuiAction::TriggerTunnel(server))) => {
                print!("\n  Port-Weiterleitung für '{}' (z.B. '8080:80' oder '5432:5432'): ", server.name.bold());
                let _ = stdout().flush();
                let mut forward = String::new();
                let _ = stdin().read_line(&mut forward);
                let forward = forward.trim();
                if !forward.is_empty() {
                    let _ = tunnel::run_tunnel(&server.name, forward).await;
                }
                println!("\nDrücke [ENTER] um zur TUI zurückzukehren...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
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

    if let Err(e) = run_ssh_session(&server, session.as_ref()).await {
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
