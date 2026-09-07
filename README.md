# S&B NetGate (sb-ssh) — Modern OAuth-Secured SSH CLI

> **Autarker, hochperformanter SSH-Client mit OAuth2/OIDC-Authentifizierung, kurzlebigen Ed25519-Sitzungszertifikaten und taktischer Terminal-UI.**

Entwickelt in **Rust** für Ingenieure und DevOps-Teams, die herkömmliche statische SSH-Schlüssel (`authorized_keys` Sprawl) durch ein modernes, identitätsbasiertes Just-in-Time Zero-Trust-Modell ersetzen wollen.

---

## ⚡ Key Features

1. **Identity-First OAuth2 & OIDC:**
   - Einloggen im Browser über GitHub oder OIDC (Google, Authentik, Keycloak, Okta) mit MFA & Passkeys.
   - Kein manuelles Verteilen privater/öffentlicher SSH-Keys mehr nötig.

2. **Kurzlebige Ephemere OpenSSH-Zertifikate (8h Gültigkeit):**
   - Generiert bei jeder Sitzung ein temporäres Ed25519-Schlüsselpaar im Speicher.
   - Wird von der internen **S&B Certificate Authority (CA)** signiert.
   - Nach Ablauf der Sitzung verfällt das Zertifikat automatisch — gestohlene Laptops oder vergessene Keys stellen kein Sicherheitsrisiko dar.

3. **1-Klick Server-CA Setup:**
   - Server müssen nicht mehr mit einzelnen Entwickler-Keys bestückt werden.
   - Ein einziger Eintrag in `/etc/ssh/sshd_config` (`TrustedUserCAKeys /etc/ssh/sb_ca.pub`) genügt.

4. **Taktisches TUI-Dashboard (Ratatui):**
   - Interaktive Serverliste direkt im Terminal.
   - Live-Latenz / Ping-Monitor zu allen hinterlegten Nodes.
   - Filterung nach Tags (`prod`, `web`, `database`, `staging`).
   - 1-Tastendruck-Verbindung mit `[ENTER]`.

5. **Kompakte Native Binary:**
   - Einzelne autarke Executable (`3.15 MB`).
   - Keine Node.js-, Python- oder OpenSSL-Runtime erforderlich.

---

## 🚀 Installation & Build

```bash
cd tools/sb-ssh
cargo build --release
```
Die fertige Binary befindet sich unter `tools/sb-ssh/target/release/sb-ssh.exe` (bzw. `sb-ssh` unter Linux/macOS).

---

## 📖 CLI Befehle

### 1. Interaktive Terminal-UI starten (Standard)
```bash
sb-ssh
# oder explizit:
sb-ssh tui
```
- `↑ / ↓`: Server auswählen
- `[ENTER]`: Direkt verbinden
- `[L]`: OAuth-Login starten
- `[Q]`: Beenden

### 2. Authentifizierung (OAuth / Dev)
```bash
# Browser-Login via OAuth2 Loopback (GitHub / OIDC)
sb-ssh login

# Schneller Entwickler-Login ohne Browser
sb-ssh login --dev leonf

# Status der aktuellen Sitzung und CA einsehen
sb-ssh status

# Sitzung beenden
sb-ssh logout
```

### 3. Server-Tresor verwalten
```bash
# Server auflisten mit Live-Latenz
sb-ssh list

# Nach Tags filtern
sb-ssh list --tag prod

# Neuen Server hinzufügen
sb-ssh add hostinger-prod 145.223.83.235 --user leonf --port 22 --tags prod,web,nginx --desc "Production VPS"

# Server entfernen
sb-ssh remove hostinger-prod
```

### 4. Direktverbindung
```bash
# Über hinterlegten Alias
sb-ssh connect hostinger-prod

# Oder Ad-hoc mit IP
sb-ssh connect leonf@145.223.83.235
```

### 5. Server für S&B CA rüsten (Zero-Key Setup)
```bash
sb-ssh server-init
```
Gibt die S&B CA Public Key Signatur und den entsprechenden `sshd_config`-Befehl aus.
