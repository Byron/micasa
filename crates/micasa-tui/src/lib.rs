// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use micasa_app::{
    AppCommand, AppEvent, AppMode, AppState, Appliance, DashboardCounts, Document, FormKind,
    FormPayload, HouseProfile, Incident, MaintenanceItem, Project, Quote, ServiceLogEntry,
    SortDirection, TabKind, Vendor,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs};
use std::cmp::Ordering;
use std::io;
use std::time::Duration;
use time::Date;

#[derive(Debug, Clone, PartialEq)]
pub enum TabSnapshot {
    House(Box<Option<HouseProfile>>),
    Projects(Vec<Project>),
    Quotes(Vec<Quote>),
    Maintenance(Vec<MaintenanceItem>),
    ServiceLog(Vec<ServiceLogEntry>),
    Incidents(Vec<Incident>),
    Appliances(Vec<Appliance>),
    Vendors(Vec<Vendor>),
    Documents(Vec<Document>),
}

impl TabSnapshot {
    pub const fn tab_kind(&self) -> TabKind {
        match self {
            Self::House(_) => TabKind::House,
            Self::Projects(_) => TabKind::Projects,
            Self::Quotes(_) => TabKind::Quotes,
            Self::Maintenance(_) => TabKind::Maintenance,
            Self::ServiceLog(_) => TabKind::ServiceLog,
            Self::Incidents(_) => TabKind::Incidents,
            Self::Appliances(_) => TabKind::Appliances,
            Self::Vendors(_) => TabKind::Vendors,
            Self::Documents(_) => TabKind::Documents,
        }
    }

    pub fn row_count(&self) -> usize {
        match self {
            Self::House(profile) => usize::from(profile.as_ref().is_some()),
            Self::Projects(rows) => rows.len(),
            Self::Quotes(rows) => rows.len(),
            Self::Maintenance(rows) => rows.len(),
            Self::ServiceLog(rows) => rows.len(),
            Self::Incidents(rows) => rows.len(),
            Self::Appliances(rows) => rows.len(),
            Self::Vendors(rows) => rows.len(),
            Self::Documents(rows) => rows.len(),
        }
    }
}

pub trait AppRuntime {
    fn load_dashboard_counts(&mut self) -> Result<DashboardCounts>;
    fn load_tab_snapshot(
        &mut self,
        tab: TabKind,
        include_deleted: bool,
    ) -> Result<Option<TabSnapshot>>;
    fn submit_form(&mut self, payload: &FormPayload) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
enum TableCell {
    Text(String),
    Integer(i64),
    OptionalInteger(Option<i64>),
    Decimal(Option<f64>),
    Date(Option<Date>),
    Money(Option<i64>),
}

impl TableCell {
    fn display(&self) -> String {
        match self {
            Self::Text(value) => value.clone(),
            Self::Integer(value) => value.to_string(),
            Self::OptionalInteger(Some(value)) => value.to_string(),
            Self::OptionalInteger(None) => String::new(),
            Self::Decimal(Some(value)) => format!("{value:.1}"),
            Self::Decimal(None) => String::new(),
            Self::Date(Some(value)) => value.to_string(),
            Self::Date(None) => String::new(),
            Self::Money(Some(cents)) => format_money(*cents),
            Self::Money(None) => String::new(),
        }
    }

