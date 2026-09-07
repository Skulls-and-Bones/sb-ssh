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
    TriggerEdit(ServerEntry),
    TriggerTunnel(ServerEntry),
    TriggerInfo(ServerEntry),
    TriggerTransfer(ServerEntry),
    TriggerExec(ServerEntry),
    Quit,
}

pub struct TuiApp {
    pub vault: ServerVault,
    pub session: Option<UserSession>,
    pub selected_index: usize,
    pub table_state: TableState,
    pub latencies: Vec<Option<Duration>>,
    pub confirm_delete: bool,
    pub show_help: bool,
    pub filter_query: String,
    pub is_filtering: bool,
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
            show_help: false,
            filter_query: String::new(),
            is_filtering: false,
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.filter_query.trim().is_empty() {
            (0..self.vault.servers.len()).collect()
        } else {
            let q = self.filter_query.to_lowercase();
            self.vault.servers.iter().enumerate()
                .filter(|(_, s)| {
                    s.name.to_lowercase().contains(&q)
                        || s.host.to_lowercase().contains(&q)
                        || s.user.to_lowercase().contains(&q)
                        || s.tags.iter().any(|t| t.to_lowercase().contains(&q))
                        || s.description.as_deref().unwrap_or("").to_lowercase().contains(&q)
                })
                .map(|(idx, _)| idx)
                .collect()
        }
    }

    pub fn selected_server(&self) -> Option<&ServerEntry> {
        let indices = self.filtered_indices();
        indices.get(self.selected_index).and_then(|&orig_idx| self.vault.servers.get(orig_idx))
    }

    pub fn selected_latency(&self) -> Option<Duration> {
        let indices = self.filtered_indices();
        indices.get(self.selected_index).and_then(|&orig_idx| self.latencies.get(orig_idx).copied().flatten())
    }

    pub fn reload(&mut self) {
        self.vault = load_vault();
        let indices = self.filtered_indices();
        if self.selected_index >= indices.len() && !indices.is_empty() {
            self.selected_index = indices.len() - 1;
        }
        if indices.is_empty() {
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
        let count = self.filtered_indices().len();
        if count == 0 { return; }
        self.selected_index = (self.selected_index + 1) % count;
        self.table_state.select(Some(self.selected_index));
    }

    pub fn previous(&mut self) {
        let count = self.filtered_indices().len();
        if count == 0 { return; }
        if self.selected_index == 0 {
            self.selected_index = count - 1;
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

    // Drain all pending events from console input buffer to avoid leftover keys
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

                // 1. In Live-Filter Mode
                if app.is_filtering {
                    match key.code {
                        KeyCode::Esc => {
                            app.is_filtering = false;
                        }
                        KeyCode::Enter => {
                            app.is_filtering = false;
                        }
                        KeyCode::Backspace => {
                            app.filter_query.pop();
                            app.selected_index = 0;
                            let count = app.filtered_indices().len();
                            if count > 0 {
                                app.table_state.select(Some(0));
                            } else {
                                app.table_state.select(None);
                            }
                        }
                        KeyCode::Char(c) => {
                            app.filter_query.push(c);
                            app.selected_index = 0;
                            let count = app.filtered_indices().len();
                            if count > 0 {
                                app.table_state.select(Some(0));
                            } else {
                                app.table_state.select(None);
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // 2. In Help Modal
                if app.show_help {
                    match key.code {
                        KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                            app.show_help = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                // 3. In Delete Confirmation Modal
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

                // 4. Standard Navigation & Shortcuts
                match key.code {
                    KeyCode::Char('q') => return Ok(Some(TuiAction::Quit)),
                    KeyCode::Esc => {
                        if !app.filter_query.is_empty() {
                            app.filter_query.clear();
                            app.selected_index = 0;
                            app.table_state.select(Some(0));
                        } else {
                            return Ok(Some(TuiAction::Quit));
                        }
                    }
                    KeyCode::Char('?') | KeyCode::F(1) => {
                        app.show_help = true;
                    }
                    KeyCode::Char('/') => {
                        app.is_filtering = true;
                    }
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
                    KeyCode::Char('p') => {
                        if let Some(server) = app.selected_server() {
                            return Ok(Some(TuiAction::TriggerTransfer(server.clone())));
                        }
                    }
                    KeyCode::Char('x') => {
                        if let Some(server) = app.selected_server() {
                            return Ok(Some(TuiAction::TriggerExec(server.clone())));
                        }
                    }
                    KeyCode::Char('e') => {
                        if let Some(server) = app.selected_server() {
                            return Ok(Some(TuiAction::TriggerEdit(server.clone())));
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Delete => {
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
            Constraint::Length(4), // Header / Brand & Session
            Constraint::Min(10),   // Main Split (Table + Inspector)
            Constraint::Length(3), // Hotkey Footer
        ])
        .split(f.area());

    // ── 1. HEADER & BRAND BAR ──────────────────────────────────────
    let online_count = app.latencies.iter().filter(|d| d.is_some()).count();
    let total_count = app.vault.servers.len();

    let header_lines = vec![
        Line::from(vec![
            Span::styled("  ⚡ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("SKULLS & BONES ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("// ", Style::default().fg(Color::DarkGray)),
            Span::styled("NETGATE ZERO-KEY SSH ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("v0.1.0 ", Style::default().fg(Color::Rgb(59, 130, 246))),
            Span::styled("[ SECURE TACTICAL FLEET VAULT ]", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            if app.session.is_some() {
                Span::styled("  [✓ AUTH] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::styled("  [! LOKAL] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            },
            if let Some(ref sess) = app.session {
                Span::styled(format!("{} ({}) ", sess.username, sess.provider), Style::default().fg(Color::White))
            } else {
                Span::styled("Ephemeres Ed25519-CA Aktiv ", Style::default().fg(Color::Gray))
            },
            Span::styled("• ", Style::default().fg(Color::DarkGray)),
            if let Some(ref sess) = app.session {
                Span::styled(format!("⏳ {} ", sess.time_remaining_str()), Style::default().fg(Color::Yellow))
            } else {
                Span::styled("🔐 Bereit ", Style::default().fg(Color::DarkGray))
            },
            Span::styled("• ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("🖥️  {} Server ", total_count), Style::default().fg(Color::Cyan)),
            Span::styled("• ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("⚡ {} Online ", online_count), Style::default().fg(Color::Green)),
            Span::styled("• ", Style::default().fg(Color::DarkGray)),
            Span::styled("🛡️  Zero-Key-Sprawl", Style::default().fg(Color::Rgb(96, 165, 250))),
        ]),
    ];

    let header_widget = Paragraph::new(header_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(45, 55, 75))),
    );
    f.render_widget(header_widget, chunks[0]);

    // ── 2. MAIN SPLIT (SERVER FLEET + INSPECTOR) ───────────────────
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(62), // Server Table
            Constraint::Percentage(38), // Inspector Card
        ])
        .split(chunks[1]);

    // 2a. Server Fleet Table
    let filter_title = if !app.filter_query.is_empty() {
        format!(" 🖥️  SERVER TREESOR [Filter: '{}'] ", app.filter_query)
    } else {
        " 🖥️  SERVER TREESOR ".to_string()
    };

    let table_block = Block::default()
        .title(Span::styled(
            filter_title,
            if !app.filter_query.is_empty() {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            },
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(
            if app.is_filtering {
                Color::Yellow
            } else {
                Color::Rgb(45, 55, 75)
            },
        ));

    let header_cells = ["STATUS", "SERVER ALIAS", "HOST : PORT", "USER", "TAGS", "LATENZ"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Rgb(56, 189, 248)).add_modifier(Modifier::BOLD)));
    let header_row = Row::new(header_cells).height(1).bottom_margin(1);

    let filtered = app.filtered_indices();

    let rows: Vec<Row> = filtered.iter().map(|&orig_idx| {
        let s = &app.vault.servers[orig_idx];
        let lat_opt = app.latencies.get(orig_idx).copied().flatten();
        let (lat_str, status_cell) = match lat_opt {
            Some(d) => {
                let ms = d.as_secs_f64() * 1000.0;
                let color = if ms < 50.0 {
                    Color::Green
                } else if ms < 150.0 {
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
            Cell::from(tags_str).style(Style::default().fg(Color::Rgb(192, 132, 252))),
            lat_str,
        ];
        Row::new(cells).height(1)
    }).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(11), // Status
            Constraint::Length(16), // Name
            Constraint::Length(22), // Host:Port
            Constraint::Length(10), // User
            Constraint::Length(18), // Tags
            Constraint::Length(11), // Latenz
        ],
    )
    .header(header_row)
    .block(table_block)
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(30, 64, 175))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(" ❯❯ ");

    f.render_stateful_widget(table, main_chunks[0], &mut app.table_state);

    // 2b. Server Inspector Card (Right Panel)
    let inspector_block = Block::default()
        .title(Span::styled(" 🔍  SERVER TELEMETRIE & DETAILS ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(45, 55, 75)));

    let inspector_content = if let Some(srv) = app.selected_server() {
        let lat_opt = app.selected_latency();
        let (signal_bar, signal_quality, signal_color) = latency_bar(lat_opt);
        let lat_str = match lat_opt {
            Some(d) => format!("{:.1} ms", d.as_secs_f64() * 1000.0),
            None => "Keine Antwort".to_string(),
        };

        let last_conn_str = srv.last_connected
            .map(|d| d.format("%d.%m.%Y %H:%M").to_string())
            .unwrap_or_else(|| "Bisher keine".to_string());

        let tags_badges: Vec<Span> = if srv.tags.is_empty() {
            vec![Span::styled("keine", Style::default().fg(Color::DarkGray))]
        } else {
            srv.tags.iter().map(|t| {
                Span::styled(format!("[{}] ", t), Style::default().fg(Color::Rgb(168, 85, 247)))
            }).collect()
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("── ENDPUNKT & IDENTITÄT ──────────────────────────", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  Alias:       ", Style::default().fg(Color::DarkGray)),
                Span::styled(&srv.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  Adresse:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}:{}", srv.host, srv.port), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("  SSH-User:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(&srv.user, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Line::from({
                let mut p = vec![Span::styled("  Tags:        ", Style::default().fg(Color::DarkGray))];
                p.extend(tags_badges);
                p
            }),
            Line::from(vec![
                Span::styled("  Info:        ", Style::default().fg(Color::DarkGray)),
                Span::styled(srv.description.as_deref().unwrap_or("Keine Beschreibung"), Style::default().fg(Color::Gray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("── NETZWERK TELEMETRIE ───────────────────────────", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  Status:      ", Style::default().fg(Color::DarkGray)),
                if lat_opt.is_some() {
                    Span::styled("● ONLINE", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("○ OFFLINE", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
                },
                Span::styled(format!(" ({})", lat_str), Style::default().fg(Color::Gray)),
            ]),
            Line::from(vec![
                Span::styled("  Signal:      ", Style::default().fg(Color::DarkGray)),
                Span::styled(signal_bar, Style::default().fg(signal_color).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" {}", signal_quality), Style::default().fg(signal_color)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("── SICHERHEIT & CRYPTO ───────────────────────────", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  Engine:      ", Style::default().fg(Color::DarkGray)),
                Span::styled("100% Pure-Rust (russh + SFTP)", Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::styled("  Zertifikat:  ", Style::default().fg(Color::DarkGray)),
                Span::styled("S&B Ephemeral Ed25519 (8h)", Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("  Letzter S&B: ", Style::default().fg(Color::DarkGray)),
                Span::styled(last_conn_str, Style::default().fg(Color::Gray)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("── SCHNELL-AKTIONEN ──────────────────────────────", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  [ENTER] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("Verbinden (PTY)    ", Style::default().fg(Color::White)),
                Span::styled("[I] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled("Health-Probe", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [U]     ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                Span::styled("TCP-Tunnel         ", Style::default().fg(Color::White)),
                Span::styled("[P] ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                Span::styled("SFTP Transfer", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [X]     ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled("Remote Exec        ", Style::default().fg(Color::White)),
                Span::styled("[E] ", Style::default().fg(Color::Rgb(96, 165, 250)).add_modifier(Modifier::BOLD)),
                Span::styled("Bearbeiten", Style::default().fg(Color::White)),
            ]),
        ];
        lines
    } else {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Kein Server ausgewählt.", Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Drücke ", Style::default().fg(Color::Gray)),
                Span::styled("[+] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("um einen neuen Server hinzuzufügen.", Style::default().fg(Color::Gray)),
            ]),
        ]
    };

    let inspector_widget = Paragraph::new(inspector_content).block(inspector_block);
    f.render_widget(inspector_widget, main_chunks[1]);

    // ── 3. FOOTER SHORTCUT BAR ─────────────────────────────────────
    let footer_spans = vec![
        Span::styled(" [ENTER] ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" Connect  ", Style::default().fg(Color::White)),
        Span::styled(" [I] ", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" Probe  ", Style::default().fg(Color::White)),
        Span::styled(" [U] ", Style::default().fg(Color::Black).bg(Color::Blue).add_modifier(Modifier::BOLD)),
        Span::styled(" Tunnel  ", Style::default().fg(Color::White)),
        Span::styled(" [P] ", Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::styled(" SFTP  ", Style::default().fg(Color::White)),
        Span::styled(" [X] ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(" Exec  ", Style::default().fg(Color::White)),
        Span::styled(" [+] ", Style::default().fg(Color::Black).bg(Color::LightCyan).add_modifier(Modifier::BOLD)),
        Span::styled(" Neu  ", Style::default().fg(Color::White)),
        Span::styled(" [E] ", Style::default().fg(Color::Black).bg(Color::Rgb(96, 165, 250)).add_modifier(Modifier::BOLD)),
        Span::styled(" Edit  ", Style::default().fg(Color::White)),
        Span::styled(" [D] ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::styled(" Löschen  ", Style::default().fg(Color::White)),
        Span::styled(" [/] ", Style::default().fg(Color::Black).bg(Color::LightYellow).add_modifier(Modifier::BOLD)),
        Span::styled(" Filter  ", Style::default().fg(Color::White)),
        Span::styled(" [R] ", Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)),
        Span::styled(" Ping  ", Style::default().fg(Color::White)),
        Span::styled(" [?] ", Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(" Hilfe  ", Style::default().fg(Color::White)),
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

    // ── 4. POPUPS & MODALS (OVERLAYS) ──────────────────────────────
    // 4a. Live Filter Bar
    if app.is_filtering {
        let filter_area = centered_rect(58, 5, f.area());
        f.render_widget(Clear, filter_area);

        let filter_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Suchbegriff: ", Style::default().fg(Color::Gray)),
                Span::styled(&app.filter_query, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled("█", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK)),
            ]),
        ];

        let filter_box = Paragraph::new(filter_lines).block(
            Block::default()
                .title(" 🔍  SERVER LIVE-FILTER (Esc = Abbrechen, Enter = Anwenden) ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Yellow)),
        );
        f.render_widget(filter_box, filter_area);
    }

    // 4b. Delete Confirmation Modal
    if app.confirm_delete {
        let name = app.selected_server().map(|s| s.name.as_str()).unwrap_or("Server");
        let delete_area = centered_rect(56, 7, f.area());
        f.render_widget(Clear, delete_area);

        let delete_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Möchtest du '", Style::default().fg(Color::White)),
                Span::styled(name, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::styled("' wirklich aus dem Tresor löschen?", Style::default().fg(Color::White)),
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
                .title(" ⚠️  SERVER LÖSCHEN  ")
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Red)),
        );
        f.render_widget(delete_box, delete_area);
    }

    // 4c. Help Modal
    if app.show_help {
        let help_area = centered_rect(68, 20, f.area());
        f.render_widget(Clear, help_area);

        let help_lines = vec![
            Line::from(vec![
                Span::styled("  TASTENKOMBINATIONEN & BEDIENUNG", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  [↑ / k] [↓ / j]   ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled("In der Serverliste navigieren", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [ENTER]           ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("Direkte SSH2-Terminalverbindung (mit Ephemeral-Cert)", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [I]               ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled("System Health Probe (CPU, RAM, Disk, Uptime)", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [U]               ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                Span::styled("SSH Portweiterleitung (Lokaler TCP-Tunnel)", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [P]               ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                Span::styled("SFTP Datei-Transfer (Upload / Download)", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [X]               ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled("Multi-Server Broadcast Remote Execution", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [+ / A]           ", Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
                Span::styled("Neuen Server zum Tresor hinzufügen", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [E]               ", Style::default().fg(Color::Rgb(96, 165, 250)).add_modifier(Modifier::BOLD)),
                Span::styled("Ausgewählten Server bearbeiten", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [D / Entf]        ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::styled("Server dauerhaft löschen (mit Bestätigung)", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [/]               ", Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD)),
                Span::styled("Live-Filter starten (Server suchen)", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [R]               ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                Span::styled("Latenzen aller Server neu anpingen", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [L]               ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("OAuth2 / OIDC Browser-Login", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [? / F1]          ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled("Dieses Hilfefenster öffnen / schließen", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  [Q / Esc]         ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                Span::styled("TUI beenden", Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Drücke [ESC] oder [?] um dieses Fenster zu schließen.", Style::default().fg(Color::DarkGray)),
            ]),
        ];

        let help_box = Paragraph::new(help_lines).block(
            Block::default()
                .title(" 📖  SKULLS & BONES // NETGATE TUI HILFE  ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
        );
        f.render_widget(help_box, help_area);
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

fn latency_bar(latency: Option<Duration>) -> (String, &'static str, Color) {
    match latency {
        Some(d) => {
            let ms = d.as_secs_f64() * 1000.0;
            if ms < 35.0 {
                ("[■■■■■■■■■■■■■■■]".to_string(), "Exzellent", Color::Green)
            } else if ms < 80.0 {
                ("[■■■■■■■■■■■■□□□]".to_string(), "Sehr gut", Color::Green)
            } else if ms < 150.0 {
                ("[■■■■■■■■□□□□□□□]".to_string(), "Gut", Color::Yellow)
            } else if ms < 300.0 {
                ("[■■■■■□□□□□□□□□□]".to_string(), "Moderat", Color::Rgb(245, 158, 11))
            } else {
                ("[■■□□□□□□□□□□□□□]".to_string(), "Langsam", Color::Red)
            }
        }
        None => ("[□□□□□□□□□□□□□□□]".to_string(), "Offline", Color::DarkGray),
    }
}

