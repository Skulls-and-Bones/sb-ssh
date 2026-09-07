use std::time::Instant;
use colored::*;
use crate::config::load_session;
use crate::vault::{load_vault, ServerEntry};

pub struct ExecResult {
    pub server_name: String,
    pub host: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: f64,
}

pub async fn run_broadcast_exec(
    target_selector: Option<&str>,
    command_str: &str,
    tag_filter: Option<&str>,
) -> Result<(), String> {
    let vault = load_vault();
    let session = load_session();

    // 1. Relevante Zielserver ermitteln
    let targets: Vec<ServerEntry> = if let Some(tag) = tag_filter {
        vault.servers.into_iter().filter(|s| s.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))).collect()
    } else {
        match target_selector {
            Some("all") | None => vault.servers,
            Some(name) => {
                vault.servers.into_iter().filter(|s| s.name.eq_ignore_ascii_case(name) || s.tags.iter().any(|t| t.eq_ignore_ascii_case(name))).collect()
            }
        }
    };

    if targets.is_empty() {
        return Err("Keine passenden Server für die Ausführung gefunden.".to_string());
    }

    println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "MULTI-SERVER BROADCAST EXECUTION".bold());
    println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Befehl:  {}", "►".bright_cyan(), command_str.bright_yellow().bold());
    println!("  {} Server:  {} Ziele (parallele Ausführung)", "►".bright_cyan(), targets.len());
    println!("{}", "──────────────────────────────────────────────────────────────────────────────────────".bright_black());

    let mut tasks = Vec::new();

    for server in targets {
        let cmd_string = command_str.to_string();
        let session_clone = session.clone();

        let handle = tokio::spawn(async move {
            let start = Instant::now();
            match crate::native_ssh::run_remote_command(&server, &cmd_string, session_clone.as_ref()).await {
                Ok((exit_code, stdout, stderr)) => {
                    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                    ExecResult {
                        server_name: server.name,
                        host: server.host,
                        success: exit_code == 0,
                        exit_code: Some(exit_code),
                        stdout,
                        stderr,
                        duration_ms,
                    }
                }
                Err(e) => {
                    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                    ExecResult {
                        server_name: server.name,
                        host: server.host,
                        success: false,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: e,
                        duration_ms,
                    }
                }
            }
        });

        tasks.push(handle);
    }

    // Auf alle parallelen Tasks warten
    for task in tasks {
        if let Ok(res) = task.await {
            let status_badge = if res.success {
                format!("🟢 OK (Exit 0)").bright_green()
            } else {
                format!("🔴 FEHLER (Exit {:?})", res.exit_code).bright_red()
            };

            println!(
                "  {} [{} ({})] ({}) // Latenz: {:.1}ms",
                "►".bright_cyan(),
                res.server_name.bold(),
                res.host.bright_black(),
                status_badge,
                res.duration_ms
            );

            let out_trim = res.stdout.trim();
            if !out_trim.is_empty() {
                for line in out_trim.lines() {
                    println!("    {}", line.bright_white());
                }
            }

            let err_trim = res.stderr.trim();
            if !err_trim.is_empty() {
                for line in err_trim.lines() {
                    println!("    {}", line.bright_red());
                }
            }

            println!("{}", "──────────────────────────────────────────────────────────────────────────────────────".bright_black());
        }
    }

    println!("{}", "══════════════════════════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Parallele Broadcast-Ausführung beendet.\n", "✓".bright_green());

    Ok(())
}
