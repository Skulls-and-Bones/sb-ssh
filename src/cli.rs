use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "sb-ssh",
    author = "Skulls & Bones Lab <dev@skulls-and-bones.org>",
    version = "0.1.0",
    about = "S&B NetGate — Modern OAuth-Secured SSH CLI with Ephemeral Certificates & TUI",
    long_about = "S&B NetGate (sb-ssh) revolutioniert den SSH-Zugriff durch OAuth2/OIDC-Authentifizierung, kurzlebige Ed25519-Sitzungszertifikate und eine interaktive Terminal-UI. 100% autarke Rust-Binary ohne statischen Key-Sprawl.",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Optionales Direkt-Ziel (z.B. 'prod-server' oder 'user@host') für Drop-in SSH-Kompatibilität
    pub target: Option<String>,

    /// Auszuführende Remote-Befehle (z.B. bei non-interactive Aufrufen wie git oder rsync)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command_args: Vec<String>,

    /// Session im Asciinema v2 (.cast) Format aufzeichnen
    #[arg(short, long)]
    pub record: bool,

    /// SSH Port (-p 22)
    #[arg(short = 'p')]
    pub port: Option<u16>,

    /// SSH Login-Benutzer (-l user)
    #[arg(short = 'l')]
    pub login_user: Option<String>,

    /// Identity-Datei / Private Key (-i id_ed25519)
    #[arg(short = 'i')]
    pub identity_file: Option<String>,

    /// OpenSSH Optionen (-o Option=Value)
    #[arg(short = 'o', action = clap::ArgAction::Append)]
    pub options: Vec<String>,

    /// Dynamisches SOCKS5 Port-Forwarding (-D [bind:]port)
    #[arg(short = 'D')]
    pub dynamic_forward: Option<String>,

    /// Lokales Port-Forwarding (-L local:remote)
    #[arg(short = 'L')]
    pub local_forward: Option<String>,

    /// Disable PTY allocation (-T)
    #[arg(short = 'T')]
    pub disable_pty: bool,

    /// Force PTY allocation (-t)
    #[arg(short = 't')]
    pub force_pty: bool,

    /// Do not execute remote command / forward only (-N)
    #[arg(short = 'N')]
    pub no_exec: bool,

    /// Quiet mode (-q)
    #[arg(short = 'q')]
    pub quiet: bool,

    /// Verbose mode (-v)
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Compression (-C)
    #[arg(short = 'C')]
    pub compression: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Startet die interaktive Terminal-Oberfläche (Standard bei Aufruf ohne Parameter)
    Tui,

    /// Startet das klassische zeilenbasierte Konsolen-Auswahlmenü
    Menu,

    /// Verbindet direkt mit einem hinterlegten Server oder einer IP
    Connect {
        /// Servername aus dem Vault oder Zieladresse (z.B. 'prod-server' oder 'root@192.168.1.10')
        target: String,
        /// Optionaler SSH-Benutzer
        #[arg(short, long)]
        user: Option<String>,
        /// Optionaler SSH-Port
        #[arg(short, long)]
        port: Option<u16>,
        /// Terminal-Sitzung revisionssicher aufzeichnen (.cast Format)
        #[arg(short, long)]
        record: bool,
    },

    /// Zeigt das BSI IT-Grundschutz konforme Session-Audit-Protokoll
    Audit {
        /// Maximale Anzahl anzuzeigender Einträge (Standard: 20)
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
        /// Ausgabe als unformatierte JSON-Lines (für SIEM / Log-Forwarding)
        #[arg(long)]
        json: bool,
    },

    /// Spielt eine aufgezeichnete Terminal-Sitzung ab (.cast Datei oder Session-ID)
    Replay {
        /// Dateipfad oder Session-ID der Aufzeichnung
        target: String,
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
        /// Hostname oder IP-Adresse (z.B. '192.168.1.10')
        host: String,
        /// SSH-Benutzername (Standard: 'root')
        #[arg(short, long, default_value = "root")]
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
        /// Optionaler Bastion- / Jump-Host aus dem Tresor
        #[arg(short, long)]
        jump: Option<String>,
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
        /// Servername aus dem Tresor oder Zieladresse (z.B. 'prod-server')
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
        /// Zielserver aus dem Tresor oder Adresse (z.B. 'prod-server')
        target: String,
        /// Pfad zur lokalen Quelldatei
        local_path: String,
        /// Optionaler entfernter Zielpfad (Standard: './<dateiname>')
        remote_path: Option<String>,
    },

    /// Lädt eine entfernte Datei vom Zielserver herunter (Pull)
    Pull {
        /// Quellserver aus dem Tresor oder Adresse (z.B. 'prod-server')
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

    /// Startet einen dynamischen SOCKS5-Proxy über den Zielserver (RFC 1928)
    Proxy {
        /// Servername aus dem Tresor oder Zieladresse
        target: String,
        /// Lokaler SOCKS5 Listening-Port (Standard: 1080)
        #[arg(short, long, default_value_t = 1080)]
        port: u16,
    },

    /// Generiert Shell-Autovervollständigungsskripte (bash, zsh, fish, powershell, elvish)
    Completions {
        /// Ziel-Shell für die Autovervollständigung
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Zeigt eine ausführliche taktische Befehlsübersicht & Cheatsheet
    Help {
        /// Optionales Thema oder Subcommand (z.B. 'socks5', 'audit', 'tunnel', 'certs', 'sftp')
        topic: Option<String>,
    },
}