    fn cmp_value(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Integer(left), Self::Integer(right)) => left.cmp(right),
            (Self::OptionalInteger(left), Self::OptionalInteger(right)) => left.cmp(right),
            (Self::Decimal(left), Self::Decimal(right)) => match (left, right) {
                (Some(left), Some(right)) => left.total_cmp(right),
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            (Self::Date(left), Self::Date(right)) => left.cmp(right),
            (Self::Money(left), Self::Money(right)) => left.cmp(right),
            (Self::Text(left), Self::Text(right)) => {
                left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
            }
            _ => self
                .display()
                .to_ascii_lowercase()
                .cmp(&other.display().to_ascii_lowercase()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TableRowProjection {
    cells: Vec<TableCell>,
    deleted: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct TableProjection {
    title: &'static str,
    columns: Vec<&'static str>,
    rows: Vec<TableRowProjection>,
}

impl TableProjection {
    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn column_count(&self) -> usize {
        self.columns.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SortSpec {
    column: usize,
    direction: SortDirection,
}

#[derive(Debug, Clone, PartialEq)]
struct PinnedCell {
    column: usize,
    value: TableCell,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct TableUiState {
    tab: Option<TabKind>,
    selected_row: usize,
    selected_col: usize,
    sort: Option<SortSpec>,
    pin: Option<PinnedCell>,
    filter_active: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct ViewData {
    dashboard_counts: DashboardCounts,
    active_tab_snapshot: Option<TabSnapshot>,
    table_state: TableUiState,
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
    if handle_table_key(state, view_data, key) {
        return false;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => return true,
        (KeyCode::Char('q'), _) => return true,
        (KeyCode::Tab, _) | (KeyCode::Char('f'), _) => {
            dispatch_and_refresh(state, runtime, view_data, AppCommand::NextTab);
        }
        (KeyCode::BackTab, _) | (KeyCode::Char('b'), _) => {
            dispatch_and_refresh(state, runtime, view_data, AppCommand::PrevTab);
        }
        (KeyCode::Char('e'), _) | (KeyCode::Char('i'), _) => {
            state.dispatch(AppCommand::EnterEditMode);
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
        (KeyCode::Char('x'), _) | (KeyCode::Char('d'), _) => {
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

fn handle_table_key(state: &mut AppState, view_data: &mut ViewData, key: KeyEvent) -> bool {
    let can_use_table_keys = state.chat == micasa_app::ChatVisibility::Hidden
        && !matches!(state.mode, AppMode::Form(_))
        && state.active_tab != TabKind::Dashboard
        && view_data.active_tab_snapshot.is_some();
    if !can_use_table_keys {
        return false;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
            move_row(view_data, 1);
            true
        }
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
            move_row(view_data, -1);
            true
        }
        (KeyCode::Char('h'), _) | (KeyCode::Left, _) => {
            move_col(view_data, -1);
            true
        }
        (KeyCode::Char('l'), _) | (KeyCode::Right, _) => {
            move_col(view_data, 1);
            true
        }
        (KeyCode::Char('g'), _) => {
            view_data.table_state.selected_row = 0;
            true
        }
        (KeyCode::Char('G'), _) => {
            if let Some(projection) = active_projection(view_data) {
                view_data.table_state.selected_row = projection.row_count().saturating_sub(1);
            }
            true
        }
        (KeyCode::Char('^'), _) => {
            view_data.table_state.selected_col = 0;
            true
        }
        (KeyCode::Char('$'), _) => {
            if let Some(projection) = active_projection(view_data) {
                view_data.table_state.selected_col = projection.column_count().saturating_sub(1);
            }
            true
        }
        (KeyCode::Char('s'), KeyModifiers::NONE) => {
            let status = cycle_sort(view_data);
            state.dispatch(AppCommand::SetStatus(status));
            true
        }
        (KeyCode::Char('S'), _) => {
            view_data.table_state.sort = None;
            clamp_table_cursor(view_data);
            state.dispatch(AppCommand::SetStatus("sort cleared".to_owned()));
            true
        }
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            view_data.table_state.pin = None;
            view_data.table_state.filter_active = false;
            clamp_table_cursor(view_data);
            state.dispatch(AppCommand::SetStatus("pins cleared".to_owned()));
            true
        }
        (KeyCode::Char('n'), KeyModifiers::NONE) => {
            let status = toggle_pin(view_data);
            state.dispatch(AppCommand::SetStatus(status));
            true
        }
        (KeyCode::Char('N'), _) => {
            let status = toggle_filter(view_data);
            state.dispatch(AppCommand::SetStatus(status));
            true
        }
        _ => false,
    }
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

    if state.active_tab == TabKind::Dashboard {
        let body = Paragraph::new(render_dashboard_text(state, view_data))
            .block(Block::default().borders(Borders::ALL).title("dashboard"));
        frame.render_widget(body, layout[1]);
    } else {
        render_table(frame, layout[1], state, view_data);
    }

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

fn render_dashboard_text(state: &AppState, view_data: &ViewData) -> String {
    [
        format!("mode: {}", mode_label(state.mode)),
        format!(
            "deleted: {}",
            if state.show_deleted {
                "shown"
            } else {
                "hidden"
            }
        ),
        String::new(),
        format!("projects due: {}", view_data.dashboard_counts.projects_due),
        format!(
            "maintenance due: {}",
            view_data.dashboard_counts.maintenance_due
        ),
        format!(
            "incidents open: {}",
            view_data.dashboard_counts.incidents_open
        ),
    ]
    .join("\n")
}

fn render_table(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &AppState,
    view_data: &ViewData,
) {
    let Some(snapshot) = &view_data.active_tab_snapshot else {
        let empty = Paragraph::new(String::new()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(state.active_tab.label()),
        );
        frame.render_widget(empty, area);
        return;
    };

    let projection = projection_for_snapshot(snapshot, &view_data.table_state);
    let columns = projection.columns.len();
    let widths = vec![Constraint::Min(8); columns.max(1)];

    let header_cells = projection
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let mut label = (*column).to_owned();
            if let Some(sort) = view_data.table_state.sort
                && sort.column == index
            {
                let suffix = match sort.direction {
                    SortDirection::Asc => " ↑",
                    SortDirection::Desc => " ↓",
                };
                label.push_str(suffix);
            }
            Cell::from(label).style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells);

    let rows = projection.rows.iter().enumerate().map(|(row_index, row)| {
        let selected_row = row_index == view_data.table_state.selected_row;
        let pin_match = row_matches_pin(row, &view_data.table_state);
        let preview_dim = view_data.table_state.pin.is_some()
            && !view_data.table_state.filter_active
            && !pin_match;

        let cells = row
            .cells
            .iter()
            .enumerate()
            .map(|(column_index, value)| {
                let mut style = Style::default();
                if row.deleted {
                    style = style
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::CROSSED_OUT);
                }
                if preview_dim {
                    style = style.fg(Color::DarkGray);
                }
                if selected_row {
                    style = style.bg(Color::DarkGray);
                }
                if selected_row && column_index == view_data.table_state.selected_col {
                    style = Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD);
                }
                Cell::from(value.display()).style(style)
            })
            .collect::<Vec<_>>();

        Row::new(cells)
    });

    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .block(
            Block::default()
                .title(table_title(&projection, &view_data.table_state))
                .borders(Borders::ALL),
        );
    frame.render_widget(table, area);
}

fn table_title(projection: &TableProjection, table_state: &TableUiState) -> String {
    let mut parts = vec![format!(
        "{} r:{} c:{}",
        projection.title,
        projection.row_count(),
        projection.column_count()
    )];

    if let Some(sort) = table_state.sort
        && let Some(label) = projection.columns.get(sort.column)
    {
        let direction = match sort.direction {
            SortDirection::Asc => "asc",
            SortDirection::Desc => "desc",
        };
        parts.push(format!("sort {label} {direction}"));
    }

    if let Some(pin) = &table_state.pin
        && let Some(label) = projection.columns.get(pin.column)
    {
        let value = pin.value.display();
        parts.push(format!("pin {label}={}", truncate_label(&value, 12)));
    }

    if table_state.filter_active {
        parts.push("filter on".to_owned());
    }

    parts.join(" | ")
}

fn truncate_label(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn row_matches_pin(row: &TableRowProjection, table_state: &TableUiState) -> bool {
    match &table_state.pin {
        Some(pin) => row
            .cells
            .get(pin.column)
            .map(|value| value == &pin.value)
            .unwrap_or(false),
        None => true,
    }
}

fn active_projection(view_data: &ViewData) -> Option<TableProjection> {
    view_data
        .active_tab_snapshot
        .as_ref()
        .map(|snapshot| projection_for_snapshot(snapshot, &view_data.table_state))
}

fn projection_for_snapshot(snapshot: &TabSnapshot, table_state: &TableUiState) -> TableProjection {
    let mut projection = base_projection(snapshot);

    if let Some(sort) = table_state.sort
        && sort.column < projection.column_count()
    {
        projection.rows.sort_by(|left, right| {
            let left_value = left.cells.get(sort.column);
            let right_value = right.cells.get(sort.column);
            let order = match (left_value, right_value) {
                (Some(left), Some(right)) => left.cmp_value(right),
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            };
            match sort.direction {
                SortDirection::Asc => order,
                SortDirection::Desc => order.reverse(),
            }
        });
    }

    if table_state.filter_active
        && let Some(pin) = &table_state.pin
    {
        projection.rows.retain(|row| {
            row.cells
                .get(pin.column)
                .map(|value| value == &pin.value)
                .unwrap_or(false)
        });
    }

    projection
}

fn base_projection(snapshot: &TabSnapshot) -> TableProjection {
    match snapshot {
        TabSnapshot::House(profile) => {
            let rows = profile
                .as_ref()
                .as_ref()
                .map(|profile| {
                    vec![TableRowProjection {
                        cells: vec![
                            TableCell::Text(profile.nickname.clone()),
                            TableCell::Text(profile.city.clone()),
                            TableCell::Text(profile.state.clone()),
                            TableCell::OptionalInteger(profile.bedrooms.map(i64::from)),
                            TableCell::Decimal(profile.bathrooms),
                            TableCell::OptionalInteger(profile.square_feet.map(i64::from)),
                            TableCell::OptionalInteger(profile.year_built.map(i64::from)),
                            TableCell::Date(profile.insurance_renewal),
                            TableCell::Money(profile.property_tax_cents),
                        ],
                        deleted: false,
                    }]
                })
                .unwrap_or_default();
            TableProjection {
                title: "house",
                columns: vec![
                    "nickname",
                    "city",
                    "state",
                    "bed",
                    "bath",
                    "sqft",
                    "year",
                    "ins renew",
                    "tax",
                ],
                rows,
            }
        }
        TabSnapshot::Projects(rows) => TableProjection {
            title: "projects",
            columns: vec!["id", "title", "status", "budget", "actual"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.title.clone()),
                        TableCell::Text(row.status.as_str().to_owned()),
                        TableCell::Money(row.budget_cents),
                        TableCell::Money(row.actual_cents),
                    ],
                    deleted: row.deleted_at.is_some(),
                })
                .collect(),
        },
        TabSnapshot::Quotes(rows) => TableProjection {
            title: "quotes",
            columns: vec!["id", "project", "vendor", "total", "recv"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Integer(row.project_id.get()),
                        TableCell::Integer(row.vendor_id.get()),
                        TableCell::Money(Some(row.total_cents)),
                        TableCell::Date(row.received_date),
                    ],
                    deleted: row.deleted_at.is_some(),
                })
                .collect(),
        },
        TabSnapshot::Maintenance(rows) => TableProjection {
            title: "maintenance",
            columns: vec!["id", "item", "cat", "appliance", "last", "every", "cost"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.name.clone()),
                        TableCell::Integer(row.category_id.get()),
                        TableCell::OptionalInteger(row.appliance_id.map(|id| id.get())),
                        TableCell::Date(row.last_serviced_at),
                        TableCell::Integer(i64::from(row.interval_months)),
                        TableCell::Money(row.cost_cents),
                    ],
                    deleted: row.deleted_at.is_some(),
                })
                .collect(),
        },
        TabSnapshot::ServiceLog(rows) => TableProjection {
            title: "service",
            columns: vec!["id", "maint", "date", "vendor", "cost"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Integer(row.maintenance_item_id.get()),
                        TableCell::Date(Some(row.serviced_at)),
                        TableCell::OptionalInteger(row.vendor_id.map(|id| id.get())),
                        TableCell::Money(row.cost_cents),
                    ],
                    deleted: row.deleted_at.is_some(),
                })
                .collect(),
        },
        TabSnapshot::Incidents(rows) => TableProjection {
            title: "incidents",
            columns: vec![
                "id", "title", "status", "sev", "noticed", "resolved", "cost",
            ],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.title.clone()),
                        TableCell::Text(row.status.as_str().to_owned()),
                        TableCell::Text(row.severity.as_str().to_owned()),
                        TableCell::Date(Some(row.date_noticed)),
                        TableCell::Date(row.date_resolved),
                        TableCell::Money(row.cost_cents),
                    ],
                    deleted: row.deleted_at.is_some(),
                })
                .collect(),
        },
        TabSnapshot::Appliances(rows) => TableProjection {
            title: "appliances",
            columns: vec!["id", "name", "brand", "location", "warranty", "cost"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.name.clone()),
                        TableCell::Text(row.brand.clone()),
                        TableCell::Text(row.location.clone()),
                        TableCell::Date(row.warranty_expiry),
                        TableCell::Money(row.cost_cents),
                    ],
                    deleted: row.deleted_at.is_some(),
                })
                .collect(),
        },
        TabSnapshot::Vendors(rows) => TableProjection {
            title: "vendors",
            columns: vec!["id", "name", "contact", "email", "phone"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.name.clone()),
                        TableCell::Text(row.contact_name.clone()),
                        TableCell::Text(row.email.clone()),
                        TableCell::Text(row.phone.clone()),
                    ],
                    deleted: row.deleted_at.is_some(),
                })
                .collect(),
        },
        TabSnapshot::Documents(rows) => TableProjection {
            title: "documents",
            columns: vec!["id", "title", "file", "entity", "size"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.title.clone()),
                        TableCell::Text(row.file_name.clone()),
                        TableCell::Text(row.entity_kind.as_str().to_owned()),
                        TableCell::Integer(row.size_bytes),
                    ],
                    deleted: row.deleted_at.is_some(),
                })
                .collect(),
        },
    }
}

