use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::config::get_sb_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub identity_file: Option<String>,
    pub description: Option<String>,
    pub last_connected: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ServerVault {
    #[serde(default)]
    pub servers: Vec<ServerEntry>,
    #[serde(default)]
    pub ignored_hosts: Vec<String>,
}

fn vault_path() -> PathBuf {
    get_sb_dir().join("servers.toml")
}

pub fn load_vault() -> ServerVault {
    let path = vault_path();
    let mut vault = if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            toml::from_str::<ServerVault>(&content).unwrap_or_default()
        } else {
            ServerVault::default()
        }
    } else {
        ServerVault::default()
    };

    // Default Seed: Hostinger VPS falls leer
    if vault.servers.is_empty() {
        vault.servers.push(ServerEntry {
            name: "hostinger-prod".to_string(),
            host: "145.223.83.235".to_string(),
            port: 22,
            user: "leonf".to_string(),
            tags: vec!["prod".to_string(), "web".to_string(), "nginx".to_string()],
            identity_file: None,
            description: Some("Hostinger Production VPS (skulls-and-bones.org)".to_string()),
            last_connected: None,
        });
        let _ = save_vault(&vault);
    }

    // Automatisch Hosts aus ~/.ssh/config synchronisieren
    let _ = sync_ssh_config_servers(&mut vault);

    vault
}

pub fn sync_ssh_config_servers(vault: &mut ServerVault) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    let config_path = home.join(".ssh").join("config");
    if !config_path.exists() {
        return false;
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let mut added_any = false;
    let mut current_host: Option<String> = None;
    let mut current_hostname: Option<String> = None;
    let mut current_user: Option<String> = None;
    let mut current_port: u16 = 22;
    let mut current_identity: Option<String> = None;

    let flush = |v: &mut ServerVault,
                 h: &Option<String>,
                 hn: &Option<String>,
                 u: &Option<String>,
                 p: u16,
                 idf: &Option<String>| -> bool {
        if let Some(name) = h {
            if name == "*" || name.contains('?') || name.contains(' ') {
                return false;
            }
            // Ignoriere Server, die der Benutzer explizit gelöscht hat
            if v.ignored_hosts.iter().any(|ih| ih.eq_ignore_ascii_case(name)) {
                return false;
            }
            let host = hn.clone().unwrap_or_else(|| name.clone());
            // Prüfe, ob Host oder Name schon im Vault existiert
            if !v.servers.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
                v.servers.push(ServerEntry {
                    name: name.clone(),
                    host,
                    port: p,
                    user: u.clone().unwrap_or_else(|| "leonf".to_string()),
                    tags: vec!["ssh-config".to_string()],
                    identity_file: idf.clone(),
                    description: Some("Importiert aus ~/.ssh/config".to_string()),
                    last_connected: None,
                });
                return true;
            }
        }
        false
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let key = parts.next().unwrap_or("").to_lowercase();
        let val = parts.collect::<Vec<&str>>().join(" ");

        match key.as_str() {
            "host" => {
                if flush(vault, &current_host, &current_hostname, &current_user, current_port, &current_identity) {
                    added_any = true;
                }
                current_host = Some(val);
                current_hostname = None;
                current_user = None;
                current_port = 22;
                current_identity = None;
            }
            "hostname" => {
                current_hostname = Some(val);
            }
            "user" => {
                current_user = Some(val);
            }
            "port" => {
                if let Ok(p) = val.parse::<u16>() {
                    current_port = p;
                }
            }
            "identityfile" => {
                current_identity = Some(val);
            }
            _ => {}
        }
    }

    if flush(vault, &current_host, &current_hostname, &current_user, current_port, &current_identity) {
        added_any = true;
    }

    if added_any {
        let _ = save_vault(vault);
    }
    added_any
}

pub fn save_vault(vault: &ServerVault) -> std::io::Result<()> {
    let path = vault_path();
    let content = toml::to_string_pretty(vault).unwrap_or_default();
    std::fs::write(path, content)
}

pub fn add_server(server: ServerEntry) -> Result<(), String> {
    let mut vault = load_vault();
    if vault.servers.iter().any(|s| s.name.eq_ignore_ascii_case(&server.name)) {
        return Err(format!("Server mit dem Namen '{}' existiert bereits!", server.name));
    }
    vault.servers.push(server);
    save_vault(&vault).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn remove_server(name: &str) -> Result<bool, String> {
    let mut vault = load_vault();
    let len_before = vault.servers.len();
    vault.servers.retain(|s| !s.name.eq_ignore_ascii_case(name));
    if vault.servers.len() < len_before {
        if !vault.ignored_hosts.iter().any(|h| h.eq_ignore_ascii_case(name)) {
            vault.ignored_hosts.push(name.to_string());
        }
        save_vault(&vault).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn update_server(old_name: &str, updated: ServerEntry) -> Result<(), String> {
    let mut vault = load_vault();
    if let Some(pos) = vault.servers.iter().position(|s| s.name.eq_ignore_ascii_case(old_name)) {
        vault.servers[pos] = updated;
        save_vault(&vault).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err(format!("Server '{}' nicht gefunden!", old_name))
    }
}

pub fn find_server<'a>(vault: &'a ServerVault, query: &str) -> Option<&'a ServerEntry> {
    vault.servers.iter().find(|s| {
        s.name.eq_ignore_ascii_case(query) || s.host.eq_ignore_ascii_case(query)
    })
}

pub fn update_last_connected(name: &str) {
    let mut vault = load_vault();
    if let Some(srv) = vault.servers.iter_mut().find(|s| s.name.eq_ignore_ascii_case(name)) {
        srv.last_connected = Some(chrono::Utc::now());
        let _ = save_vault(&vault);
    }
}
