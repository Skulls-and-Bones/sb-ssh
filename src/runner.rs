use std::time::{Duration, Instant};
use std::net::TcpStream;
use crate::config::UserSession;
use crate::vault::ServerEntry;

/// Prüft die TCP-Erreichbarkeit und Latenz des Servers
pub fn check_server_latency(host: &str, port: u16) -> Option<Duration> {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();
    if let Ok(stream) = TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| {
            use std::net::ToSocketAddrs;
            addr.to_socket_addrs().ok()
                .and_then(|mut iter| iter.next())
                .unwrap_or_else(|| "0.0.0.0:22".parse().unwrap())
        }),
        Duration::from_millis(1500),
    ) {
        let elapsed = start.elapsed();
        drop(stream);
        Some(elapsed)
    } else {
        None
    }
}

pub async fn run_ssh_session(server: &ServerEntry, session: Option<&UserSession>) -> Result<(), String> {
    crate::native_ssh::run_interactive_shell(server, session).await
}
