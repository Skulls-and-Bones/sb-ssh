use std::time::Instant;
use colored::*;
use crate::config::load_session;
use crate::vault::{find_server, load_vault, ServerEntry};

#[derive(Debug, Clone)]
pub struct ServerHealthStats {
    pub server_name: String,
    pub host: String,
    pub latency_ms: f64,
    pub uptime: String,
    pub load_1m: f64,
    pub load_5m: f64,
    pub load_15m: f64,
    pub ram_total_mb: u64,
    pub ram_used_mb: u64,
    pub ram_percent: f64,
    pub disk_total_gb: f64,
    pub disk_used_gb: f64,
    pub disk_avail_gb: f64,
    pub disk_percent: f64,
}

pub async fn fetch_server_health(target_query: &str) -> Result<ServerHealthStats, String> {
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
            description: Some("Ad-hoc Health Check".to_string()),
            last_connected: None,
        }
    };

    // Non-interactive Remote Batch Telemetrie-Befehl
    let remote_cmd = "cat /proc/loadavg; echo '---SB_SEP---'; free -b; echo '---SB_SEP---'; df -k /; echo '---SB_SEP---'; uptime -p 2>/dev/null || uptime";

    let start_time = Instant::now();
    let (exit_code, stdout_str, stderr_str) = crate::native_ssh::run_remote_command(&server, remote_cmd, session.as_ref()).await?;
    let duration = start_time.elapsed();
    let latency_ms = duration.as_secs_f64() * 1000.0;

    if exit_code != 0 {
        return Err(format!("Remote-Befehl fehlgeschlagen (Exit: {}): {}", exit_code, stderr_str.trim()));
    }
    let sections: Vec<&str> = stdout_str.split("---SB_SEP---").collect();

    // 1. Loadavg
    let mut load_1m = 0.0;
    let mut load_5m = 0.0;
    let mut load_15m = 0.0;
    if let Some(sec0) = sections.get(0) {
        let parts: Vec<&str> = sec0.trim().split_whitespace().collect();
        if parts.len() >= 3 {
            load_1m = parts[0].parse().unwrap_or(0.0);
            load_5m = parts[1].parse().unwrap_or(0.0);
            load_15m = parts[2].parse().unwrap_or(0.0);
        }
    }

    // 2. RAM (free -b)
    let mut ram_total_mb = 0;
    let mut ram_used_mb = 0;
    let mut ram_percent = 0.0;
    if let Some(sec1) = sections.get(1) {
        for line in sec1.lines() {
            let line_trim = line.trim();
            if line_trim.starts_with("Mem:") {
                let parts: Vec<&str> = line_trim.split_whitespace().collect();
                if parts.len() >= 3 {
                    let total_bytes = parts[1].parse::<u64>().unwrap_or(1);
                    let used_bytes = parts[2].parse::<u64>().unwrap_or(0);
                    ram_total_mb = total_bytes / (1024 * 1024);
                    ram_used_mb = used_bytes / (1024 * 1024);
                    if total_bytes > 0 {
                        ram_percent = (used_bytes as f64 / total_bytes as f64) * 100.0;
                    }
                }
            }
        }
    }

    // 3. Disk (df -k /)
    let mut disk_total_gb = 0.0;
    let mut disk_used_gb = 0.0;
    let mut disk_avail_gb = 0.0;
    let mut disk_percent = 0.0;
    if let Some(sec2) = sections.get(2) {
        for line in sec2.lines() {
            let line_trim = line.trim();
            if line_trim.contains('/') && !line_trim.starts_with("Filesystem") {
                let parts: Vec<&str> = line_trim.split_whitespace().collect();
                if parts.len() >= 5 {
                    let total_kb = parts[1].parse::<f64>().unwrap_or(1.0);
                    let used_kb = parts[2].parse::<f64>().unwrap_or(0.0);
                    let avail_kb = parts[3].parse::<f64>().unwrap_or(0.0);
                    disk_total_gb = total_kb / (1024.0 * 1024.0);
                    disk_used_gb = used_kb / (1024.0 * 1024.0);
                    disk_avail_gb = avail_kb / (1024.0 * 1024.0);
                    if total_kb > 0.0 {
                        disk_percent = (used_kb / total_kb) * 100.0;
                    }
                }
            }
        }
    }

    // 4. Uptime
    let uptime = if let Some(sec3) = sections.get(3) {
        sec3.trim().replace("up ", "")
    } else {
        "Unbekannt".to_string()
    };

    Ok(ServerHealthStats {
        server_name: server.name,
        host: server.host,
        latency_ms,
        uptime,
        load_1m,
        load_5m,
        load_15m,
        ram_total_mb,
        ram_used_mb,
        ram_percent,
        disk_total_gb,
        disk_used_gb,
        disk_avail_gb,
        disk_percent,
    })
}

fn progress_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar_filled = "█".repeat(filled);
    let bar_empty = "░".repeat(empty);

    if percent > 85.0 {
        format!("[{}{}]", bar_filled.red(), bar_empty.bright_black())
    } else if percent > 70.0 {
        format!("[{}{}]", bar_filled.yellow(), bar_empty.bright_black())
    } else {
        format!("[{}{}]", bar_filled.green(), bar_empty.bright_black())
    }
}

pub async fn print_server_health(target: &str) {
    println!("  {} Führe Remote-Health-Probe für '{}' durch...", "►".bright_cyan(), target.bold());

    match fetch_server_health(target).await {
        Ok(stats) => {
            println!("\n{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} {} {}", "S&B NETGATE".bright_blue().bold(), "//".bright_black(), "REMOTE SERVER HEALTH PROBE".bold());
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
            println!("  {} Server:          {} ({})", "►".bright_cyan(), stats.server_name.bright_white().bold(), stats.host);
            println!("  {} Uptime:          {}", "►".bright_cyan(), stats.uptime.bright_yellow());
            println!("  {} SSH RTT Latenz:  {}", "►".bright_cyan(), format!("{:.1}ms", stats.latency_ms).bright_green());
            println!("{}", "──────────────────────────────────────────────────────────────────".bright_black());

            // CPU Load
            let load_status = if stats.load_1m > 4.0 { "HOHE LAST".red().bold() } else if stats.load_1m > 1.5 { "MODERAT".yellow() } else { "OPTIMAL".green() };
            println!(
                "  {} CPU Load:        {:.2} (1m)  {:.2} (5m)  {:.2} (15m)  [{}]",
                "►".bright_cyan(),
                stats.load_1m, stats.load_5m, stats.load_15m,
                load_status
            );

            // RAM
            let ram_used_gb = stats.ram_used_mb as f64 / 1024.0;
            let ram_total_gb = stats.ram_total_mb as f64 / 1024.0;
            let ram_bar = progress_bar(stats.ram_percent, 16);
            println!(
                "  {} Arbeitsspeicher: {} {:.1}% ({:.2} GB / {:.2} GB)",
                "►".bright_cyan(),
                ram_bar,
                stats.ram_percent,
                ram_used_gb,
                ram_total_gb
            );

            // Disk
            let disk_bar = progress_bar(stats.disk_percent, 16);
            println!(
                "  {} Festplatte (/):  {} {:.1}% ({:.1} GB / {:.1} GB) [Frei: {:.1} GB]",
                "►".bright_cyan(),
                disk_bar,
                stats.disk_percent,
                stats.disk_used_gb,
                stats.disk_total_gb,
                stats.disk_avail_gb
            );
            println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
        }
        Err(e) => {
            eprintln!("  {} Health-Probe fehlgeschlagen: {}", "✗".bright_red(), e);
        }
    }
}