fn format_money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let absolute = cents.unsigned_abs();
    let dollars = absolute / 100;
    let cents_component = absolute % 100;
    format!("{sign}${dollars}.{cents_component:02}")
}

fn move_row(view_data: &mut ViewData, delta: isize) {
    let Some(projection) = active_projection(view_data) else {
        return;
    };
    let row_count = projection.row_count();
    if row_count == 0 {
        view_data.table_state.selected_row = 0;
        return;
    }

    let current = view_data.table_state.selected_row;
    let next = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize)
    };
    view_data.table_state.selected_row = next.min(row_count.saturating_sub(1));
}

fn move_col(view_data: &mut ViewData, delta: isize) {
    let Some(projection) = active_projection(view_data) else {
        return;
    };
    let column_count = projection.column_count();
    if column_count == 0 {
        view_data.table_state.selected_col = 0;
        return;
    }

    let current = view_data.table_state.selected_col;
    let next = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as usize)
    };
    view_data.table_state.selected_col = next.min(column_count.saturating_sub(1));
}

fn selected_cell(view_data: &ViewData) -> Option<(usize, TableCell)> {
    let projection = active_projection(view_data)?;
    let row = projection.rows.get(view_data.table_state.selected_row)?;
    let col = view_data
        .table_state
        .selected_col
        .min(projection.column_count().saturating_sub(1));
    let cell = row.cells.get(col)?;
    Some((col, cell.clone()))
}

