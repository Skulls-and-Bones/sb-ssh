use crate::config::load_session;
use crate::vault::{find_server, load_vault, ServerEntry};

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

pub async fn run_tunnel(target_query: &str, forward_arg: &str) -> Result<(), String> {
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

    crate::native_ssh::run_tunnel(
        &server,
        tunnel_cfg.local_port,
        &tunnel_cfg.remote_host,
        tunnel_cfg.remote_port,
        session.as_ref(),
    ).await
}
