use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub default_user: String,
    pub oauth_provider: String,
    pub github_client_id: String,
    pub session_duration_hours: u32,
    pub ca_name: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_user: "leonf".to_string(),
            oauth_provider: "github".to_string(),
            // Skulls & Bones default client id or customizable via config
            github_client_id: "Ov23liaL9b0SkullsAndBones".to_string(),
            session_duration_hours: 8,
            ca_name: "Skulls & Bones SSH CA".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub username: String,
    pub email: Option<String>,
    pub provider: String,
    pub access_token: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl UserSession {
    pub fn is_valid(&self) -> bool {
        chrono::Utc::now() < self.expires_at
    }

    pub fn time_remaining_str(&self) -> String {
        let now = chrono::Utc::now();
        if now >= self.expires_at {
            "Abgelaufen".to_string()
        } else {
            let diff = self.expires_at - now;
            let hours = diff.num_hours();
            let mins = diff.num_minutes() % 60;
            format!("{}h {}m verbleibend", hours, mins)
        }
    }
}

pub fn get_sb_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let sb_dir = home.join(".sb-ssh");
    if !sb_dir.exists() {
        let _ = std::fs::create_dir_all(&sb_dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&sb_dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    sb_dir
}

#[allow(dead_code)]
pub fn load_config() -> AppConfig {
    let config_path = get_sb_dir().join("config.toml");
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(cfg) = toml::from_str(&content) {
                return cfg;
            }
        }
    }
    let default_cfg = AppConfig::default();
    let _ = save_config(&default_cfg);
    default_cfg
}

#[allow(dead_code)]
pub fn save_config(cfg: &AppConfig) -> std::io::Result<()> {
    let config_path = get_sb_dir().join("config.toml");
    let content = toml::to_string_pretty(cfg).unwrap_or_default();
    std::fs::write(config_path, content)
}

pub fn load_session() -> Option<UserSession> {
    let session_path = get_sb_dir().join("session.json");
    if session_path.exists() {
        if let Ok(content) = std::fs::read_to_string(session_path) {
            if let Ok(sess) = serde_json::from_str::<UserSession>(&content) {
                if sess.is_valid() {
                    return Some(sess);
                }
            }
        }
    }
    None
}

pub fn save_session(session: &UserSession) -> std::io::Result<()> {
    let session_path = get_sb_dir().join("session.json");
    let content = serde_json::to_string_pretty(session).unwrap_or_default();
    std::fs::write(session_path, content)
}

pub fn clear_session() -> std::io::Result<()> {
    let session_path = get_sb_dir().join("session.json");
    if session_path.exists() {
        std::fs::remove_file(session_path)?;
    }
    Ok(())
}
