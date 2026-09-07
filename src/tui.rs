use std::io::stdout;
use std::time::Duration;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};
use crate::config::{load_session, UserSession};
use crate::vault::{load_vault, ServerEntry, ServerVault};
use crate::runner::check_server_latency;

pub enum TuiAction {
    Connect(ServerEntry),
    TriggerLogin,
    TriggerAdd,
    TriggerTunnel(ServerEntry),
    TriggerInfo(ServerEntry),
    Quit,
}

pub struct TuiApp {
    pub vault: ServerVault,
    pub session: Option<UserSession>,
    pub selected_index: usize,
    pub table_state: TableState,
    pub latencies: Vec<Option<Duration>>,
    pub confirm_delete: bool,
}

impl TuiApp {
    pub fn new() -> Self {
        let vault = load_vault();
        let session = load_session();
        let mut table_state = TableState::default();
        if !vault.servers.is_empty() {
            table_state.select(Some(0));
        }

        let latencies = vault.servers.iter()
            .map(|s| check_server_latency(&s.host, s.port))
            .collect();

        Self {
            vault,
            session,
            selected_index: 0,
            table_state,
            latencies,
            confirm_delete: false,
        }
    }

    pub fn reload(&mut self) {
        self.vault = load_vault();
        if self.selected_index >= self.vault.servers.len() && !self.vault.servers.is_empty() {
            self.selected_index = self.vault.servers.len() - 1;
        }
        if self.vault.servers.is_empty() {
            self.table_state.select(None);
        } else {
            self.table_state.select(Some(self.selected_index));
        }
        self.refresh_latencies();
    }

    pub fn refresh_latencies(&mut self) {
        self.latencies = self.vault.servers.iter()
            .map(|s| check_server_latency(&s.host, s.port))
            .collect();
    }

    pub fn next(&mut self) {
        if self.vault.servers.is_empty() { return; }
        self.selected_index = (self.selected_index + 1) % self.vault.servers.len();
        self.table_state.select(Some(self.selected_index));
    }

    pub fn previous(&mut self) {
        if self.vault.servers.is_empty() { return; }
        if self.selected_index == 0 {
            self.selected_index = self.vault.servers.len() - 1;
        } else {
            self.selected_index -= 1;
        }
        self.table_state.select(Some(self.selected_index));
    }

    pub fn selected_server(&self) -> Option<&ServerEntry> {
        self.vault.servers.get(self.selected_index)
    }
}

