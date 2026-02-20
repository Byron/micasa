// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use micasa_app::{
    AppCommand, AppEvent, AppMode, AppState, DashboardCounts, FormKind, FormPayload, TabKind,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use std::io;
use std::time::Duration;

pub trait AppRuntime {
    fn load_dashboard_counts(&mut self) -> Result<DashboardCounts>;
    fn load_tab_row_count(&mut self, tab: TabKind, include_deleted: bool) -> Result<Option<usize>>;
    fn submit_form(&mut self, payload: &FormPayload) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ViewData {
    dashboard_counts: DashboardCounts,
    active_tab_row_count: Option<usize>,
}

pub fn run_app<R: AppRuntime>(state: &mut AppState, runtime: &mut R) -> Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen).context("enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    let mut result = Ok(());
    let mut view_data = ViewData::default();
    if let Err(error) = refresh_view_data(state, runtime, &mut view_data) {
        state.dispatch(AppCommand::SetStatus(format!("load failed: {error}")));
    }

    loop {
        if let Err(error) = terminal.draw(|frame| render(frame, state, &view_data)) {
            result = Err(error).context("draw frame");
            break;
        }

        let has_event = event::poll(Duration::from_millis(200)).context("poll event")?;
        if !has_event {
            continue;
        }

        match event::read().context("read event")? {
            Event::Key(key) => {
                if handle_key_event(state, runtime, &mut view_data, key) {
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

fn handle_key_event<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    key: KeyEvent,
) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
        (KeyCode::Char('q'), _) => return true,
        (KeyCode::Tab, _) => {
            dispatch_and_refresh(state, runtime, view_data, AppCommand::NextTab);
        }
        (KeyCode::BackTab, _) => {
            dispatch_and_refresh(state, runtime, view_data, AppCommand::PrevTab);
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
                if let Some(payload) = template_payload_for_form(form_kind) {
                    state.dispatch(AppCommand::SetFormPayload(payload));
                }
            }
        }
        (KeyCode::Enter, _) => {
            if matches!(state.mode, AppMode::Form(_)) {
                let payload = match state.validated_form_payload() {
                    Ok(payload) => payload,
                    Err(error) => {
                        state.dispatch(AppCommand::SetStatus(format!("form invalid: {error}")));
                        return false;
                    }
                };

                if let Err(error) = runtime.submit_form(&payload) {
                    state.dispatch(AppCommand::SetStatus(format!("save failed: {error}")));
                    return false;
                }

                dispatch_and_refresh(state, runtime, view_data, AppCommand::SubmitForm);
            }
        }
        (KeyCode::Char('d'), _) => {
            dispatch_and_refresh(state, runtime, view_data, AppCommand::ToggleDeleted);
        }
        (KeyCode::Char('@'), _) => {
            state.dispatch(AppCommand::OpenChat);
        }
        (KeyCode::Esc, _) => {
            if state.chat == micasa_app::ChatVisibility::Visible {
                state.dispatch(AppCommand::CloseChat);
            } else if matches!(state.mode, AppMode::Form(_)) {
                state.dispatch(AppCommand::CancelForm);
            } else {
                state.dispatch(AppCommand::ExitToNav);
            }
        }
        _ => {}
    }

    false
}

fn render(frame: &mut ratatui::Frame<'_>, state: &AppState, view_data: &ViewData) {
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

    let body = Paragraph::new(render_body_text(state, view_data))
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

fn render_body_text(state: &AppState, view_data: &ViewData) -> String {
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
            lines.push(format!(
                "projects due: {}",
                view_data.dashboard_counts.projects_due
            ));
            lines.push(format!(
                "maintenance due: {}",
                view_data.dashboard_counts.maintenance_due
            ));
            lines.push(format!(
                "incidents open: {}",
                view_data.dashboard_counts.incidents_open
            ));
        }
        _ => {
            lines.push(String::new());
            if let Some(row_count) = view_data.active_tab_row_count {
                lines.push(format!("rows: {row_count}"));
            } else {
                lines.push("rows: n/a".to_owned());
            }
        }
    }

    if let AppMode::Form(kind) = state.mode {
        lines.push(String::new());
        lines.push(format!("form: {:?}", kind));
        if let Some(payload) = &state.form_payload {
            lines.push(format!("payload: {:?}", payload.kind()));
        }
    }

    lines.join("\n")
}

