# S&B NetGate (`sb-ssh`) — Roadmap & TODOs
> **Tactical Zero-Trust SSH CLI & Infrastructure Gateway**  
> Autarke Single-Binary Desktop- & Terminal-Applikation (100% Rust / Tokio / Ratatui)  
> *Sicherheitsmarke: Skulls & Bones ([skulls-and-bones.org](https://www.skulls-and-bones.org))*

---

## 🎯 Produkt-Vision
**S&B NetGate (`sb-ssh`)** eliminiert statische SSH-Schlüssel (`~/.ssh/authorized_keys`), unübersichtlichen Key-Sprawl und ungeschützte Server-Zugänge vollständig. 

Anstelle jahrelang gültiger Private-Keys auf Entwickler-Laptops authentifiziert sich der Administrator über seinen zentralen Identity-Provider (OAuth2 PKCE / OIDC). Bei jedem Verbindungsaufbau stellt eine interne, kryptographische Certificate Authority (CA) in Millisekunden ein **kurzlebiges OpenSSH-Zertifikat** (z. B. 8 Stunden Gültigkeit) aus. 

Zusammen mit integrierter Live-Latenzmessung, interaktiver TUI, 1-Click Server-Management und Zero-Configuration Tunnels bietet `sb-ssh` maximalen Bedienkomfort bei kompromissloser Sicherheit nach **BSI IT-Grundschutz**.

---

## 📋 Entwicklungs-Phasen & Feature-Status

### Phase 1: Fundament, Kryptographie & OAuth2 (Status: 🟢 Erledigt)
- [x] **Projekt-Scaffolding:** 100% nativer Rust 2021 Workspace (`tools/sb-ssh`) ohne externe Laufzeitabhängigkeiten (kein Python, kein Node.js).
- [x] **OpenSSH Certificate Authority (CA):**
  - Integrierte Ed25519-CA (`~/.sb-ssh/ca_key` und `ca_key.pub`).
  - Automatisches Erzeugen und Signieren echter OpenSSH-User-Zertifikate (`ssh-ed25519-cert-v01@openssh.com`).
  - Einstellbare Gültigkeitsdauer (Standard: 8 Stunden) und Principals (`user`, `root`, `admin`).
- [x] **OAuth2 / OIDC PKCE Flow:**
  - Browserbasierter Login-Flow mit dynamischem Code-Verifier / Challenge (PKCE).
  - Temporärer lokaler Loopback-Listener (`http://127.0.0.1:48200/callback`).
  - Taktische Dark-Mode HTML-Bestätigungsseite im S&B Cyber-Defense-Stil.
- [x] **Entwickler- & Offline-Login:**
  - `sb-ssh login --dev <user>` zur sofortigen Sitzungserstellung in Air-Gapped- oder Test-Umgebungen.
- [x] **System OpenSSH-Integration:**
  - Nahtlose Übergabe via `ssh -o CertificateFile=... -i ...`.
  - 100% Kompatibilität für PTY, Farb-Terminal, `vim`, `htop`, Terminal-Resizing und Signale (`Ctrl+C`).
- [x] **Kompilierung & Packaging:**
  - Schlankes Standalone-Binary (`sb-ssh.exe`, nur 3.15 MB).
  - Globale Installation im Windows-Pfad (`~/.cargo/bin/sb-ssh.exe`).
- [x] **Automatisierte Kryptographie-Tests:**
  - Unit-Tests für CA-Schlüsselerzeugung und Zertifikatsvalidierung erfolgreich (2/2 bestanden).

---

### Phase 2: Server-Vault, Sync & Interaktives Management (Status: 🟢 Erledigt)
- [x] **TOML Server-Tresor:**
  - Persistente Verwaltung in `~/.sb-ssh/servers.toml`.
  - Strukturierte Metadaten: Name, Host, Port, User, Tags, Description, Last Connected.
- [x] **Automatischer `~/.ssh/config`-Sync:**
  - Automatisches Einlesen bestehender SSH-Hosts (z. B. `development_server`).
  - Intelligente `ignored_hosts`-Liste: Verhindert, dass vom Benutzer gelöschte Server beim Neustart wiederkehren.
- [x] **Live-Latenz-Audit (TCP RTT):**
  - Paralleler TCP-Connect-Check vor Verbindungsaufbau mit Millisekunden-Anzeige und Online/Offline-Badges.
- [x] **Nummeriertes Schnell-Auswahlmenü (`sb-ssh`):**
  - Schnellauswahl mit `[1]`, `[2]`, ... oder direkter Druck auf `[Enter]` für den Standardserver.
  - Befehlsleiste: `[+] Hinzufügen`, `[-] Löschen`, `[e] Bearbeiten`, `[r] Neu messen`, `[t] TUI`, `[q] Beenden`.
- [x] **Vollbild Ratatui TUI Dashboard (`sb-ssh` / `sb-ssh tui`):**
  - **Master-Detail Split-Layout:** Linkes Flotten-Panel (Tabelle mit Status, Tags, Latenz) + Rechtes Inspektoren-Panel (Telemetrie, 15-Bar Signal-Balken, Auth-Info, Schnellaktionen).
  - **Echtzeit Live-Filter (`/`):** Schnellsuche mit instantan filternden Server-Einträgen und Cursor.
  - **Zentrierte Dialog-Modale (Overlays):**
    - Sicherheits-Löschdialog (`[D]`) mit roter Doppelumrahmung.
    - Hilfe-Dialog (`[?]` / `F1`) mit kompletter Tastaturbefehls-Referenz.
  - **1-Klick Aktionen:** `[Enter]` Verbinden, `[I]` Info, `[U]` Tunnel, `[P]` SFTP, `[X]` Exec, `[E]` Edit, `[+]` Add, `[R]` Ping, `[Q]` Beenden.
  - **Buffer-Drain Fix:** Entleeren des Konsolenpuffers beim Start gegen ungewolltes Auto-Connect.

---

#### Phase 3: 1-Click SSH Port-Forwarding & Tunnels (Status: 🟢 Erledigt)
- [x] **CLI Tunnel-Kommando:**
  - Syntax: `sb-ssh tunnel <server> <local_port>:<remote_port>`
  - Beispiel: `sb-ssh tunnel hostinger-prod 8080:80` (leitet entfernten Webserver verschlüsselt auf localhost:8080)
  - Beispiel: `sb-ssh tunnel hostinger-prod 5432:5432` (PostgreSQL direkt ansprechen)
- [x] **Live Tunnel-Status:**
  - Übersichtliche Terminal-Statusanzeige mit lokalem Endpunkt, Remote-Ziel, Ephemeral-Zertifikat und Beenden via `[STRG + C]`.
- [x] **Interaktives Menü & TUI Integration:**
  - Konsolenmenü-Option `[u] SSH-Tunnel` zur geführten Port-Weiterleitung.
  - TUI-Hotkey `[U]` zur 1-Klick-Tunnel-Initiierung für den ausgewählten Server.

---

### Phase 4: Live Remote Server-Stats & Quick-Health-Probe (Status: 🟢 Erledigt)
- [x] **Ad-hoc Remote Telemetrie (`sb-ssh info <server>`):**
  - Blitzschnelle Abfrage über non-interactive SSH in unter 1 Sekunde (Batch-Abfrage von CPU, RAM, Disk, Uptime).
- [x] **Metriken & Visualisierung:**
  - **CPU-Last:** 1m / 5m / 15m Load Average mit Einstufung (`[OPTIMAL]`, `[MODERAT]`, `[HOHE LAST]`).
  - **RAM-Auslastung:** Total, Belegt, Frei und Prozentwert mit taktischem Auslastungsbalken (`[████░░░]`).
  - **Disk Usage:** Füllstand der Root-Partition (`/`) in GB und Prozent mit freiem Speicherplatz.
  - **Uptime:** Systemlaufzeit seit dem letzten Reboot.
- [x] **Interaktives Menü & TUI Integration:**
  - Menüoption `[i] Health-Probe` im Konsolenmenü.
  - TUI-Hotkey `[I]` zur sofortigen Inspektion des markierten Servers.

---

### Phase 5: Schneller Datei-Transfer (Pure-Rust SFTP) (Status: 🟢 Erledigt)
- [x] **S&B Push / Pull:**
  - `sb-ssh push <server> <lokaler_pfad> [remote_pfad]`
  - `sb-ssh pull <server> <remote_pfad> [lokaler_pfad]`
  - Vollständige Nutzung der Vault-Namen und des ephemeren 8h-Zertifikats ohne Passwort-Prompt.
  - 100% nativer SFTP-Client (`russh-sftp`) ohne externen `scp.exe`-Subprozess.
- [x] **Transfer-Statistiken:**
  - Anzeige von Dateigröße, Übertragungsdauer und Geschwindigkeit in MB/s.
- [x] **Interaktives Menü:**
  - Menüoption `[p] Dateitransfer` für geführten Upload/Download.

---

### Phase 6: Multi-Server Broadcast Execution (`sb-ssh exec`) (Status: 🟢 Erledigt)
- [x] **Tag-basierte Parallelausführung:**
  - `sb-ssh exec "docker ps -a" --tag prod`
  - `sb-ssh exec "uptime" --target all`
- [x] **Parallele Worker:**
  - Parallele Ausführung über asynchrone Tokio-Tasks mit nativer SSH2-Kanalsteuerung.
- [x] **Aggregierte Ausgabe:**
  - Saubere tabellarische Zusammenfassung mit Servername, Host, Exit-Code-Badge, Latenz und Konsolenausgaben.
- [x] **Interaktives Menü:**
  - Menüoption `[x] Broadcast (Exec)` im Konsolen-Auswahlmenü.

---

### Phase 6.5: Vollständige Autarkie — Pure-Rust SSH2-Transport (Status: 🟢 Erledigt)
- [x] **100% Standalone Single-Binary (Beseitigung aller externen Abhängigkeiten):**
  - Vollständige Eliminierung von Abhängigkeiten zu Windows OpenSSH (`ssh.exe`, `scp.exe`).
  - Direkter SSH2-Transport über puren Rust-Stack (`russh` v0.63.2 + `ring` + `russh-sftp`).
  - Direkte Zertifikatsauthentifizierung (`authenticate_openssh_cert`) mit In-Memory-Verarbeitung ephemerer Ed25519-OpenSSH-Zertifikate.
  - Interaktives Terminal mit nativer PTY-Allokation (`xterm-256color`), Crossterm Raw-Mode und dynamischem Resizing.
  - Natives Port-Forwarding via `channel_open_direct_tcpip`.
  - Native Remote-Kommandoausführung (`channel_open_session` -> `exec`) für Probes und Broadcasts.

---

### Phase 7: Revisionssicheres Session-Audit & Recording (Status: ⚪ Konzeption)
- [ ] **Compliance nach BSI IT-Grundschutz (OPS.1.1.4 & DER.1):**
  - Lokales, unveränderbares Audit-Protokoll in `~/.sb-ssh/audit/`.
  - Protokolliert: Zeitstempel, OAuth-Identität, Ziel-IP, Session-Dauer und Zertifikats-Fingerprint.
- [ ] **Optionales Terminal Session Recording:**
  - Aufzeichnung im standardisierten Asciinema- / Cast-Format (`sb-ssh connect --record <server>`).
  - Offline-Replay zur forensischen Analyse oder für Sicherheitsaudits.

---

### Phase 8: Bastion & Jump-Host-Kaskadierung (Status: ⚪ Konzeption)
- [ ] **ProxyJump-Unterstützung:**
  - Konfigurierbarer `jump_host` im Server-Eintrag.
  - Automatisches Weiterreichen des Ephemeral-Zertifikats über DMZ-Bastion-Hosts in isolierte interne Netze.

---

### Phase 9: Team-Vault & Identity-Provider Integration (Status: ⚪ Konzeption)
- [ ] **Erweiterte Identity Provider:**
  - Fertige Vorlagen für Authentik, Keycloak, Okta und Google Workspace.
- [ ] **Rollenbasierte Autorisierung (RBAC):**
  - Mappen von OIDC-Rollen / Gruppen auf erlaubte SSH-Principals (z. B. Gruppe `ops` -> `root`, Gruppe `dev` -> `developer`).
- [ ] **Verschlüsselter Vault-Export / Import:**
  - `sb-ssh export --gpg` zum sicheren Teilen von Server-Konfigurationen im Team.

---

## 🛠️ CLI Befehls-Referenz (Aktueller Stand)

| Befehl | Kurzbeschreibung |
| :--- | :--- |
| `sb-ssh` | Öffnet das interaktive Konsolen-Auswahl- & Management-Menü |
| `sb-ssh tui` | Startet das grafische Vollbild-Terminal-Dashboard (Ratatui) |
| `sb-ssh connect <name>` | Verbindet nativ im Terminal (100% Pure-Rust PTY, 8h-Cert) |
| `sb-ssh tunnel <name> <spec>` | Öffnet einen nativen TCP-Tunnel (`local:remote` z. B. `8080:80`) |
| `sb-ssh info <name>` | Blitzschnelle Health-Probe (CPU, RAM, Disk, Uptime, Latenz) |
| `sb-ssh push <name> <local> [rem]` | Lädt Dateien nativ via SFTP auf den Server |
| `sb-ssh pull <name> <rem> [local]` | Lädt Dateien nativ via SFTP vom Server herunter |
| `sb-ssh exec "<cmd>" [-t target]` | Führt Befehle parallel auf Zielservern aus (Broadcast) |
| `sb-ssh list` | Gibt die formatierten Server inkl. Live-Latenz-Ping aus |
| `sb-ssh status` | Zeigt OAuth-Status, Restlaufzeit und CA-Zertifikat |
| `sb-ssh login` | Startet den OAuth2 PKCE Browser-Login |
| `sb-ssh login --dev <user>` | Lokaler Entwickler-Login ohne externen IdP |
| `sb-ssh logout` | Löscht den lokalen OAuth-Token und beendet die Sitzung |
| `sb-ssh add <name> <ip>` | Fügt einen neuen Server zum Tresor hinzu |
| `sb-ssh remove <name>` | Entfernt einen Server dauerhaft aus dem Tresor |
| `sb-ssh cert -H <hours>` | Manuelle Erstellung eines signierten OpenSSH-Zertifikats |
| `sb-ssh server-init` | Gibt die 1-Zeilen-Kommandos zur CA-Einrichtung auf Zielservern aus |