fn cycle_sort(view_data: &mut ViewData) -> String {
    let Some(projection) = active_projection(view_data) else {
        return "sort unavailable".to_owned();
    };
    if projection.column_count() == 0 {
        return "sort unavailable".to_owned();
    }

    let column = view_data
        .table_state
        .selected_col
        .min(projection.column_count().saturating_sub(1));
    let label = projection.columns[column];

    view_data.table_state.sort = match view_data.table_state.sort {
        Some(existing) if existing.column == column && existing.direction == SortDirection::Asc => {
            Some(SortSpec {
                column,
                direction: SortDirection::Desc,
            })
        }
        Some(existing)
            if existing.column == column && existing.direction == SortDirection::Desc =>
        {
            None
        }
        _ => Some(SortSpec {
            column,
            direction: SortDirection::Asc,
        }),
    };

    clamp_table_cursor(view_data);
    match view_data.table_state.sort {
        Some(SortSpec {
            direction: SortDirection::Asc,
            ..
        }) => format!("sort {label} asc"),
        Some(SortSpec {
            direction: SortDirection::Desc,
            ..
        }) => format!("sort {label} desc"),
        None => "sort cleared".to_owned(),
    }
}

fn toggle_pin(view_data: &mut ViewData) -> String {
    let Some((column, value)) = selected_cell(view_data) else {
        return "pin unavailable".to_owned();
    };

    if let Some(existing) = &view_data.table_state.pin
        && existing.column == column
        && existing.value == value
    {
        view_data.table_state.pin = None;
        view_data.table_state.filter_active = false;
        clamp_table_cursor(view_data);
        return "pin off".to_owned();
    }

    view_data.table_state.pin = Some(PinnedCell {
        column,
        value: value.clone(),
    });
    clamp_table_cursor(view_data);
    format!("pin on ({})", truncate_label(&value.display(), 14))
}

