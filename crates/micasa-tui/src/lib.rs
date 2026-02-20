// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use micasa_app::{AppCommand, AppMode, AppState, DashboardCounts, FormKind, TabKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use std::io;
use std::time::Duration;

pub fn run_app(state: &mut AppState, counts: DashboardCounts) -> Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen).context("enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    let mut result = Ok(());
    loop {
        if let Err(error) = terminal.draw(|frame| render(frame, state, &counts)) {
            result = Err(error).context("draw frame");
            break;
        }

        let has_event = event::poll(Duration::from_millis(200)).context("poll event")?;
        if !has_event {
            continue;
        }

        match event::read().context("read event")? {
            Event::Key(key) => {
                if handle_key_event(state, key) {
                    break;
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    disable_raw_mode().context("disable raw mode")?;
    execute!(io::stdout(), terminal::LeaveAlternateScreen).context("leave alternate screen")?;
    result
}

pub fn handle_key_event(state: &mut AppState, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
        (KeyCode::Char('q'), _) => return true,
        (KeyCode::Tab, _) => {
            state.dispatch(AppCommand::NextTab);
        }
        (KeyCode::BackTab, _) => {
            state.dispatch(AppCommand::PrevTab);
        }
        (KeyCode::Char('e'), _) => {
            state.dispatch(AppCommand::EnterEditMode);
        }
        (KeyCode::Char('n'), _) => {
            state.dispatch(AppCommand::ExitToNav);
        }
        (KeyCode::Char('a'), _) => {
            if let Some(form_kind) = form_for_tab(state.active_tab) {
                state.dispatch(AppCommand::OpenForm(form_kind));
            }
        }
        (KeyCode::Char('d'), _) => {
            state.dispatch(AppCommand::ToggleDeleted);
        }
        (KeyCode::Char('@'), _) => {
            state.dispatch(AppCommand::OpenChat);
        }
        (KeyCode::Esc, _) => {
            if state.chat == micasa_app::ChatVisibility::Visible {
                state.dispatch(AppCommand::CloseChat);
            } else {
                state.dispatch(AppCommand::ExitToNav);
            }
        }
        _ => {}
    }

    false
}

fn render(frame: &mut ratatui::Frame<'_>, state: &AppState, counts: &DashboardCounts) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let selected = TabKind::ALL
        .iter()
        .position(|tab| *tab == state.active_tab)
        .unwrap_or(0);
    let tab_titles = TabKind::ALL
        .iter()
        .map(|tab| format!(" {} ", tab.label()))
        .collect::<Vec<String>>();

    let tabs = Tabs::new(tab_titles)
        .block(Block::default().title("micasa").borders(Borders::ALL))
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .select(selected);
    frame.render_widget(tabs, layout[0]);

    let body = Paragraph::new(render_body_text(state, counts))
        .block(Block::default().borders(Borders::ALL).title("view"));
    frame.render_widget(body, layout[1]);

    let status = status_text(state);
    let status_widget = Paragraph::new(status)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(status_widget, layout[2]);

    if state.chat == micasa_app::ChatVisibility::Visible {
        let area = centered_rect(70, 45, frame.area());
        frame.render_widget(Clear, area);
        let chat = Paragraph::new("chat open (Rust parity in progress)\nPress esc to close")
            .block(Block::default().title("LLM").borders(Borders::ALL));
        frame.render_widget(chat, area);
    }
}

fn render_body_text(state: &AppState, counts: &DashboardCounts) -> String {
    let mut lines = Vec::new();
    lines.push(format!("mode: {}", mode_label(state.mode)));
    lines.push(format!(
        "deleted: {}",
        if state.show_deleted {
            "shown"
        } else {
            "hidden"
        }
    ));

    match state.active_tab {
        TabKind::Dashboard => {
            lines.push(String::new());
            lines.push(format!("projects due: {}", counts.projects_due));
            lines.push(format!("maintenance due: {}", counts.maintenance_due));
            lines.push(format!("incidents open: {}", counts.incidents_open));
        }
        _ => {
            lines.push(String::new());
            lines.push(format!(
                "{} view ported to Rust baseline; full parity work in progress.",
                state.active_tab.label()
            ));
        }
    }

    lines.join("\n")
}

fn status_text(state: &AppState) -> String {
    let default = "tab/backtab nav | e edit | a form | d deleted | @ chat | q quit";
    match &state.status_line {
        Some(status) => format!("{status} | {default}"),
        None => default.to_owned(),
    }
}

fn mode_label(mode: AppMode) -> &'static str {
    match mode {
        AppMode::Nav => "nav",
        AppMode::Edit => "edit",
        AppMode::Form(_) => "form",
    }
}

fn form_for_tab(tab: TabKind) -> Option<FormKind> {
    match tab {
        TabKind::Dashboard => None,
        TabKind::House => Some(FormKind::HouseProfile),
        TabKind::Projects => Some(FormKind::Project),
        TabKind::Quotes => Some(FormKind::Quote),
        TabKind::Maintenance => Some(FormKind::MaintenanceItem),
        TabKind::ServiceLog => Some(FormKind::ServiceLogEntry),
        TabKind::Incidents => Some(FormKind::Incident),
        TabKind::Appliances => Some(FormKind::Appliance),
        TabKind::Vendors => Some(FormKind::Vendor),
        TabKind::Documents => Some(FormKind::Document),
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::handle_key_event;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use micasa_app::{AppMode, AppState, ChatVisibility, FormKind, TabKind};

    #[test]
    fn tab_key_cycles_tabs() {
        let mut state = AppState::default();

        let should_quit =
            handle_key_event(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(!should_quit);
        assert_eq!(state.active_tab, TabKind::House);
    }

    #[test]
    fn at_key_opens_chat_and_esc_closes_it() {
        let mut state = AppState::default();

        handle_key_event(
            &mut state,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );
        assert_eq!(state.chat, ChatVisibility::Visible);

        handle_key_event(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(state.chat, ChatVisibility::Hidden);
    }

    #[test]
    fn a_key_enters_form_mode_for_tab() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };

        handle_key_event(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );

        assert_eq!(state.mode, AppMode::Form(FormKind::Project));
    }

    #[test]
    fn quit_keys_exit() {
        let mut state = AppState::default();

        assert!(handle_key_event(
            &mut state,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        ));

        assert!(handle_key_event(
            &mut state,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ));
    }
}
