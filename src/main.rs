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
mod audit;
mod recorder;
mod socks5;

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

    // Direktverbindung oder Drop-in ssh Flags wenn ein Ziel übergeben wurde
    if let Some(target) = cli.target {
        // 1. Dynamischer SOCKS5-Proxy (-D [bind:]port)
        if let Some(dyn_arg) = cli.dynamic_forward {
            let port = dyn_arg.split(':').last().and_then(|p| p.parse::<u16>().ok()).unwrap_or(1080);
            handle_proxy(&target, port).await;
            return;
        }

        // 2. Lokales Port-Forwarding (-L local:remote)
        if let Some(fwd_arg) = cli.local_forward {
            if let Err(e) = tunnel::run_tunnel(&target, &fwd_arg).await {
                eprintln!("  {} Tunnel-Fehler: {}", "✗".bright_red(), e);
            }
            return;
        }

        // 3. Non-interactive Streaming Command (z.B. git, rsync, batch commands)
        if !cli.command_args.is_empty() {
            let cmd_str = cli.command_args.join(" ");
            handle_streaming_exec(&target, &cmd_str, cli.login_user, cli.port).await;
            return;
        }

        // 4. Interaktive Shell-Verbindung
        handle_connect(&target, cli.login_user, cli.port, cli.record).await;
        return;
    }

    match cli.command {
        None => {
            handle_tui().await;
        }
        Some(Commands::Tui) => {
            handle_tui().await;
        }
        Some(Commands::Menu) => {
            handle_interactive_selector().await;
        }
        Some(Commands::Connect { target, user, port, record }) => {
            handle_connect(&target, user, port, record || cli.record).await;
        }
        Some(Commands::Audit { limit, json }) => {
            audit::print_audit_table(limit, json);
        }
        Some(Commands::Replay { target }) => {
            if let Err(e) = recorder::replay_session(&target).await {
                eprintln!("  {} Replay-Fehler: {}", "✗".bright_red(), e);
            }
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
        Some(Commands::Add { name, host, user, port, tags, desc, jump }) => {
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
                jump_host: jump,
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
            let sys_user = crate::config::get_system_username();
            let user = session.as_ref().map(|s| s.username.as_str()).unwrap_or(&sys_user);
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
        Some(Commands::Proxy { target, port }) => {
            handle_proxy(&target, port).await;
        }
        Some(Commands::Completions { shell }) => {
            handle_completions(shell);
        }
        Some(Commands::Help { topic }) => {
            handle_help(topic.as_deref());
        }
    }
}

fn handle_completions(shell: clap_complete::Shell) {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "sb-ssh", &mut std::io::stdout());
}

async fn handle_proxy(target: &str, port: u16) {
    let vault = load_vault();
    let session = load_session();
    let server = if let Some(s) = find_server(&vault, target) {
        s.clone()
    } else {
        let sys_user = crate::config::get_system_username();
        let (parsed_user, parsed_host) = if target.contains('@') {
            let mut parts = target.split('@');
            (parts.next().unwrap_or(&sys_user).to_string(), parts.next().unwrap_or("").to_string())
        } else {
            (sys_user, target.to_string())
        };
        ServerEntry {
            name: target.to_string(),
            host: parsed_host,
            port: 22,
            user: parsed_user,
            tags: vec!["ad-hoc".to_string()],
            identity_file: None,
            description: Some("Ad-hoc SOCKS5 Proxy".to_string()),
            last_connected: None,
            jump_host: None,
        }
    };

    if let Err(e) = socks5::run_socks5_proxy(&server, port, session.as_ref()).await {
        eprintln!("  {} SOCKS5 Fehler: {}", "✗".bright_red(), e);
    }
}

