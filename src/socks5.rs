use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use colored::*;
use crate::config::UserSession;
use crate::vault::ServerEntry;

/// Startet einen vollwertigen RFC 1928 SOCKS5 Proxy Server, der TCP-Verbindungen dynamisch durch die SSH2-Verbindung tunnelt.
pub async fn run_socks5_proxy(
    server: &ServerEntry,
    local_port: u16,
    session: Option<&UserSession>,
) -> Result<(), String> {
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!(
        "  {} {} {}",
        "S&B NETGATE".bright_blue().bold(),
        "//".bright_black(),
        "DYNAMISCHER SOCKS5 PROXY (RFC 1928)".bold()
    );
    println!("{}", "══════════════════════════════════════════════════════════════════".bright_black());
    println!("  {} Server:         {} ({}:{})", "►".bright_cyan(), server.name.bold(), server.host, server.port);
    println!("  {} SOCKS5 Endpunkt:{}", "►".bright_cyan(), format!("127.0.0.1:{}", local_port).bright_yellow().bold());
    println!("  {} Protokoll:      {}", "►".bright_cyan(), "RFC 1928 (IPv4, IPv6, Remote Domain Resolution)".bright_white());
    println!("  {} Transport:      {}", "►".bright_cyan(), "Natives SSH2 Direct-TCPIP Tunneling (100% Rust)".bright_magenta());

    let handle = Arc::new(crate::native_ssh::connect_and_auth(server, session).await?);
    println!("  {} SOCKS5 Proxy aktiv! Drücke [Ctrl+C] zum Beenden.", "✓".bright_green());
    println!("  {} Konfiguriere deinen Browser oder curl:", "i".bright_blue());
    println!("     curl --socks5-hostname 127.0.0.1:{} http://<interner-host>\n", local_port);

    let bind_addr: SocketAddr = format!("127.0.0.1:{}", local_port)
        .parse()
        .map_err(|e| format!("Ungültige Bind-Adresse: {}", e))?;

    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|e| format!("Konnte Port {} nicht binden: {}", local_port, e))?;

    loop {
        let (client_stream, peer_addr) = listener
            .accept()
            .await
            .map_err(|e| format!("Fehler beim Annehmen der Client-Verbindung: {}", e))?;

        let handle_clone = Arc::clone(&handle);

        tokio::spawn(async move {
            if let Err(e) = handle_socks5_client(client_stream, handle_clone, local_port).await {
                let err_str = e.to_string();
                if !err_str.contains("early eof") && !err_str.contains("Broken pipe") && !err_str.contains("reset") {
                    eprintln!("  {} SOCKS5 Weiterleitungsfehler [{}]: {}", "✗".bright_red(), peer_addr, err_str);
                }
            }
        });
    }
}

async fn handle_socks5_client(
    mut client_socket: TcpStream,
    handle: Arc<russh::client::Handle<crate::native_ssh::ClientHandler>>,
    local_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Greeting: [VER, NMETHODS, METHODS...]
    let mut ver_nmethods = [0u8; 2];
    client_socket.read_exact(&mut ver_nmethods).await?;
    if ver_nmethods[0] != 5 {
        return Err("Ungültiges Protokoll (kein SOCKS5)".into());
    }

    let nmethods = ver_nmethods[1] as usize;
    let mut methods = vec![0u8; nmethods];
    client_socket.read_exact(&mut methods).await?;

    if !methods.contains(&0x00) {
        let _ = client_socket.write_all(&[0x05, 0xFF]).await;
        return Err("Client verlangt nicht-unterstützte Authentifizierung".into());
    }

    // Server wählt: 0x00 (NO AUTHENTICATION REQUIRED)
    client_socket.write_all(&[0x05, 0x00]).await?;

    // 2. Request: [VER, CMD, RSV, ATYP, DST.ADDR, DST.PORT]
    let mut req_header = [0u8; 4];
    client_socket.read_exact(&mut req_header).await?;

    if req_header[0] != 5 {
        return Err("Ungültige SOCKS-Version in Anfrage".into());
    }

    if req_header[1] != 1 {
        let _ = client_socket.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await;
        return Err("Nur SOCKS5 CONNECT (TCP) wird unterstützt".into());
    }

    let atyp = req_header[3];
    let dest_host = match atyp {
        1 => {
            let mut ip = [0u8; 4];
            client_socket.read_exact(&mut ip).await?;
            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
        }
        3 => {
            let mut len_buf = [0u8; 1];
            client_socket.read_exact(&mut len_buf).await?;
            let mut domain_buf = vec![0u8; len_buf[0] as usize];
            client_socket.read_exact(&mut domain_buf).await?;
            String::from_utf8_lossy(&domain_buf).to_string()
        }
        4 => {
            let mut ip = [0u8; 16];
            client_socket.read_exact(&mut ip).await?;
            let ipv6 = std::net::Ipv6Addr::from(ip);
            ipv6.to_string()
        }
        _ => {
            let _ = client_socket.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await;
            return Err("Nicht unterstützter SOCKS5 Adresstyp".into());
        }
    };

    let mut port_buf = [0u8; 2];
    client_socket.read_exact(&mut port_buf).await?;
    let dest_port = u16::from_be_bytes(port_buf);

    // 3. Kanal über SSH öffnen
    match handle
        .channel_open_direct_tcpip(dest_host.clone(), dest_port as u32, "127.0.0.1", local_port as u32)
        .await
    {
        Ok(channel) => {
            let reply = [0x05, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
            client_socket.write_all(&reply).await?;

            let mut channel_stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut client_socket, &mut channel_stream).await;
            Ok(())
        }
        Err(e) => {
            let reply = [0x05, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
            let _ = client_socket.write_all(&reply).await;
            Err(format!("Ziel {}:{} über SSH nicht erreichbar: {}", dest_host, dest_port, e).into())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_socks5_constants() {
        // SOCKS Version 5
        let ver = 0x05u8;
        assert_eq!(ver, 5);

        // Success response template
        let reply = [0x05, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(reply[0], 5);
        assert_eq!(reply[1], 0); // Success
    }
}
