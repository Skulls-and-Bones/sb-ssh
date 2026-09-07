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
}

fn vault_path() -> PathBuf {
    get_sb_dir().join("servers.toml")
}

pub fn load_vault() -> ServerVault {
    let path = vault_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(vault) = toml::from_str::<ServerVault>(&content) {
                return vault;
            }
        }
    }

    // Default Seed: Unser Hostinger VPS Server!
    let mut default_vault = ServerVault::default();
    default_vault.servers.push(ServerEntry {
        name: "hostinger-prod".to_string(),
        host: "145.223.83.235".to_string(),
        port: 22,
        user: "leonf".to_string(),
        tags: vec!["prod".to_string(), "web".to_string(), "nginx".to_string()],
        identity_file: None,
        description: Some("Hostinger Production VPS (skulls-and-bones.org)".to_string()),
        last_connected: None,
    });
    let _ = save_vault(&default_vault);
    default_vault
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
        save_vault(&vault).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
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
