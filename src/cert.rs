use std::path::PathBuf;
use russh::keys::ssh_key::{
    certificate::{Builder, CertType},
    Algorithm, LineEnding, PrivateKey,
};
use crate::config::get_sb_dir;

#[allow(dead_code)]
pub struct EphemeralCertBundle {
    pub private_key_path: PathBuf,
    pub cert_path: PathBuf,
    pub public_key_openssh: String,
    pub cert_openssh: String,
    pub valid_until: chrono::DateTime<chrono::Utc>,
}

fn ca_key_path() -> PathBuf {
    get_sb_dir().join("ca_key")
}

pub fn ca_pub_path() -> PathBuf {
    get_sb_dir().join("ca_key.pub")
}

pub fn get_or_create_ca_key() -> Result<PrivateKey, String> {
    let key_path = ca_key_path();
    let pub_path = ca_pub_path();

    if key_path.exists() {
        let content = std::fs::read_to_string(&key_path)
            .map_err(|e| format!("Fehler beim Lesen des CA-Keys: {}", e))?;
        return PrivateKey::from_openssh(&content)
            .map_err(|e| format!("Ungültiges OpenSSH CA-Key Format: {}", e));
    }

    // Neu generieren
    let mut rng = rand::rng();
    let ca_key = PrivateKey::random(&mut rng, Algorithm::Ed25519)
        .map_err(|e| format!("Fehler beim Generieren des CA-Keys: {}", e))?;

    let priv_openssh = ca_key.to_openssh(LineEnding::LF)
        .map_err(|e| format!("Fehler beim Serialisieren des CA-Keys: {}", e))?;
    let pub_openssh = ca_key.public_key().to_openssh()
        .map_err(|e| format!("Fehler beim Serialisieren des CA-Public-Keys: {}", e))?;

    std::fs::write(&key_path, priv_openssh.as_bytes())
        .map_err(|e| format!("Konnte CA-Key nicht speichern: {}", e))?;
    std::fs::write(&pub_path, pub_openssh.as_bytes())
        .map_err(|e| format!("Konnte CA-Public-Key nicht speichern: {}", e))?;

    Ok(ca_key)
}

pub fn get_ca_public_key_string() -> Result<String, String> {
    let pub_path = ca_pub_path();
    if pub_path.exists() {
        return std::fs::read_to_string(pub_path)
            .map_err(|e| format!("Konnte CA-Public-Key nicht lesen: {}", e));
    }
    let ca = get_or_create_ca_key()?;
    ca.public_key().to_openssh().map_err(|e| e.to_string())
}

/// Generiert ein kurzlebiges Ed25519-Schluesselpaar und signiert es mit der CA
pub fn generate_ephemeral_certificate(
    username: &str,
    principals: &[&str],
    valid_hours: u32,
) -> Result<EphemeralCertBundle, String> {
    let ca_key = get_or_create_ca_key()?;

    // 1. Temporärer ephemerer User-Key
    let mut rng = rand::rng();
    let user_priv = PrivateKey::random(&mut rng, Algorithm::Ed25519)
        .map_err(|e| format!("Konnte ephemeren Key nicht erzeugen: {}", e))?;
    let user_pub = user_priv.public_key().clone();

    // 2. Gültigkeitszeitraum berechnen
    let now = std::time::SystemTime::now();
    let unix_now = now.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let unix_valid_until = unix_now + (valid_hours as u64 * 3600);
    let chrono_valid_until = chrono::Utc::now() + chrono::Duration::hours(valid_hours as i64);

    let key_id = format!("sb-ssh-session-{}-{}", username, unix_now);

    // 3. OpenSSH User-Zertifikat bauen
    let mut builder = Builder::new_with_random_nonce(&mut rng, &user_pub, unix_now, unix_valid_until)
        .map_err(|e| format!("Zertifikats-Builder Fehler: {}", e))?;

    builder.cert_type(CertType::User)
        .map_err(|e| format!("Konnte CertType nicht setzen: {}", e))?;
    builder.key_id(&key_id)
        .map_err(|e| format!("Konnte KeyId nicht setzen: {}", e))?;

    for p in principals {
        builder.valid_principal(*p)
            .map_err(|e| format!("Ungültiger Principal '{}': {}", p, e))?;
    }

    // Standard OpenSSH Erweiterungen
    builder.extension("permit-pty", "")
        .map_err(|e| format!("Extension-Fehler: {}", e))?;
    builder.extension("permit-user-rc", "")
        .map_err(|e| format!("Extension-Fehler: {}", e))?;
    builder.extension("permit-port-forwarding", "")
        .map_err(|e| format!("Extension-Fehler: {}", e))?;
    builder.extension("permit-agent-forwarding", "")
        .map_err(|e| format!("Extension-Fehler: {}", e))?;

    let certificate = builder.sign(&ca_key)
        .map_err(|e| format!("Konnte Zertifikat nicht signieren: {}", e))?;

    // 4. In Dateisystem fuer SSH-Client hinterlegen
    let sb_dir = get_sb_dir();
    let priv_path = sb_dir.join("id_ephemeral");
    let cert_path = sb_dir.join("id_ephemeral-cert.pub");

    let priv_str = user_priv.to_openssh(LineEnding::LF)
        .map_err(|e| format!("Konnte privaten Key nicht formatieren: {}", e))?;
    let cert_str = certificate.to_openssh()
        .map_err(|e| format!("Konnte Zertifikat nicht formatieren: {}", e))?;
    let pub_str = user_pub.to_openssh()
        .map_err(|e| format!("Konnte Public Key nicht formatieren: {}", e))?;

    std::fs::write(&priv_path, priv_str.as_bytes())
        .map_err(|e| format!("Fehler beim Schreiben von id_ephemeral: {}", e))?;
    std::fs::write(&cert_path, cert_str.as_bytes())
        .map_err(|e| format!("Fehler beim Schreiben von id_ephemeral-cert.pub: {}", e))?;

    Ok(EphemeralCertBundle {
        private_key_path: priv_path,
        cert_path,
        public_key_openssh: pub_str,
        cert_openssh: cert_str,
        valid_until: chrono_valid_until,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ca_key_creation() {
        let ca = get_or_create_ca_key().expect("CA creation should succeed");
        assert_eq!(ca.algorithm(), Algorithm::Ed25519);
    }

    #[test]
    fn test_ephemeral_cert_generation() {
        let bundle = generate_ephemeral_certificate("testuser", &["testuser", "root"], 4)
            .expect("Certificate generation should succeed");
        assert!(bundle.private_key_path.exists());
        assert!(bundle.cert_path.exists());
        assert!(bundle.cert_openssh.starts_with("ssh-ed25519-cert-v01@openssh.com"));
        assert!(bundle.valid_until > chrono::Utc::now());
    }
}