fn toggle_filter(view_data: &mut ViewData) -> String {
    if view_data.table_state.pin.is_none() {
        return "set a pin first".to_owned();
    }

    view_data.table_state.filter_active = !view_data.table_state.filter_active;
    clamp_table_cursor(view_data);
    if view_data.table_state.filter_active {
        "filter on".to_owned()
    } else {
        "filter off".to_owned()
    }
}

fn clamp_table_cursor(view_data: &mut ViewData) {
    let Some(snapshot) = &view_data.active_tab_snapshot else {
        view_data.table_state.selected_col = 0;
        view_data.table_state.selected_row = 0;
        return;
    };

    let mut projection = projection_for_snapshot(snapshot, &view_data.table_state);

    if let Some(sort) = view_data.table_state.sort
        && sort.column >= projection.column_count()
    {
        view_data.table_state.sort = None;
        projection = projection_for_snapshot(snapshot, &view_data.table_state);
    }

    if let Some(pin) = &view_data.table_state.pin
        && pin.column >= projection.column_count()
    {
        view_data.table_state.pin = None;
        view_data.table_state.filter_active = false;
        projection = projection_for_snapshot(snapshot, &view_data.table_state);
    }

    if projection.column_count() == 0 {
        view_data.table_state.selected_col = 0;
    } else {
        view_data.table_state.selected_col = view_data
            .table_state
            .selected_col
            .min(projection.column_count().saturating_sub(1));
    }

    if projection.row_count() == 0 {
        view_data.table_state.selected_row = 0;
    } else {
        view_data.table_state.selected_row = view_data
            .table_state
            .selected_row
            .min(projection.row_count().saturating_sub(1));
    }
}