async fn handle_streaming_exec(
    target: &str,
    command: &str,
    user_override: Option<String>,
    port_override: Option<u16>,
) {
    let vault = load_vault();
    let session = load_session();
    let server = if let Some(found) = find_server(&vault, target) {
        let mut s = found.clone();
        if let Some(u) = user_override { s.user = u; }
        if let Some(p) = port_override { s.port = p; }
        s
    } else {
        let sys_user = crate::config::get_system_username();
        let (parsed_user, parsed_host) = if target.contains('@') {
            let mut parts = target.split('@');
            (parts.next().unwrap_or(&sys_user).to_string(), parts.next().unwrap_or("").to_string())
        } else {
            (user_override.unwrap_or(sys_user), target.to_string())
        };
        ServerEntry {
            name: target.to_string(),
            host: parsed_host,
            port: port_override.unwrap_or(22),
            user: parsed_user,
            tags: vec!["ad-hoc".to_string()],
            identity_file: None,
            description: Some("Ad-hoc Exec".to_string()),
            last_connected: None,
            jump_host: None,
        }
    };

    match native_ssh::run_streaming_command(&server, command, session.as_ref()).await {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("  {} Remote-Ausführungsfehler: {}", "✗".bright_red(), e);
            std::process::exit(1);
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
            "  {} {}   {} {}   {} {}\n  {} {}   {} {}   {} {}\n  {} {}   {} {}   {} {}\n  {} {}   {} {}   {} {}",
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
            "[?]".bright_cyan().bold(), "Hilfe & Cheatsheet",
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

        if choice == "?" || choice.eq_ignore_ascii_case("help") || choice.eq_ignore_ascii_case("hilfe") {
            handle_help(None);
            println!("\n  Drücke [ENTER] um zum Menü zurückzukehren...");
            let mut _b = String::new();
            let _ = std::io::stdin().read_line(&mut _b);
            continue;
        }

        if choice.eq_ignore_ascii_case("r") || choice.eq_ignore_ascii_case("refresh") {
            continue;
        }

        if choice.eq_ignore_ascii_case("u") || choice.eq_ignore_ascii_case("tunnel") {
            handle_interactive_tunnel(None).await;
            continue;
        }

        if choice.eq_ignore_ascii_case("i") || choice.eq_ignore_ascii_case("info") {
            handle_interactive_info(None).await;
            continue;
        }

        if choice.eq_ignore_ascii_case("p") || choice.eq_ignore_ascii_case("transfer") || choice.eq_ignore_ascii_case("scp") {
            handle_interactive_transfer(None).await;
            continue;
        }

        if choice.eq_ignore_ascii_case("x") || choice.eq_ignore_ascii_case("exec") {
            handle_interactive_exec(None).await;
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
            handle_interactive_edit(None);
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
            let _ = run_ssh_session(&target, session.as_ref(), false).await;
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

    let sys_user = crate::config::get_system_username();
    print!("  SSH Benutzer (Standard: {}): ", sys_user);
    let _ = stdout().flush();
    let mut user = String::new();
    let _ = stdin().read_line(&mut user);
    let user = user.trim();
    let user = if user.is_empty() { sys_user } else { user.to_string() };

    print!("  SSH Port (Standard: 22): ");
    let _ = stdout().flush();
    let mut port_str = String::new();
    let _ = stdin().read_line(&mut port_str);
    let port = port_str.trim().parse::<u16>().unwrap_or(22);

    print!("  Bastion / Jump-Host (optional, Alias aus Tresor): ");
    let _ = stdout().flush();
    let mut jump_str = String::new();
    let _ = stdin().read_line(&mut jump_str);
    let jump_host = {
        let trimmed = jump_str.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
    };

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
        jump_host,
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

fn handle_interactive_edit(preset: Option<&str>) {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server zum Bearbeiten vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── SERVER BEARBEITEN ──".bright_cyan());
    let target = if let Some(p) = preset {
        vault.servers.iter().find(|s| s.name.eq_ignore_ascii_case(p)).cloned()
    } else {
        print!("  Welchen Server möchtest du bearbeiten? [1-{}, Name oder Enter zum Abbrechen]: ", vault.servers.len());
        let _ = stdout().flush();

        let mut input = String::new();
        if stdin().read_line(&mut input).is_err() { return; }
        let choice = input.trim();
        if choice.is_empty() { return; }

        if let Ok(num) = choice.parse::<usize>() {
            if num >= 1 && num <= vault.servers.len() {
                vault.servers.get(num - 1).cloned()
            } else {
                eprintln!("  {} Ungültige Nummer: {}", "✗".bright_red(), num);
                return;
            }
        } else {
            vault.servers.iter().find(|s| s.name.eq_ignore_ascii_case(choice)).cloned()
        }
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

        print!("  Bastion / Jump-Host [{}]: ", srv.jump_host.as_deref().unwrap_or("keiner").bold());
        let _ = stdout().flush();
        buf.clear();
        let _ = stdin().read_line(&mut buf);
        let trimmed_jump = buf.trim();
        if !trimmed_jump.is_empty() {
            if trimmed_jump.eq_ignore_ascii_case("none") || trimmed_jump.eq_ignore_ascii_case("-") {
                srv.jump_host = None;
            } else {
                srv.jump_host = Some(trimmed_jump.to_string());
            }
        }

        match crate::vault::update_server(&old_name, srv) {
            Ok(_) => println!("  {} Server erfolgreich aktualisiert!\n", "✓".bright_green()),
            Err(e) => eprintln!("  {} Fehler: {}\n", "✗".bright_red(), e),
        }
    } else {
        eprintln!("  {} Server nicht gefunden.\n", "✗".bright_red());
    }
}

async fn handle_interactive_tunnel(preset: Option<&str>) {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── SSH PORT-FORWARDING TUNNEL ÖFFNEN ──".bright_blue());
    let target = if let Some(p) = preset {
        Some(p.to_string())
    } else {
        print!("  Zielserver auswählen [1-{}, Name oder Enter für Standard [1]]: ", vault.servers.len());
        let _ = stdout().flush();
        let mut choice = String::new();
        let _ = stdin().read_line(&mut choice);
        let choice = choice.trim();

        if choice.is_empty() {
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
        }
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

async fn handle_interactive_info(preset: Option<&str>) {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
        return;
    }

    let target = if let Some(p) = preset {
        Some(p.to_string())
    } else {
        println!("\n{}", "── REMOTE SERVER HEALTH PROBE ──".bright_green());
        print!("  Server für Health-Probe auswählen [1-{}, Name oder Enter für Standard [1]]: ", vault.servers.len());
        let _ = stdout().flush();
        let mut choice = String::new();
        let _ = stdin().read_line(&mut choice);
        let choice = choice.trim();

        if choice.is_empty() {
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
        }
    };

    if let Some(target_server) = target {
        probe::print_server_health(&target_server).await;
        println!("\nDrücke [ENTER] um fortzufahren...");
        let mut _b = String::new();
        let _ = stdin().read_line(&mut _b);
    }
}

async fn handle_interactive_transfer(preset: Option<&str>) {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── DATEI-TRANSFER (SFTP / SCP) ──".bright_magenta());
    println!("  [1] Upload (Push):   Lokale Datei -> Remote Server");
    println!("  [2] Download (Pull): Remote Datei -> Lokaler Rechner");
    print!("  Aktion wählen [1/2 oder Enter zum Abbrechen]: ");
    let _ = stdout().flush();
    let mut act = String::new();
    let _ = stdin().read_line(&mut act);
    let act = act.trim();

    if act == "1" || act.eq_ignore_ascii_case("push") {
        let target_srv = if let Some(p) = preset {
            Some(p.to_string())
        } else {
            print!("  Zielserver [1-{}, Name oder Enter für Standard [1]]: ", vault.servers.len());
            let _ = stdout().flush();
            let mut target = String::new();
            let _ = stdin().read_line(&mut target);
            let target = target.trim();
            if target.is_empty() {
                vault.servers.first().map(|s| s.name.clone())
            } else if let Ok(num) = target.parse::<usize>() {
                vault.servers.get(num.saturating_sub(1)).map(|s| s.name.clone())
            } else {
                Some(target.to_string())
            }
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
        let target_srv = if let Some(p) = preset {
            Some(p.to_string())
        } else {
            print!("  Quellserver [1-{}, Name oder Enter für Standard [1]]: ", vault.servers.len());
            let _ = stdout().flush();
            let mut target = String::new();
            let _ = stdin().read_line(&mut target);
            let target = target.trim();
            if target.is_empty() {
                vault.servers.first().map(|s| s.name.clone())
            } else if let Ok(num) = target.parse::<usize>() {
                vault.servers.get(num.saturating_sub(1)).map(|s| s.name.clone())
            } else {
                Some(target.to_string())
            }
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

async fn handle_interactive_exec(preset: Option<&str>) {
    let vault = load_vault();
    if vault.servers.is_empty() {
        println!("  {} Keine Server im Tresor vorhanden.", "!".bright_yellow());
        return;
    }

    println!("\n{}", "── MULTI-SERVER BROADCAST EXECUTION ──".bright_yellow());
    let target_opt: Option<String> = if let Some(p) = preset {
        Some(p.to_string())
    } else {
        print!("  Zielgruppe wählen ['all', Tag oder Server-Name] (Standard: 'all'): ");
        let _ = stdout().flush();
        let mut target = String::new();
        let _ = stdin().read_line(&mut target);
        let target = target.trim();
        if target.is_empty() || target.eq_ignore_ascii_case("all") { None } else { Some(target.to_string()) }
    };

    print!("  Auszuführender Remote-Befehl (z.B. 'uptime' oder 'df -h /'): ");
    let _ = stdout().flush();
    let mut cmd_str = String::new();
    let _ = stdin().read_line(&mut cmd_str);
    let cmd_str = cmd_str.trim();
    if cmd_str.is_empty() { return; }

    if let Err(e) = exec::run_broadcast_exec(target_opt.as_deref(), cmd_str, None).await {
        eprintln!("  {} Broadcast-Fehler: {}\n", "✗".bright_red(), e);
    }
}

async fn handle_tui() {
    loop {
        match run_tui() {
            Ok(Some(TuiAction::Connect(server))) => {
                let session = load_session();
                let _ = run_ssh_session(&server, session.as_ref(), false).await;
                println!("\nDrücke [ENTER] um zur Serverliste zurückzukehren...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
            }
            Ok(Some(TuiAction::TriggerLogin)) => {
                let _ = run_oauth_flow("github").await;
                println!("\nDrücke [ENTER] um zur Serverliste zurückzukehren...");
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
            }
            Ok(Some(TuiAction::TriggerAdd)) => {
                handle_interactive_add();
                println!("\nDrücke [ENTER] um zur Serverliste zurückzukehren...");
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

async fn handle_connect(target: &str, user_override: Option<String>, port_override: Option<u16>, record: bool) {
    let vault = load_vault();
    let session = load_session();

    let server = if let Some(found) = find_server(&vault, target) {
        let mut s = found.clone();
        if let Some(u) = user_override { s.user = u; }
        if let Some(p) = port_override { s.port = p; }
        s
    } else {
        // Ziel direkt als Host/IP oder user@host parsen
        let sys_user = crate::config::get_system_username();
        let (parsed_user, parsed_host) = if target.contains('@') {
            let mut parts = target.split('@');
            (parts.next().unwrap_or(&sys_user).to_string(), parts.next().unwrap_or("").to_string())
        } else {
            (user_override.unwrap_or(sys_user), target.to_string())
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
            jump_host: None,
        }
    };

    if let Err(e) = run_ssh_session(&server, session.as_ref(), record).await {
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

pub fn handle_help(topic: Option<&str>) {
    let t = topic.map(|s| s.to_lowercase());
    match t.as_deref() {
        Some("tui") => {
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE // TUI DASHBOARD HILFE", "💡".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  Aufruf:           {}", "sb-ssh [tui]".bright_yellow().bold());
            println!("  Beschreibung:     Vollbild-Terminal-Oberfläche (Ratatui) mit Live-Ping");
            println!("\n  Tastenkürzel:");
            println!("    {}        Verbindung zum ausgewählten Server herstellen", "[ENTER]".bright_cyan().bold());
            println!("    {}        Neuen Server interaktiv zum Tresor hinzufügen", "[+] / [A]".bright_green().bold());
            println!("    {}        Ausgewählten Server mit Bestätigung löschen", "[D] / [X]".bright_red().bold());
            println!("    {}            Ping-Latenzen aller Server live neu messen", "[R]".bright_yellow().bold());
            println!("    {}            OAuth2 / OIDC Browser-Login ausführen", "[L]".bright_magenta().bold());
            println!("    {}    Navigation in der Server-Tabelle", "[↑/↓/j/k]".bright_cyan());
            println!("    {}        Hilfe-Popup ein- / ausblenden", "[?] / [H]".bright_yellow().bold());
            println!("    {}        TUI beenden / Dialog schließen", "[Q] / [ESC]".bright_white().bold());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        Some("socks5") | Some("proxy") => {
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE // DYNAMISCHER SOCKS5-PROXY (RFC 1928)", "🌐".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  Aufruf:           {}", "sb-ssh proxy <server> [--port 1080]".bright_yellow().bold());
            println!("  OpenSSH-Alias:    {}", "ssh -D 1080 <server>".bright_yellow().bold());
            println!("  Beschreibung:     Öffnet einen lokalen SOCKS5-Server, der allen TCP-Traffic");
            println!("                    in-memory über SSH Direct-TCPIP an den Remote-Host leitet.");
            println!("\n  Beispiele:");
            println!("    # 1. Proxy auf Port 1080 starten:");
            println!("    {}", "sb-ssh proxy srv-prod-01 --port 1080".bright_white());
            println!("\n    # 2. HTTP/HTTPS-Traffic über Remote-Netzwerk tunneln (kein DNS-Leak):");
            println!("    {}", "curl --socks5-hostname 127.0.0.1:1080 https://internal-api.lan".bright_white());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        Some("audit") | Some("replay") | Some("record") => {
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE // BSI IT-GRUNDSCHUTZ AUDIT & RECORDING", "🛡️".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  Compliance:       BSI OPS.1.1.4 (Protokollierung) & DER.1 (Erkennung)");
            println!("  Audit-Speicher:   ~/.sb-ssh/audit/sessions.jsonl");
            println!("  Cast-Speicher:    ~/.sb-ssh/recordings/<session_id>.cast");
            println!("\n  Befehle:");
            println!("    {}      Sitzung aufzeichnen (Asciinema v2)", "sb-ssh connect <server> -r".bright_yellow());
            println!("    {}               Revisions-Tabelle formatieren", "sb-ssh audit".bright_yellow());
            println!("    {}        JSONL-Export für Splunk / Elastic", "sb-ssh audit --json".bright_yellow());
            println!("    {}     Aufgezeichnete Sitzung abspielen", "sb-ssh replay <session_id>".bright_yellow());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        Some("cert") | Some("certs") | Some("oauth") | Some("ca") => {
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE // ZERO-KEY CA & EPHEMERAL CERTIFICATES", "🔑".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  Prinzip:          Keine statischen Keys auf Servern oder Laptops.");
            println!("                    Browser-Login generiert 8h gültige Ed25519-User-Zertifikate.");
            println!("\n  Befehle:");
            println!("    {}               Browser-Authentifizierung (OAuth2 PKCE)", "sb-ssh login".bright_yellow());
            println!("    {}              Aktiven Token- und Zertifikatsstatus prüfen", "sb-ssh status".bright_yellow());
            println!("    {}              Zertifikat manuell generieren (z.B. 12h)", "sb-ssh cert 12".bright_yellow());
            println!("    {}              1-Zeilen sshd_config für neue Server ausgeben", "sb-ssh server-init".bright_yellow());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        Some("tunnel") | Some("forward") => {
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE // TCP PORT-FORWARDING & TUNNELS", "⚡".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  Aufruf:           {}", "sb-ssh tunnel <server> <local_port>:<remote_port>".bright_yellow().bold());
            println!("\n  Beispiele:");
            println!("    # Remote PostgreSQL (5432) auf localhost:5432 binden:");
            println!("    {}", "sb-ssh tunnel srv-db 5432:5432".bright_white());
            println!("\n    # Internen Remote-Webserver auf Port 8080 testen:");
            println!("    {}", "sb-ssh tunnel srv-prod 8080:80".bright_white());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        Some("sftp") | Some("push") | Some("pull") => {
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE // NATIVE SFTP FILE TRANSFERS", "📁".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  Engine:           100% Pure-Rust russh-sftp (keine externe scp.exe)");
            println!("\n  Befehle:");
            println!("    {}    Datei hochladen (Push)", "sb-ssh push <server> <local> [remote]".bright_yellow());
            println!("    {}    Datei herunterladen (Pull)", "sb-ssh pull <server> <remote> [local]".bright_yellow());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        Some("jump") | Some("bastion") | Some("proxyjump") => {
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE // PURE-RUST PROXYJUMP (BASTION KASKADE)", "🌉".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  Prinzip:          SSH-over-SSH Direct-TCPIP Stream im Speicher.");
            println!("                    Keine offenen lokalen Ports, keine externen Prozesse.");
            println!("\n  Einrichtung & Aufruf:");
            println!("    # 1. Server mit Jump-Host hinterlegen:");
            println!("    {}", "sb-ssh add db-internal 10.0.1.5 --jump bastion-dmz".bright_white());
            println!("\n    # 2. Nahtlos verbinden:");
            println!("    {}", "sb-ssh connect db-internal".bright_white());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        Some("exec") | Some("broadcast") => {
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE // MULTI-SERVER BROADCAST EXECUTION", "⚡".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  Aufruf:           {}", "sb-ssh exec \"<befehl>\" [--target all] [--tag <tag>]".bright_yellow().bold());
            println!("\n  Beispiele:");
            println!("    # Uptime parallel auf allen Hosts abfragen:");
            println!("    {}", "sb-ssh exec \"uptime\" --target all".bright_white());
            println!("\n    # Festplattenbelegung nur auf Servern mit Tag 'prod' abfragen:");
            println!("    {}", "sb-ssh exec \"df -h /\" --tag prod".bright_white());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        _ => {
            // General Cheatsheet
            println!("{}", "══════════════════════════════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} S&B NETGATE (`sb-ssh`) // TACTICAL COMMAND & CHEATSHEET REFERENCE", "🛡️".bright_cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════════════════════════════".bright_black());
            println!("  Version: 0.1.0  |  Lizenz: MIT (100% Free & Open-Source)  |  Pure-Rust Standalone");
            println!("  Dokumentation & Code: https://github.com/Skulls-and-Bones/sb-ssh");

            println!("\n  {}", "► 1. INTERAKTIVE BEDIENOBERFLÄCHEN".bright_cyan().bold());
            println!("     {:<32} Startet das moderne Vollbild-TUI-Dashboard", "sb-ssh [tui]".bright_yellow().bold());
            println!("     {:<32} Startet das zeilenbasierte Textmenü", "sb-ssh menu".bright_yellow().bold());

            println!("\n  {}", "► 2. VERBINDUNG & AUTHENTIFIZIERUNG".bright_cyan().bold());
            println!("     {:<32} Direktverbindung (z.B. 'sb-ssh srv-prod-01')", "sb-ssh <server>".bright_yellow().bold());
            println!("     {:<32} Verbindet mit hinterlegtem Server oder ad-hoc", "sb-ssh connect <server>".bright_yellow().bold());
            println!("     {:<32} OAuth2/OIDC Browser-Login (GitHub, Google, OIDC)", "sb-ssh login".bright_yellow().bold());
            println!("     {:<32} Zeigt aktiven Identity-Token & Zertifikats-Restzeit", "sb-ssh status".bright_yellow().bold());
            println!("     {:<32} Beendet aktive Sitzung und löscht lokalen Token", "sb-ssh logout".bright_yellow().bold());
            println!("     {:<32} Erzeugt manuelles ephemeres Ed25519-Zertifikat", "sb-ssh cert [hours]".bright_yellow().bold());

            println!("\n  {}", "► 3. NETZWERK, TUNNEL & PROXY".bright_cyan().bold());
            println!("     {:<32} Dynamischer SOCKS5-Proxy (RFC 1928, Standard: 1080)", "sb-ssh proxy <srv> [--port P]".bright_yellow().bold());
            println!("     {:<32} OpenSSH-kompatibler dynamischer SOCKS5-Proxy", "ssh -D 1080 <server>".bright_yellow().bold());
            println!("     {:<32} TCP Port-Forwarding (z.B. 'sb-ssh tunnel db 5432:5432')", "sb-ssh tunnel <srv> L:R".bright_yellow().bold());
            println!("     {:<32} Server mit vorgeschaltetem Bastion-Host anlegen", "sb-ssh add <s> <ip> --jump <b]".bright_yellow().bold());

            println!("\n  {}", "► 4. BSI IT-GRUNDSCHUTZ AUDIT & RECORDING".bright_cyan().bold());
            println!("     {:<32} Verbindung aufbauen & Sitzung aufzeichnen (.cast)", "sb-ssh connect <srv> -r".bright_yellow().bold());
            println!("     {:<32} Revisionssicheres Sitzungsprotokoll anzeigen", "sb-ssh audit [--limit N]".bright_yellow().bold());
            println!("     {:<32} Audit-Log als JSONL für SIEM (Splunk / Elastic)", "sb-ssh audit --json".bright_yellow().bold());
            println!("     {:<32} Aufgezeichnete Session nativ im Terminal abspielen", "sb-ssh replay <session_id>".bright_yellow().bold());

            println!("\n  {}", "► 5. DATEITRANSFER & BROADCAST-AUTOMATION".bright_cyan().bold());
            println!("     {:<32} Datei per nativem SFTP hochladen (Push)", "sb-ssh push <srv> <local> [dst]".bright_yellow().bold());
            println!("     {:<32} Datei per nativem SFTP herunterladen (Pull)", "sb-ssh pull <srv> <remote> [dst]".bright_yellow().bold());
            println!("     {:<32} Parallele Befehlsausführung über Server/Tags", "sb-ssh exec \"<cmd>\" [--tag T]".bright_yellow().bold());
            println!("     {:<32} Non-interactive Remote-Kommando (CI / Skripte)", "sb-ssh <srv> \"<command>\"".bright_yellow().bold());

            println!("\n  {}", "► 6. SERVER-VERWALTUNG & SYSTEM".bright_cyan().bold());
            println!("     {:<32} Listet alle Server mit Live-Latenz auf", "sb-ssh list [--tag <tag>]".bright_yellow().bold());
            println!("     {:<32} Neuen Server zum lokalen Tresor hinzufügen", "sb-ssh add <name> <host>".bright_yellow().bold());
            println!("     {:<32} Server aus dem Tresor löschen", "sb-ssh remove <name>".bright_yellow().bold());
            println!("     {:<32} Non-interactive CPU/RAM/Disk Health-Probe", "sb-ssh info <server>".bright_yellow().bold());
            println!("     {:<32} 1-Zeilen-Setup für Zielserver (sshd_config)", "sb-ssh server-init".bright_yellow().bold());
            println!("     {:<32} Shell-Autovervollständigung (powershell, zsh, ...)", "sb-ssh completions <shell>".bright_yellow().bold());
            println!("     {:<32} Detaillierte Hilfe zu einem Thema anzeigen", "sb-ssh help [thema]".bright_yellow().bold());

            println!("\n  {}", "► 7. VERFÜGBARE HILFE-THEMEN (sb-ssh help <thema>):".bright_black());
            println!("     tui, proxy, socks5, audit, replay, certs, tunnel, sftp, jump, exec");

            println!("{}", "══════════════════════════════════════════════════════════════════════════════════════════".bright_black());
        }
    }
}
