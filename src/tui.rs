use std::io::stdout;
use std::time::Duration;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};
use crate::config::{load_session, UserSession};
use crate::vault::{load_vault, ServerEntry, ServerVault};
use crate::runner::check_server_latency;

pub enum TuiAction {
    Connect(ServerEntry),
    TriggerLogin,
    TriggerAdd,
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

    pub fn selected_server(&self) -> Option<&ServerEntry> {
        self.vault.servers.get(self.selected_index)
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
}

pub fn run_tui() -> Result<Option<TuiAction>, String> {
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| e.to_string())?;

    // Drain console input buffer to prevent lingering enter keys
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

                // Delete-Bestätigungsdialog
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

                // Standard-Tasten
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(Some(TuiAction::Quit)),
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Char('l') => return Ok(Some(TuiAction::TriggerLogin)),
                    KeyCode::Char('r') => app.refresh_latencies(),
                    KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => {
                        if app.selected_server().is_some() {
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
            Constraint::Length(3), // Schlanker Header
            Constraint::Min(6),    // Aufgeräumte Server-Tabelle
            Constraint::Length(3), // Übersichtliche Fußzeile
        ])
        .split(f.area());

    // ── 1. HEADER ──────────────────────────────────────────────────
    let header_lines = vec![
        Line::from(vec![
            Span::styled("  SKULLS & BONES ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("// ", Style::default().fg(Color::DarkGray)),
            Span::styled("SSH ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(format!("({} Server)", app.vault.servers.len()), Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            if let Some(ref sess) = app.session {
                Span::styled(format!("  [✓] Angemeldet: {} ", sess.username), Style::default().fg(Color::Green))
            } else {
                Span::styled("  [!] Lokale Schlüssel (Drücke [L] für Login) ", Style::default().fg(Color::Yellow))
            },
            if let Some(ref sess) = app.session {
                Span::styled(format!("• Restzeit: {}", sess.time_remaining_str()), Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            },
        ]),
    ];

    let header_widget = Paragraph::new(header_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(45, 55, 75))),
    );
    f.render_widget(header_widget, chunks[0]);

    // ── 2. SERVER TABELLE ──────────────────────────────────────────
    let header_cells = ["STATUS", "NAME", "HOST : PORT", "BENUTZER", "TAGS", "PING"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
    let header_row = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = app.vault.servers.iter().enumerate().map(|(idx, s)| {
        let lat_opt = app.latencies.get(idx).copied().flatten();
        let (lat_str, status_cell) = match lat_opt {
            Some(d) => {
                let ms = d.as_secs_f64() * 1000.0;
                let color = if ms < 60.0 {
                    Color::Green
                } else if ms < 160.0 {
                    Color::Yellow
                } else {
                    Color::Red
                };
                (
                    Cell::from(format!("{:.1} ms", ms)).style(Style::default().fg(color)),
                    Cell::from(" ● ONLINE").style(Style::default().fg(Color::Green)),
                )
            }
            None => (
                Cell::from("---").style(Style::default().fg(Color::DarkGray)),
                Cell::from(" ○ OFFLINE").style(Style::default().fg(Color::Red)),
            ),
        };

        let tags_str = s.tags.join(", ");

        let cells = vec![
            status_cell,
            Cell::from(s.name.clone()).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Cell::from(format!("{}:{}", s.host, s.port)).style(Style::default().fg(Color::Rgb(148, 163, 184))),
            Cell::from(s.user.clone()).style(Style::default().fg(Color::Yellow)),
            Cell::from(tags_str).style(Style::default().fg(Color::Blue)),
            lat_str,
        ];
        Row::new(cells).height(1)
    }).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12), // Status
            Constraint::Length(20), // Name
            Constraint::Length(24), // Host:Port
            Constraint::Length(14), // User
            Constraint::Length(22), // Tags
            Constraint::Length(12), // Ping
        ],
    )
    .header(header_row)
    .block(
        Block::default()
            .title(Span::styled(" [ SERVER LISTE ] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(45, 55, 75))),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(30, 64, 175))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(" ❯ ");

    f.render_stateful_widget(table, chunks[1], &mut app.table_state);

    // ── 3. FOOTER ──────────────────────────────────────────────────
    let footer_spans = vec![
        Span::styled(" [ENTER] ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" Verbinden    ", Style::default().fg(Color::White)),
        Span::styled(" [+] ", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" Hinzufügen    ", Style::default().fg(Color::White)),
        Span::styled(" [D] ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled(" Löschen    ", Style::default().fg(Color::White)),
        Span::styled(" [R] ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(" Ping    ", Style::default().fg(Color::White)),
        Span::styled(" [Q] ", Style::default().fg(Color::Black).bg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Span::styled(" Beenden", Style::default().fg(Color::White)),
    ];

    let footer_widget = Paragraph::new(Line::from(footer_spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(45, 55, 75))),
    );
    f.render_widget(footer_widget, chunks[2]);

    // ── 4. LÖSCH-BESTÄTIGUNGSDIALOG ────────────────────────────────
    if app.confirm_delete {
        let name = app.selected_server().map(|s| s.name.as_str()).unwrap_or("Server");
        let delete_area = centered_rect(54, 7, f.area());
        f.render_widget(Clear, delete_area);

        let delete_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Möchtest du '", Style::default().fg(Color::White)),
                Span::styled(name, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::styled("' wirklich löschen?", Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("    [J / Enter] ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::styled(" Ja, löschen    ", Style::default().fg(Color::White)),
                Span::styled("  [N / Esc] ", Style::default().fg(Color::Black).bg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                Span::styled(" Abbrechen", Style::default().fg(Color::White)),
            ]),
        ];

        let delete_box = Paragraph::new(delete_lines).block(
            Block::default()
                .title(" ⚠️  SERVER LÖSCHEN ")
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Red)),
        );
        f.render_widget(delete_box, delete_area);
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}


