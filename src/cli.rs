use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "sb-ssh",
    author = "Skulls & Bones Lab <dev@skulls-and-bones.org>",
    version = "0.1.0",
    about = "S&B NetGate — Modern OAuth-Secured SSH CLI with Ephemeral Certificates & TUI",
    long_about = "S&B NetGate (sb-ssh) revolutioniert den SSH-Zugriff durch OAuth2/OIDC-Authentifizierung, kurzlebige Ed25519-Sitzungszertifikate und eine interaktive Terminal-UI. 100% autarke Rust-Binary ohne statischen Key-Sprawl."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Startet die interaktive Terminal-Oberfläche (Standard bei Aufruf ohne Parameter)
    Tui,

    /// Verbindet direkt mit einem hinterlegten Server oder einer IP
    Connect {
        /// Servername aus dem Vault oder Zieladresse (z.B. 'hostinger-prod' oder 'root@145.223.83.235')
        target: String,
        /// Optionaler SSH-Benutzer
        #[arg(short, long)]
        user: Option<String>,
        /// Optionaler SSH-Port
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Startet die OAuth2 / OIDC Browser-Authentifizierung
    Login {
        /// OAuth Provider (z.B. 'github', 'oidc')
        #[arg(short, long, default_value = "github")]
        provider: String,
        /// Schneller lokaler Entwickler-Login ohne Browser
        #[arg(long)]
        dev: Option<String>,
    },

    /// Beendet die aktive Sitzung und löscht den lokalen OAuth-Token
    Logout,

    /// Zeigt den aktuellen Authentifizierungs- und CA-Status
    Status,

    /// Fügt einen neuen Server zum lokalen Tresor hinzu
    Add {
        /// Eindeutiger Name / Alias (z.B. 'prod-web-01')
        name: String,
        /// Hostname oder IP-Adresse (z.B. '145.223.83.235')
        host: String,
        /// SSH-Benutzername (Standard: 'leonf')
        #[arg(short, long, default_value = "leonf")]
        user: String,
        /// SSH-Port (Standard: 22)
        #[arg(short, long, default_value_t = 22)]
        port: u16,
        /// Kommagetrennte Tags (z.B. 'prod,web,nginx')
        #[arg(short, long)]
        tags: Option<String>,
        /// Beschreibung
        #[arg(short, long)]
        desc: Option<String>,
    },

    /// Listet alle gespeicherten Server auf
    List {
        /// Optionaler Tag-Filter
        #[arg(short, long)]
        tag: Option<String>,
    },

    /// Entfernt einen Server aus dem Tresor
    Remove {
        /// Name des zu entfernenden Servers
        name: String,
    },

    /// Generiert ein neues ephemeres OpenSSH-Zertifikat
    Cert {
        /// Gültigkeitsdauer in Stunden (Standard: 8)
        #[arg(short = 'H', long, default_value_t = 8)]
        hours: u32,
    },

    /// Öffnet einen verschlüsselten SSH-Port-Forwarding-Tunnel zum Zielserver
    Tunnel {
        /// Servername aus dem Tresor oder Zieladresse (z.B. 'hostinger-prod')
        target: String,
        /// Port-Weiterleitung im Format '<local_port>:<remote_port>' (z.B. '8080:80')
        forward: String,
    },

    /// Führt eine schnelle Non-Interactive Telemetrie- und Health-Probe auf dem Server aus
    Info {
        /// Servername aus dem Tresor oder Zieladresse
        target: String,
    },

    /// Lädt eine lokale Datei oder ein Verzeichnis auf den Zielserver hoch (Push)
    Push {
        /// Zielserver aus dem Tresor oder Adresse (z.B. 'hostinger-prod')
        target: String,
        /// Pfad zur lokalen Quelldatei
        local_path: String,
        /// Optionaler entfernter Zielpfad (Standard: './<dateiname>')
        remote_path: Option<String>,
    },

    /// Lädt eine entfernte Datei vom Zielserver herunter (Pull)
    Pull {
        /// Quellserver aus dem Tresor oder Adresse (z.B. 'hostinger-prod')
        target: String,
        /// Pfad zur entfernten Datei auf dem Server
        remote_path: String,
        /// Optionaler lokaler Zielpfad (Standard: aktuelles Verzeichnis)
        local_path: Option<String>,
    },

    /// Führt einen Befehl parallel auf mehreren oder allen Servern aus (Broadcast)
    Exec {
        /// Der auszuführende Remote-Befehl (z.B. 'uptime' oder 'df -h')
        command_to_run: String,
        /// Zielserver oder 'all' (Standard: 'all')
        #[arg(short, long, default_value = "all")]
        target: String,
        /// Optionaler Tag-Filter (z.B. 'prod')
        #[arg(long)]
        tag: Option<String>,
    },

    /// Gibt die 1-Klick Anleitung & Konfiguration aus, um Ziel-Server für S&B CA zu rüsten
    ServerInit,
}