fn status_text(state: &AppState) -> String {
    let default = "tab/backtab tabs | j/k/h/l g/G ^/$ | s/S sort | n/N pin/filter | ctrl+n clear | a form | enter submit | x deleted | @ chat | q quit";
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
        FormKind::HouseProfile => Some(FormPayload::HouseProfile(Box::new(
            micasa_app::HouseProfileFormInput {
                nickname: "My house".to_owned(),
                address_line_1: String::new(),
                address_line_2: String::new(),
                city: String::new(),
                state: String::new(),
                postal_code: String::new(),
                year_built: None,
                square_feet: None,
                lot_square_feet: None,
                bedrooms: None,
                bathrooms: None,
                foundation_type: String::new(),
                wiring_type: String::new(),
                roof_type: String::new(),
                exterior_type: String::new(),
                heating_type: String::new(),
                cooling_type: String::new(),
                water_source: String::new(),
                sewer_type: String::new(),
                parking_type: String::new(),
                basement_type: String::new(),
                insurance_carrier: String::new(),
                insurance_policy: String::new(),
                insurance_renewal: None,
                property_tax_cents: None,
                hoa_name: String::new(),
                hoa_fee_cents: None,
            },
        ))),
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
        FormKind::ServiceLogEntry => Some(FormPayload::ServiceLogEntry(
            micasa_app::ServiceLogEntryFormInput {
                maintenance_item_id: micasa_app::MaintenanceItemId::new(1),
                serviced_at: time::Date::from_calendar_date(2026, time::Month::January, 1)
                    .expect("valid static date"),
                vendor_id: None,
                cost_cents: None,
                notes: String::new(),
            },
        )),
        FormKind::Document => None,
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
            view_data.active_tab_snapshot = None;
        }
        tab => {
            if view_data.table_state.tab != Some(tab) {
                view_data.table_state = TableUiState::default();
                view_data.table_state.tab = Some(tab);
            }
            view_data.active_tab_snapshot = runtime.load_tab_snapshot(tab, state.show_deleted)?;
            clamp_table_cursor(view_data);
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
    use super::{TabSnapshot, ViewData, handle_key_event, refresh_view_data};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use micasa_app::{
        AppMode, AppState, ChatVisibility, DashboardCounts, FormKind, FormPayload, Project,
        ProjectStatus, ProjectTypeId, TabKind,
    };
    use time::OffsetDateTime;

    #[derive(Debug, Default)]
    struct TestRuntime {
        submit_count: usize,
    }

    impl TestRuntime {
        fn sample_project(id: i64, title: &str) -> Project {
            Project {
                id: micasa_app::ProjectId::new(id),
                title: title.to_owned(),
                project_type_id: ProjectTypeId::new(1),
                status: ProjectStatus::Planned,
                description: String::new(),
                start_date: None,
                end_date: None,
                budget_cents: Some(id * 1000),
                actual_cents: None,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                deleted_at: None,
            }
        }
    }

    impl super::AppRuntime for TestRuntime {
        fn load_dashboard_counts(&mut self) -> anyhow::Result<DashboardCounts> {
            Ok(DashboardCounts {
                projects_due: 2,
                maintenance_due: 1,
                incidents_open: 3,
            })
        }

        fn load_tab_snapshot(
            &mut self,
            tab: TabKind,
            _include_deleted: bool,
        ) -> anyhow::Result<Option<TabSnapshot>> {
            let snapshot = match tab {
                TabKind::Dashboard => None,
                TabKind::House => Some(TabSnapshot::House(Box::new(None))),
                TabKind::Projects => Some(TabSnapshot::Projects(vec![
                    Self::sample_project(1, "Alpha"),
                    Self::sample_project(2, "Beta"),
                ])),
                TabKind::Quotes => Some(TabSnapshot::Quotes(Vec::new())),
                TabKind::Maintenance => Some(TabSnapshot::Maintenance(Vec::new())),
                TabKind::ServiceLog => Some(TabSnapshot::ServiceLog(Vec::new())),
                TabKind::Incidents => Some(TabSnapshot::Incidents(Vec::new())),
                TabKind::Appliances => Some(TabSnapshot::Appliances(Vec::new())),
                TabKind::Vendors => Some(TabSnapshot::Vendors(Vec::new())),
                TabKind::Documents => Some(TabSnapshot::Documents(Vec::new())),
            };
            Ok(snapshot)
        }

        fn submit_form(&mut self, payload: &FormPayload) -> anyhow::Result<()> {
            payload.validate()?;
            self.submit_count += 1;
            Ok(())
        }
    }

    fn view_data_for_test() -> ViewData {
        ViewData::default()
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
    fn movement_keys_adjust_table_cursor() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );

        assert_eq!(view_data.table_state.selected_row, 1);
        assert_eq!(view_data.table_state.selected_col, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT),
        );
        assert_eq!(view_data.table_state.selected_row, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('^'), KeyModifiers::SHIFT),
        );
        assert_eq!(view_data.table_state.selected_col, 0);
    }

    #[test]
    fn sort_and_filter_toggles_update_state() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert!(view_data.table_state.sort.is_some());

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        assert!(view_data.table_state.pin.is_some());

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.filter_active);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        assert!(view_data.table_state.pin.is_none());
        assert!(!view_data.table_state.filter_active);
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
