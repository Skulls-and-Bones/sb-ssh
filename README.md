# S&B NetGate (`sb-ssh`)
> **Tactical Zero-Trust SSH CLI & Infrastructure Gateway**  
> Autarke Single-Binary CLI & TUI in 100% Rust (russh, Tokio, Ratatui).  
> *Sicherheitsmarke: Skulls & Bones ([skulls-and-bones.org](https://www.skulls-and-bones.org))*

[![Rust](https://img.shields.io/badge/Language-Rust_2021-orange.svg?style=flat-square)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)
[![Free & Open Source](https://img.shields.io/badge/Cost-100%25_Free_%26_Open--Source-brightgreen.svg?style=flat-square)](#)
[![Compliance](https://img.shields.io/badge/BSI_IT--Grundschutz-OPS.1.1.4_%26_DER.1-brightgreen.svg?style=flat-square)](https://www.bsi.bund.de)
[![Zero-Dependency](https://img.shields.io/badge/Dependencies-100%25_Pure--Rust-blueviolet.svg?style=flat-square)](#)

---

## ⚡ Das Problem mit herkömmlichem SSH

* **Key-Sprawl & Gestohlene Laptops:** Entwickler-Laptops tragen oft Dutzende ungeschützte Private-Keys (`id_ed25519`), die jahrelang auf Servern in `~/.ssh/authorized_keys` hinterlegt bleiben.
* **Keine Revocation:** Scheidet ein Mitarbeiter aus, müssen Keys manuell auf jedem Host gelöscht werden.
* **Mangelndes Audit:** System-Logs zeigen nur "User root logged in", aber nicht *wer* hinter dem SSH-Key steckte.
* **Abhängigkeiten:** Bestehende Tools sind oft schwerfällige Wrapper um externe Binaries (`ssh.exe`, `scp.exe`, OpenSSL).

---

## 🛡️ Die Lösung: S&B NetGate

**S&B NetGate (`sb-ssh`)** eliminiert statische SSH-Schlüssel vollständig. Anstelle unübersichtlicher Key-Dateien authentifiziert sich der Nutzer über seinen zentralen Identity-Provider (OAuth2 PKCE / OIDC). 

Eine integrierte, kryptographische **Certificate Authority (CA)** stellt in Millisekunden ein **kurzlebiges OpenSSH-Zertifikat** (z. B. 8 Stunden Gültigkeit) aus. Server benötigen lediglich **einen einzigen Eintrag** in `/etc/ssh/sshd_config` (`TrustedUserCAKeys /etc/ssh/sb_ca.pub`) und akzeptieren fortan jeden autorisierten Zugriff passwortlos und schlüsselfrei.

---

## 🚀 Key Features

| Feature | Beschreibung |
| :--- | :--- |
| **100% Pure-Rust Transport** | Nativer SSH2-Client (`russh` v0.63 + `ring`). Null externe Abhängigkeiten zu `ssh.exe`, `scp.exe` oder OpenSSL. |
| **Kurzlebige Zertifikate** | Just-in-Time Ed25519 OpenSSH User-Zertifikate (`ssh-ed25519-cert-v01@openssh.com`) mit automatischer Gültigkeitsbeschränkung. |
| **Aufgeräumtes TUI Dashboard** | Minimalistisches Ratatui-Interface mit Vollbild-Serverliste, Live-Latenz-Ping, Suchfilter und 1-Klick Connect. |
| **Dynamischer SOCKS5-Proxy** | Integrierter RFC 1928 SOCKS5-Server (`sb-ssh proxy` / `-D 1080`) für Browser und interne Netze mit DNS-Leak Protection. |
| **BSI Session-Audit** | Revisionssicheres JSONL-Log nach BSI IT-Grundschutz (OPS.1.1.4 & DER.1) in `~/.sb-ssh/audit/sessions.jsonl`. |
| **Asciinema Recording & Replay** | Vollständige Aufzeichnung von Terminal-Sitzungen im `.cast`-Format (`-r`) mit nativem Player (`sb-ssh replay`). |
| **In-Memory Bastion / ProxyJump** | Kaskadierter SSH-over-SSH Tunnelbau im Speicher (`channel_open_direct_tcpip`) ohne lokale Ports oder Subprozesse. |
| **Natives SFTP & Remote-Exec** | Upload (`push`), Download (`pull`) via SFTP und parallele Broadcast-Ausführung (`exec`) über Server-Tags. |
| **Drop-in OpenSSH Alias** | Vollständig kompatibel mit `GIT_SSH_COMMAND`, `rsync`, `vscode` und Skripten durch automatische Flag-Toleranz. |
| **Shell-Completions** | Autovervollständigung für PowerShell, Bash, Zsh, Fish und Elvish via `clap_complete`. |

---

## 📦 Installation & Schnellstart

### 1. Aus Quellcode installieren (Cargo)
```bash
# Repository klonen
git clone https://github.com/Skulls-and-Bones/sb-ssh.git
cd sb-ssh

# Global installieren
cargo install --path . --force
```

### 2. Optional: Als primären SSH-Client setzen (Windows Drop-in)
```powershell
# sb-ssh als Standard-ssh.exe registrieren:
Copy-Item "$env:USERPROFILE\.cargo\bin\sb-ssh.exe" "$env:USERPROFILE\.cargo\bin\ssh.exe" -Force
```

### 3. Shell-Autovervollständigung aktivieren
```powershell
# In PowerShell ($PROFILE):
sb-ssh completions powershell | Out-String | Invoke-Expression

# In Zsh (~/.zshrc):
sb-ssh completions zsh > ~/.zfunc/_sb-ssh
```

---

## 🔄 Migrations-Tutorial: In 5 Minuten von OpenSSH zu S&B NetGate

Die Migration von herkömmlichem OpenSSH zu S&B NetGate erfolgt **ohne Ausfallzeit**, **100% abwärtskompatibel** und in 5 einfachen Schritten:

### Schritt 1: Automatische Übernahme bestehender Hosts (`~/.ssh/config`)
Du musst deine Server nicht neu anlegen. Beim ersten Start liest `sb-ssh` automatisch deine bestehende `~/.ssh/config` ein:
* Hostnamen, IP-Adressen, Ports und SSH-Benutzer werden nahtlos in den lokalen Tresor übernommen.
* Bestehende Bastion-Kaskadierungen (`ProxyJump`) werden automatisch erkannt und nativ gemappt.
* Deine bestehenden Private Keys (`id_ed25519`, `id_rsa`) funktionieren übergangsweise sofort weiter.

```bash
# Zeigt alle automatisch importierten Hosts mit Live-Ping:
sb-ssh list
```

### Schritt 2: Drop-in Ersatz für bestehende Tools (`git`, `rsync`, VS Code)
Da `sb-ssh` alle standardmäßigen OpenSSH-Flags (`-p`, `-i`, `-o`, `-T`, `-v`, `-q`) unterstützt, kannst du OpenSSH systemweit transparent ersetzen:

```powershell
# Windows: Als primäre ssh.exe im User-Pfad registrieren:
Copy-Item "$env:USERPROFILE\.cargo\bin\sb-ssh.exe" "$env:USERPROFILE\.cargo\bin\ssh.exe" -Force

# Linux / macOS: Symlink anlegen:
sudo ln -sf ~/.cargo/bin/sb-ssh /usr/local/bin/ssh
```

**Git-Transfers tunneln:**
```powershell
$env:GIT_SSH_COMMAND = "sb-ssh"
git pull origin main
```
Ab diesem Zeitpunkt laufen alle deine gewohnten Workflows (inklusive VS Code Remote-SSH) transparent über die Pure-Rust Engine von NetGate.

### Schritt 3: Server auf Zero-Key umstellen (Die CA autorisieren)
Das größte Sicherheitsrisiko herkömmlicher Setups sind statische Keys auf Zielservern. Um einen Server schlüsselfrei zu machen:

1. Führe den Einrichtungshelfer auf deinem lokalen Rechner aus:
   ```bash
   sb-ssh server-init
   ```
2. Führe die ausgegebenen 2 Befehle einmalig als Root auf deinem Zielserver aus:
   ```bash
   # CA Public Key hinterlegen:
   echo 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5... S&B_NetGate_CA' | sudo tee /etc/ssh/sb_ca.pub

   # In /etc/ssh/sshd_config einbinden und sshd neuladen:
   echo 'TrustedUserCAKeys /etc/ssh/sb_ca.pub' | sudo tee -a /etc/ssh/sshd_config
   sudo sshd -t && sudo systemctl reload ssh
   ```
**Ergebnis:** Der Server vertraut ab sofort allen von deiner CA signierten 8h-Zertifikaten. Du musst nie wieder SSH-Keys auf diesen Server kopieren oder bei Personalwechseln mühsam manuell bereinigen.

### Schritt 4: Zentrale Identität verknüpfen (OAuth2 / OIDC)
Melde dich über deinen Identity-Provider (GitHub, Google, Keycloak, etc.) an:
```bash
sb-ssh login
```
Der Browser öffnet sich, verifiziert deine Identität und stellt in Millisekunden ein ephemeres OpenSSH-Zertifikat mit 8 Stunden Gültigkeit aus.
Mit `sb-ssh status` kannst du jederzeit die Restlaufzeit deines Zertifikats einsehen.

### Schritt 5: BSI Session-Audit & Recording nutzen
Aktiviere für sensible Infrastruktur-Arbeiten die automatische Aufzeichnung:
```bash
# Sitzung verbinden und als Asciinema v2 (.cast) revisionssicher mitloggen:
sb-ssh connect prod-server --record

# Nach der Sitzung das BSI IT-Grundschutz Protokoll prüfen:
sb-ssh audit

# Die Sitzung für Incident-Reviews oder Team-Demos offline abspielen:
sb-ssh replay <session_id>
```

---

### 📊 Vergleich: OpenSSH vs. S&B NetGate

| Kriterium | Herkömmliches OpenSSH | S&B NetGate (`sb-ssh`) |
| :--- | :--- | :--- |
| **Schlüssel-Management** | Statische `id_ed25519` Keys in `authorized_keys` | 100% Zero-Key: 8h ephemere Zertifikate via OAuth2 |
| **Offboarding von Mitarbeitern**| Manuelles Key-Löschen auf jedem einzelnen Server | Sofortiger Entzug über IdP (Zertifikat verfällt in <8h) |
| **Bedienkomfort** | Reine Text-CLI | Minimalistisches Vollbild-TUI mit Live-Ping & Suche |
| **Session-Audit** | Nur unvollständige Syslog-Einträge | Revisionssichere JSONL-Trails & Asciinema Replay |
| **SOCKS5 & Bastions** | Externe Subprozesse & komplexe Parameter | 1-Klick SOCKS5 (`-D 1080`) & In-Memory ProxyJump |
| **Dateitransfer** | Auf externe `scp.exe` / OpenSSL angewiesen | Autarkes Pure-Rust SFTP (`push` & `pull`) |
| **Abhängigkeiten** | C-Bibliotheken / externe Binaries | 100% autarke Rust Standalone-Binary |

---

## 📖 Anwendungsbeispiele & CLI Cheatsheet

### 🖥️ Interaktive Terminal-Oberfläche (TUI)
```bash
sb-ssh
# oder explizit:
sb-ssh tui
```
* `↑ / ↓`: Server auswählen
* `[Enter]`: Sofortige native Verbindung
* `[+]`: Server zum Tresor hinzufügen
* `[D]`: Server mit Sicherheitsdialog löschen
* `[R]`: Latenzen neu messen
* `[?]`: Hilfe & Tastenübersicht einblenden
* `[Q]`: Beenden

---

### 🌐 Dynamischer SOCKS5-Proxy (`-D`)
Verbindet einen Browser oder Tools direkt mit dem internen Netzwerk des Remote-Servers:
```bash
# Expliziter Proxy:
sb-ssh proxy prod-server --port 1080

# Oder im gewohnten OpenSSH-Stil:
ssh -D 1080 prod-server
```
Nutzung z. B. mit `curl`:
```bash
curl --socks5-hostname 127.0.0.1:1080 http://internal-db:5432
```

---

### 🛡️ Revisionssicheres Session-Audit & Recording
```bash
# Sitzung verbinden und aufzeichnen:
sb-ssh connect prod-server --record
# oder kurz:
ssh prod-server -r

# BSI IT-Grundschutz Audit-Protokoll einsehen:
sb-ssh audit

# Audit als unformatierte JSON-Lines für SIEM (Splunk, Elastic):
sb-ssh audit --json

# Aufgezeichnete Sitzung im Terminal abspielen:
sb-ssh replay sb-20260907-182353-04a1
```

---

### 🌉 Bastion & Jump-Host (ProxyJump)
```bash
# Server mit vorgeschaltetem Bastion-Host hinzufügen:
sb-ssh add db-internal 10.0.1.5 --user ubuntu --jump bastion-dmz

# Verbinden (baut SSH-over-SSH Streaming direkt im RAM auf):
sb-ssh connect db-internal
```

---

### 📂 SFTP Dateitransfer & Remote-Exec
```bash
# Datei per nativem SFTP hochladen:
sb-ssh push prod-server ./dist/bundle.tar.gz /var/www/

# Datei herunterladen:
sb-ssh pull prod-server /var/log/nginx/access.log ./access.log

# Befehl parallel auf allen Produktionsservern ausführen:
sb-ssh exec "uptime" --tag prod
```

---

### 🤖 Drop-in Kompatibilität (`git`, CI/CD, Scripts)
Da `sb-ssh` standardmäßige OpenSSH-Flags (`-p`, `-i`, `-o`, `-T`, etc.) versteht und bei Befehlsübergabe automatisch in den Non-Interactive Streaming-Modus wechselt:
```powershell
# Git-Transfers über S&B NetGate tunneln:
$env:GIT_SSH_COMMAND = "sb-ssh"
git clone git@github.com:Skulls-and-Bones/sb-ssh.git

# Batch-Kommando ausführen (gibt sauberes stdout & Exit-Code zurück):
ssh prod-server "df -h /"
```

---

### 🔧 1-Klick Server CA-Einrichtung
Damit ein neuer Server sofort Zertifikate von `sb-ssh` akzeptiert:
```bash
sb-ssh server-init
```
Folge den zwei angezeigten Befehlen (Public Key hinterlegen & `sshd_config` anpassen). Danach benötigt kein Benutzer jemals wieder einen statischen Key auf diesem Server.

---

## 📋 Vollständige CLI-Befehlsreferenz

| Befehl | Argumente | Beschreibung |
| :--- | :--- | :--- |
| `sb-ssh` | `[target] [args...]` | Startet die TUI oder verbindet direkt (Drop-in `ssh`) |
| `sb-ssh tui` | — | Öffnet das grafische Vollbild-Terminal-Dashboard |
| `sb-ssh menu` | — | Öffnet das zeilenbasierte Schnellmenü mit Latenzen |
| `sb-ssh connect` | `<target> [-r]` | Verbindet nativ (optional mit Aufzeichnung) |
| `sb-ssh proxy` | `<target> [-p port]` | Startet lokalen SOCKS5-Proxy (Standard: 1080) |
| `sb-ssh tunnel` | `<target> <local:rem>`| Öffnet einen TCP Port-Forwarding-Tunnel |
| `sb-ssh info` | `<target>` | Blitzschnelle Health-Probe (CPU, RAM, Disk, Uptime) |
| `sb-ssh push` | `<target> <loc> [rem]`| Lädt Dateien via nativem SFTP hoch |
| `sb-ssh pull` | `<target> <rem> [loc]`| Lädt Dateien via nativem SFTP herunter |
| `sb-ssh exec` | `"<cmd>" [-t target]` | Multi-Server Broadcast-Ausführung |
| `sb-ssh audit` | `[-l limit] [--json]` | Zeigt BSI IT-Grundschutz Sitzungsprotokolle |
| `sb-ssh replay` | `<session/file>` | Spielt eine Terminal-Aufzeichnung (.cast) ab |
| `sb-ssh login` | `[--dev <user>]` | Startet OAuth2 PKCE Browser-Login / Dev-Login |
| `sb-ssh logout` | — | Beendet die Sitzung & löscht lokalen Token |
| `sb-ssh status` | — | Zeigt Auth-Status, Ablaufzeit & CA-Public-Key |
| `sb-ssh add` | `<name> <host> [-j jump]` | Fügt neuen Server zum Tresor hinzu |
| `sb-ssh list` | `[--tag <tag>]` | Listet alle Server mit Live-Latenz auf |
| `sb-ssh remove` | `<name>` | Entfernt Server dauerhaft aus dem Tresor |
| `sb-ssh cert` | `[-H hours]` | Manuelle Erstellung eines signierten Zertifikats |
| `sb-ssh server-init`| — | Gibt die 1-Klick Anleitung für Zielserver aus |
| `sb-ssh completions`| `<shell>` | Generiert Shell-Completions (powershell, bash, zsh, fish) |
| `sb-ssh help` | `[topic]` | Taktisches Cheatsheet & Hilfeseite für alle Funktionen |

---

## 🔒 Lizenz & Kosten
**S&B NetGate (`sb-ssh`) ist und bleibt 100% kostenlos und quelloffen.**
* Lizenziert unter der liberalen **MIT-Lizenz**.
* Vollständig frei nutzbar für private, universitäre und kommerzielle Zwecke ohne Lizenzgebühren, ohne Registrierungspflicht und ohne versteckte Kosten.
* Entwickelt von **Skulls & Bones Lab** ([skulls-and-bones.org](https://www.skulls-and-bones.org)).