fn status_text(state: &AppState) -> String {
    let default = "tab/backtab nav | e edit | a form | enter submit | esc cancel | d deleted | @ chat | q quit";
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

fn template_payload_for_form(kind: FormKind) -> Option<FormPayload> {
    match kind {
        FormKind::Project => Some(FormPayload::Project(micasa_app::ProjectFormInput {
            title: "New project".to_owned(),
            project_type_id: micasa_app::ProjectTypeId::new(1),
            status: micasa_app::ProjectStatus::Planned,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: None,
            actual_cents: None,
        })),
        FormKind::Quote => Some(FormPayload::Quote(micasa_app::QuoteFormInput {
            project_id: micasa_app::ProjectId::new(1),
            vendor_id: micasa_app::VendorId::new(1),
            total_cents: 10_000,
            labor_cents: None,
            materials_cents: None,
            other_cents: None,
            received_date: None,
            notes: String::new(),
        })),
        FormKind::MaintenanceItem => Some(FormPayload::Maintenance(
            micasa_app::MaintenanceItemFormInput {
                name: "New maintenance".to_owned(),
                category_id: micasa_app::MaintenanceCategoryId::new(1),
                appliance_id: None,
                last_serviced_at: None,
                interval_months: 1,
                manual_url: String::new(),
                manual_text: String::new(),
                notes: String::new(),
                cost_cents: None,
            },
        )),
        FormKind::Incident => Some(FormPayload::Incident(micasa_app::IncidentFormInput {
            title: "New incident".to_owned(),
            description: String::new(),
            status: micasa_app::IncidentStatus::Open,
            severity: micasa_app::IncidentSeverity::Soon,
            date_noticed: time::Date::from_calendar_date(2026, time::Month::January, 1)
                .expect("valid static date"),
            date_resolved: None,
            location: String::new(),
            cost_cents: None,
            appliance_id: None,
            vendor_id: None,
            notes: String::new(),
        })),
        FormKind::Appliance => Some(FormPayload::Appliance(micasa_app::ApplianceFormInput {
            name: "New appliance".to_owned(),
            brand: String::new(),
            model_number: String::new(),
            serial_number: String::new(),
            purchase_date: None,
            warranty_expiry: None,
            location: String::new(),
            cost_cents: None,
            notes: String::new(),
        })),
        FormKind::Vendor => Some(FormPayload::Vendor(micasa_app::VendorFormInput {
            name: "New vendor".to_owned(),
            contact_name: String::new(),
            email: String::new(),
            phone: String::new(),
            website: String::new(),
            notes: String::new(),
        })),
        FormKind::Document => None,
        FormKind::HouseProfile | FormKind::ServiceLogEntry => None,
    }
}

fn dispatch_and_refresh<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    command: AppCommand,
) {
    let events = state.dispatch(command);
    if should_refresh_view(&events)
        && let Err(error) = refresh_view_data(state, runtime, view_data)
    {
        state.dispatch(AppCommand::SetStatus(format!("load failed: {error}")));
    }
}

fn should_refresh_view(events: &[AppEvent]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            AppEvent::TabChanged(_)
                | AppEvent::DeletedFilterChanged(_)
                | AppEvent::FormSubmitted(_)
        )
    })
}

fn refresh_view_data<R: AppRuntime>(
    state: &AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
) -> Result<()> {
    match state.active_tab {
        TabKind::Dashboard => {
            view_data.dashboard_counts = runtime.load_dashboard_counts()?;
            view_data.active_tab_row_count = None;
        }
        tab => {
            view_data.active_tab_row_count = runtime.load_tab_row_count(tab, state.show_deleted)?;
        }
    }
    Ok(())
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
    use micasa_app::{
        AppMode, AppState, ChatVisibility, DashboardCounts, FormKind, FormPayload, TabKind,
    };

    #[derive(Debug, Default)]
    struct TestRuntime {
        submit_count: usize,
    }

    impl super::AppRuntime for TestRuntime {
        fn load_dashboard_counts(&mut self) -> anyhow::Result<DashboardCounts> {
            Ok(DashboardCounts {
                projects_due: 2,
                maintenance_due: 1,
                incidents_open: 3,
            })
        }

        fn load_tab_row_count(
            &mut self,
            tab: TabKind,
            _include_deleted: bool,
        ) -> anyhow::Result<Option<usize>> {
            let count = match tab {
                TabKind::Dashboard => None,
                TabKind::House => None,
                TabKind::Projects => Some(5),
                TabKind::Quotes => Some(4),
                TabKind::Maintenance => Some(3),
                TabKind::ServiceLog => Some(2),
                TabKind::Incidents => Some(1),
                TabKind::Appliances => Some(6),
                TabKind::Vendors => Some(7),
                TabKind::Documents => Some(8),
            };
            Ok(count)
        }

        fn submit_form(&mut self, payload: &FormPayload) -> anyhow::Result<()> {
            payload.validate()?;
            self.submit_count += 1;
            Ok(())
        }
    }

    fn view_data_for_test() -> super::ViewData {
        super::ViewData::default()
    }

    #[test]
    fn tab_key_cycles_tabs() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();

        let should_quit = handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert!(!should_quit);
        assert_eq!(state.active_tab, TabKind::House);
    }

    #[test]
    fn at_key_opens_chat_and_esc_closes_it() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );
        assert_eq!(state.chat, ChatVisibility::Visible);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert_eq!(state.chat, ChatVisibility::Hidden);
    }

    #[test]
    fn a_key_enters_form_mode_for_tab() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );

        assert_eq!(state.mode, AppMode::Form(FormKind::Project));
    }

    #[test]
    fn enter_submits_form() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );
        assert_eq!(state.mode, AppMode::Form(FormKind::Project));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(state.mode, AppMode::Nav);
        assert_eq!(runtime.submit_count, 1);
    }

    #[test]
    fn quit_keys_exit() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();

        assert!(handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        ));

        assert!(handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ));
    }
}