pub fn run_tui() -> Result<Option<TuiAction>, String> {
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| e.to_string())?;

    // Drain all pending events from console input buffer to avoid leftover Enter keys
    while event::poll(Duration::from_millis(50)).unwrap_or(false) {
        let _ = event::read();
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let mut app = TuiApp::new();
    let res = run_loop(&mut terminal, &mut app);

    disable_raw_mode().map_err(|e| e.to_string())?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|e| e.to_string())?;
    terminal.show_cursor().map_err(|e| e.to_string())?;

    res
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut TuiApp,
) -> Result<Option<TuiAction>, String> {
    loop {
        terminal.draw(|f| ui(f, app)).map_err(|e| e.to_string())?;

        if event::poll(Duration::from_millis(200)).map_err(|e| e.to_string())? {
            if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }

                if app.confirm_delete {
                    match key.code {
                        KeyCode::Char('j') | KeyCode::Char('y') | KeyCode::Enter => {
                            if let Some(srv) = app.selected_server() {
                                let name = srv.name.clone();
                                let _ = crate::vault::remove_server(&name);
                                app.reload();
                            }
                            app.confirm_delete = false;
                        }
                        KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => {
                            app.confirm_delete = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(Some(TuiAction::Quit)),
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Char('l') => return Ok(Some(TuiAction::TriggerLogin)),
                    KeyCode::Char('r') => app.refresh_latencies(),
                    KeyCode::Char('i') => {
                        if let Some(server) = app.selected_server() {
                            return Ok(Some(TuiAction::TriggerInfo(server.clone())));
                        }
                    }
                    KeyCode::Char('u') => {
                        if let Some(server) = app.selected_server() {
                            return Ok(Some(TuiAction::TriggerTunnel(server.clone())));
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => {
                        if !app.vault.servers.is_empty() {
                            app.confirm_delete = true;
                        }
                    }
                    KeyCode::Char('a') | KeyCode::Char('+') => return Ok(Some(TuiAction::TriggerAdd)),
                    KeyCode::Enter => {
                        if let Some(server) = app.selected_server() {
                            return Ok(Some(TuiAction::Connect(server.clone())));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(4), // Header / Brand Lockup
            Constraint::Length(3), // Identity & OAuth Session Bar
            Constraint::Min(8),    // Server Table
            Constraint::Length(3), // Hotkey Footer
        ])
        .split(f.area());

    // 1. Header
    let header_text = vec![
        Line::from(vec![
            Span::styled("  SKULLS & BONES ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("// ", Style::default().fg(Color::DarkGray)),
            Span::styled("NETGATE CLI ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("v0.1.0 ", Style::default().fg(Color::Blue)),
            Span::styled("[ HIGH-PERFORMANCE ZERO-KEY-SPRAWL SSH ]", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("  Autarker SSH-Client mit OAuth2/OIDC, ephemeren Ed25519-Zertifikaten & TUI", Style::default().fg(Color::Gray)),
        ]),
    ];
    let header = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::Rgb(30, 38, 56))));
    f.render_widget(header, chunks[0]);

    // 2. Identity & OAuth Session Bar
    let session_line = if let Some(ref sess) = app.session {
        Line::from(vec![
            Span::styled("  [✓ AUTHENTIFIZIERT] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(format!("Benutzer: {} ", sess.username), Style::default().fg(Color::White)),
            Span::styled(format!("({}) ", sess.email.as_deref().unwrap_or("")), Style::default().fg(Color::DarkGray)),
            Span::styled("• ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("Provider: {} ", sess.provider), Style::default().fg(Color::Cyan)),
            Span::styled("• ", Style::default().fg(Color::DarkGray)),
            Span::styled(sess.time_remaining_str(), Style::default().fg(Color::Yellow)),
        ])
    } else {
        Line::from(vec![
            Span::styled("  [! KEINE AKTIVE OAUTH SITZUNG] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("Drücke ", Style::default().fg(Color::Gray)),
            Span::styled("[L]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" um sich via GitHub / OIDC zu authentifizieren (oder direkte Verbindung mit Key)", Style::default().fg(Color::Gray)),
        ])
    };
    let session_widget = Paragraph::new(session_line)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Rgb(30, 38, 56))));
    f.render_widget(session_widget, chunks[1]);

    // 3. Server Table
    let header_cells = ["STATUS", "SERVER NAME", "HOST : PORT", "USER", "TAGS", "LATENZ", "LETZTER ZUGRIFF"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
    let header_row = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.vault.servers.iter().enumerate().map(|(idx, s)| {
        let latency_str = match app.latencies.get(idx).copied().flatten() {
            Some(d) => format!("{:.1} ms", d.as_secs_f64() * 1000.0),
            None => "---".to_string(),
        };

        let status_cell = if app.latencies.get(idx).copied().flatten().is_some() {
            Cell::from("  ● ONLINE").style(Style::default().fg(Color::Green))
        } else {
            Cell::from("  ○ PRÜFE").style(Style::default().fg(Color::DarkGray))
        };

        let last_conn = s.last_connected
            .map(|d| d.format("%d.%m %H:%M").to_string())
            .unwrap_or_else(|| "Nie".to_string());

        let tags_str = s.tags.join(", ");

        let cells = vec![
            status_cell,
            Cell::from(s.name.clone()).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Cell::from(format!("{}:{}", s.host, s.port)).style(Style::default().fg(Color::Gray)),
            Cell::from(s.user.clone()).style(Style::default().fg(Color::Yellow)),
            Cell::from(tags_str).style(Style::default().fg(Color::Blue)),
            Cell::from(latency_str).style(Style::default().fg(Color::Cyan)),
            Cell::from(last_conn).style(Style::default().fg(Color::DarkGray)),
        ];
        Row::new(cells).height(1)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(20),
            Constraint::Length(22),
            Constraint::Length(12),
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Length(16),
        ],
    )
    .header(header_row)
    .block(Block::default().title(" [ GESPEICHERTE SERVER ] ").borders(Borders::ALL).border_style(Style::default().fg(Color::Rgb(30, 38, 56))))
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(37, 99, 235))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(" ► ");

    f.render_stateful_widget(table, chunks[2], &mut app.table_state);

    // 4. Hotkey Footer
    let footer_widget = if app.confirm_delete {
        let name = app.selected_server().map(|s| s.name.as_str()).unwrap_or("Server");
        let spans = vec![
            Span::styled(" ACHTUNG: ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" Möchtest du '{}' wirklich löschen? ", name), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(" [J / Enter] Ja ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled("   ", Style::default()),
            Span::styled(" [N / Esc] Abbrechen ", Style::default().fg(Color::Black).bg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        ];
        Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Red)))
    } else {
        let footer_spans = vec![
            Span::styled(" [ENTER] ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Verbinden  ", Style::default().fg(Color::White)),
            Span::styled(" [I] ", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(" Stats  ", Style::default().fg(Color::White)),
            Span::styled(" [U] ", Style::default().fg(Color::Black).bg(Color::Blue).add_modifier(Modifier::BOLD)),
            Span::styled(" Tunnel  ", Style::default().fg(Color::White)),
            Span::styled(" [D] ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Löschen  ", Style::default().fg(Color::White)),
            Span::styled(" [+] ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" Neu  ", Style::default().fg(Color::White)),
            Span::styled(" [R] ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(" Ping  ", Style::default().fg(Color::White)),
            Span::styled(" [↑/↓] ", Style::default().fg(Color::Black).bg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::styled(" Nav  ", Style::default().fg(Color::White)),
            Span::styled(" [Q] ", Style::default().fg(Color::Black).bg(Color::Gray).add_modifier(Modifier::BOLD)),
            Span::styled(" Beenden", Style::default().fg(Color::White)),
        ];
        Paragraph::new(Line::from(footer_spans))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Rgb(30, 38, 56))))
    };
    f.render_widget(footer_widget, chunks[3]);
}
