// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use micasa_app::{
    AppCommand, AppEvent, AppMode, AppState, Appliance, ApplianceId, DashboardCounts, Document,
    FormKind, FormPayload, HouseProfile, Incident, IncidentId, IncidentSeverity, MaintenanceItem,
    MaintenanceItemId, Project, ProjectId, ProjectStatus, Quote, ServiceLogEntry,
    ServiceLogEntryId, SortDirection, TabKind, Vendor,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs};
use std::cmp::Ordering;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DashboardSection {
    Incidents,
    Overdue,
    Upcoming,
    ActiveProjects,
    ExpiringSoon,
    RecentActivity,
}

impl DashboardSection {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Incidents => "incidents",
            Self::Overdue => "overdue",
            Self::Upcoming => "upcoming",
            Self::ActiveProjects => "active projects",
            Self::ExpiringSoon => "expiring soon",
            Self::RecentActivity => "recent activity",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardIncident {
    pub incident_id: IncidentId,
    pub title: String,
    pub severity: IncidentSeverity,
    pub days_open: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardMaintenance {
    pub maintenance_item_id: MaintenanceItemId,
    pub item_name: String,
    pub days_from_now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardProject {
    pub project_id: ProjectId,
    pub title: String,
    pub status: ProjectStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardWarranty {
    pub appliance_id: ApplianceId,
    pub appliance_name: String,
    pub days_from_now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardServiceEntry {
    pub service_log_entry_id: ServiceLogEntryId,
    pub maintenance_item_id: MaintenanceItemId,
    pub serviced_at: Date,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DashboardSnapshot {
    pub incidents: Vec<DashboardIncident>,
    pub overdue: Vec<DashboardMaintenance>,
    pub upcoming: Vec<DashboardMaintenance>,
    pub active_projects: Vec<DashboardProject>,
    pub expiring_warranties: Vec<DashboardWarranty>,
    pub recent_activity: Vec<DashboardServiceEntry>,
}

impl DashboardSnapshot {
    fn has_rows(&self) -> bool {
        !(self.incidents.is_empty()
            && self.overdue.is_empty()
            && self.upcoming.is_empty()
            && self.active_projects.is_empty()
            && self.expiring_warranties.is_empty()
            && self.recent_activity.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    Delete,
    Restore,
}

pub trait AppRuntime {
    fn load_dashboard_counts(&mut self) -> Result<DashboardCounts>;
    fn load_dashboard_snapshot(&mut self) -> Result<DashboardSnapshot>;
    fn load_tab_snapshot(
        &mut self,
        tab: TabKind,
        include_deleted: bool,
    ) -> Result<Option<TabSnapshot>>;
    fn submit_form(&mut self, payload: &FormPayload) -> Result<()>;
    fn apply_lifecycle(&mut self, tab: TabKind, row_id: i64, action: LifecycleAction)
    -> Result<()>;
    fn undo_last_edit(&mut self) -> Result<bool>;
    fn redo_last_edit(&mut self) -> Result<bool>;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableCommand {
    MoveRow(isize),
    MoveColumn(isize),
    JumpFirstRow,
    JumpLastRow,
    JumpFirstColumn,
    JumpLastColumn,
    CycleSort,
    ClearSort,
    TogglePin,
    ToggleFilter,
    ClearPins,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TableStatus {
    SortUnavailable,
    SortAsc(&'static str),
    SortDesc(&'static str),
    SortCleared,
    PinUnavailable,
    PinOn(String),
    PinOff,
    PinsCleared,
    SetPinFirst,
    FilterOn,
    FilterOff,
}

impl TableStatus {
    fn message(self) -> String {
        match self {
            Self::SortUnavailable => "sort unavailable".to_owned(),
            Self::SortAsc(column) => format!("sort {column} asc"),
            Self::SortDesc(column) => format!("sort {column} desc"),
            Self::SortCleared => "sort cleared".to_owned(),
            Self::PinUnavailable => "pin unavailable".to_owned(),
            Self::PinOn(value) => format!("pin on ({value})"),
            Self::PinOff => "pin off".to_owned(),
            Self::PinsCleared => "pins cleared".to_owned(),
            Self::SetPinFirst => "set a pin first".to_owned(),
            Self::FilterOn => "filter on".to_owned(),
            Self::FilterOff => "filter off".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TableEvent {
    CursorUpdated,
    Status(TableStatus),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardNavEntry {
    Section(DashboardSection),
    Incident(IncidentId),
    Overdue(MaintenanceItemId),
    Upcoming(MaintenanceItemId),
    ActiveProject(ProjectId),
    ExpiringWarranty(ApplianceId),
    RecentService(ServiceLogEntryId),
}

impl DashboardNavEntry {
    const fn target(self) -> Option<DashboardTarget> {
        match self {
            Self::Section(_) => None,
            Self::Incident(id) => Some(DashboardTarget {
                tab: TabKind::Incidents,
                row_id: id.get(),
            }),
            Self::Overdue(id) | Self::Upcoming(id) => Some(DashboardTarget {
                tab: TabKind::Maintenance,
                row_id: id.get(),
            }),
            Self::ActiveProject(id) => Some(DashboardTarget {
                tab: TabKind::Projects,
                row_id: id.get(),
            }),
            Self::ExpiringWarranty(id) => Some(DashboardTarget {
                tab: TabKind::Appliances,
                row_id: id.get(),
            }),
            Self::RecentService(id) => Some(DashboardTarget {
                tab: TabKind::ServiceLog,
                row_id: id.get(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DashboardTarget {
    tab: TabKind,
    row_id: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct DashboardUiState {
    visible: bool,
    cursor: usize,
    snapshot: DashboardSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingRowSelection {
    tab: TabKind,
    row_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalEvent {
    ClearStatus { token: u64 },
}

#[derive(Debug, Clone, PartialEq, Default)]
struct ViewData {
    dashboard_counts: DashboardCounts,
    dashboard: DashboardUiState,
    help_visible: bool,
    active_tab_snapshot: Option<TabSnapshot>,
    table_state: TableUiState,
    status_token: u64,
    pending_row_selection: Option<PendingRowSelection>,
}

pub fn run_app<R: AppRuntime>(state: &mut AppState, runtime: &mut R) -> Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen).context("enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    let mut view_data = ViewData::default();
    let (internal_tx, internal_rx) = mpsc::channel();

    if state.active_tab == TabKind::Dashboard {
        state.active_tab = TabKind::Projects;
        view_data.dashboard.visible = true;
    }

    if let Err(error) = refresh_view_data(state, runtime, &mut view_data) {
        state.dispatch(AppCommand::SetStatus(format!("load failed: {error}")));
    }

    let mut result = Ok(());
    loop {
        process_internal_events(state, &mut view_data, &internal_rx);

        if let Err(error) = terminal.draw(|frame| render(frame, state, &view_data)) {
            result = Err(error).context("draw frame");
            break;
        }

        let has_event = event::poll(Duration::from_millis(120)).context("poll event")?;
        if has_event {
            match event::read().context("read event")? {
                Event::Key(key) => {
                    if handle_key_event(state, runtime, &mut view_data, &internal_tx, key) {
                        break;
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    disable_raw_mode().context("disable raw mode")?;
    execute!(io::stdout(), terminal::LeaveAlternateScreen).context("leave alternate screen")?;
    result
}

fn process_internal_events(
    state: &mut AppState,
    view_data: &mut ViewData,
    rx: &Receiver<InternalEvent>,
) {
    while let Ok(event) = rx.try_recv() {
        match event {
            InternalEvent::ClearStatus { token } if token == view_data.status_token => {
                state.dispatch(AppCommand::ClearStatus);
            }
            InternalEvent::ClearStatus { .. } => {}
        }
    }
}

fn schedule_status_clear(internal_tx: &Sender<InternalEvent>, token: u64) {
    let sender = internal_tx.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(4));
        let _ = sender.send(InternalEvent::ClearStatus { token });
    });
}

fn emit_status(
    state: &mut AppState,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    message: impl Into<String>,
) {
    state.dispatch(AppCommand::SetStatus(message.into()));
    view_data.status_token = view_data.status_token.saturating_add(1);
    schedule_status_clear(internal_tx, view_data.status_token);
}

fn handle_key_event<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    key: KeyEvent,
) -> bool {
    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }

    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        emit_status(
            state,
            view_data,
            internal_tx,
            "cancel requested; no in-flight LLM operation",
        );
        return false;
    }

    if view_data.help_visible {
        if key.code == KeyCode::Esc || key.code == KeyCode::Char('?') {
            view_data.help_visible = false;
            emit_status(state, view_data, internal_tx, "help hidden");
        }
        return false;
    }

    if state.chat == micasa_app::ChatVisibility::Visible {
        if key.code == KeyCode::Esc {
            dispatch_and_refresh(
                state,
                runtime,
                view_data,
                AppCommand::CloseChat,
                internal_tx,
            );
        }
        return false;
    }

    if view_data.dashboard.visible {
        return handle_dashboard_overlay_key(state, runtime, view_data, internal_tx, key);
    }

    if handle_table_key(state, view_data, internal_tx, key) {
        return false;
    }

    if !matches!(state.mode, AppMode::Form(_)) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('f'), KeyModifiers::NONE) => {
                dispatch_and_refresh(state, runtime, view_data, AppCommand::NextTab, internal_tx);
                return false;
            }
            (KeyCode::Char('b'), KeyModifiers::NONE) => {
                dispatch_and_refresh(state, runtime, view_data, AppCommand::PrevTab, internal_tx);
                return false;
            }
            (KeyCode::Char('F'), _) => {
                dispatch_and_refresh(state, runtime, view_data, AppCommand::LastTab, internal_tx);
                return false;
            }
            (KeyCode::Char('B'), _) => {
                dispatch_and_refresh(state, runtime, view_data, AppCommand::FirstTab, internal_tx);
                return false;
            }
            (KeyCode::Char('@'), KeyModifiers::NONE) => {
                dispatch_and_refresh(state, runtime, view_data, AppCommand::OpenChat, internal_tx);
                return false;
            }
            (KeyCode::Char('?'), KeyModifiers::NONE) => {
                view_data.help_visible = true;
                emit_status(state, view_data, internal_tx, "help open");
                return false;
            }
            _ => {}
        }
    }

    match state.mode {
        AppMode::Nav => match (key.code, key.modifiers) {
            (KeyCode::Char('i'), KeyModifiers::NONE) => {
                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::EnterEditMode,
                    internal_tx,
                );
            }
            (KeyCode::Tab, KeyModifiers::NONE) => {
                let target = if state.active_tab == TabKind::House {
                    TabKind::Projects
                } else {
                    TabKind::House
                };
                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::SetActiveTab(target),
                    internal_tx,
                );
            }
            (KeyCode::Char('D'), _) => {
                view_data.dashboard.visible = !view_data.dashboard.visible;
                view_data.dashboard.cursor = 0;
                if let Err(error) = refresh_view_data(state, runtime, view_data) {
                    emit_status(
                        state,
                        view_data,
                        internal_tx,
                        format!("load failed: {error}"),
                    );
                } else if view_data.dashboard.visible {
                    emit_status(state, view_data, internal_tx, "dashboard open");
                } else {
                    emit_status(state, view_data, internal_tx, "dashboard hidden");
                }
            }
            (KeyCode::Esc, _) => {
                state.dispatch(AppCommand::ClearStatus);
            }
            _ => {}
        },
        AppMode::Edit => match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::ExitToNav,
                    internal_tx,
                );
            }
            (KeyCode::Char('x'), _) => {
                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::ToggleDeleted,
                    internal_tx,
                );
            }
            (KeyCode::Char('a'), _) | (KeyCode::Char('e'), _) => {
                if let Some(form_kind) = form_for_tab(state.active_tab) {
                    dispatch_and_refresh(
                        state,
                        runtime,
                        view_data,
                        AppCommand::OpenForm(form_kind),
                        internal_tx,
                    );
                    if let Some(payload) = template_payload_for_form(form_kind) {
                        dispatch_and_refresh(
                            state,
                            runtime,
                            view_data,
                            AppCommand::SetFormPayload(payload),
                            internal_tx,
                        );
                    }
                } else {
                    emit_status(state, view_data, internal_tx, "form unavailable");
                }
            }
            (KeyCode::Char('p'), _) => {
                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::OpenForm(FormKind::HouseProfile),
                    internal_tx,
                );
                if let Some(payload) = template_payload_for_form(FormKind::HouseProfile) {
                    dispatch_and_refresh(
                        state,
                        runtime,
                        view_data,
                        AppCommand::SetFormPayload(payload),
                        internal_tx,
                    );
                }
            }
            (KeyCode::Char('d'), _) => {
                if let Some((row_id, deleted)) = selected_row_metadata(view_data) {
                    let action = if deleted {
                        LifecycleAction::Restore
                    } else {
                        LifecycleAction::Delete
                    };
                    match runtime.apply_lifecycle(state.active_tab, row_id, action) {
                        Ok(()) => {
                            if let Err(error) = refresh_view_data(state, runtime, view_data) {
                                emit_status(
                                    state,
                                    view_data,
                                    internal_tx,
                                    format!("reload failed: {error}"),
                                );
                            } else {
                                let status = match action {
                                    LifecycleAction::Delete => "row deleted",
                                    LifecycleAction::Restore => "row restored",
                                };
                                emit_status(state, view_data, internal_tx, status);
                            }
                        }
                        Err(error) => {
                            emit_status(
                                state,
                                view_data,
                                internal_tx,
                                format!("delete failed: {error}"),
                            );
                        }
                    }
                } else {
                    emit_status(state, view_data, internal_tx, "no row selected");
                }
            }
            (KeyCode::Char('u'), _) => match runtime.undo_last_edit() {
                Ok(true) => {
                    if let Err(error) = refresh_view_data(state, runtime, view_data) {
                        emit_status(
                            state,
                            view_data,
                            internal_tx,
                            format!("reload failed: {error}"),
                        );
                    } else {
                        emit_status(state, view_data, internal_tx, "undo applied");
                    }
                }
                Ok(false) => emit_status(state, view_data, internal_tx, "nothing to undo"),
                Err(error) => emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("undo failed: {error}"),
                ),
            },
            (KeyCode::Char('r'), _) => match runtime.redo_last_edit() {
                Ok(true) => {
                    if let Err(error) = refresh_view_data(state, runtime, view_data) {
                        emit_status(
                            state,
                            view_data,
                            internal_tx,
                            format!("reload failed: {error}"),
                        );
                    } else {
                        emit_status(state, view_data, internal_tx, "redo applied");
                    }
                }
                Ok(false) => emit_status(state, view_data, internal_tx, "nothing to redo"),
                Err(error) => emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("redo failed: {error}"),
                ),
            },
            _ => {}
        },
        AppMode::Form(_) => match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::CancelForm,
                    internal_tx,
                );
            }
            (KeyCode::Enter, _) | (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                let payload = match state.validated_form_payload() {
                    Ok(payload) => payload,
                    Err(error) => {
                        emit_status(
                            state,
                            view_data,
                            internal_tx,
                            format!("form invalid: {error}"),
                        );
                        return false;
                    }
                };
                if let Err(error) = runtime.submit_form(&payload) {
                    emit_status(
                        state,
                        view_data,
                        internal_tx,
                        format!("save failed: {error}"),
                    );
                    return false;
                }

                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::SubmitForm,
                    internal_tx,
                );
            }
            _ => {}
        },
    }

    false
}

fn handle_dashboard_overlay_key<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    key: KeyEvent,
) -> bool {
    let entries = dashboard_nav_entries(&view_data.dashboard.snapshot);
    let nav_len = entries.len();
    if nav_len == 0 {
        view_data.dashboard.cursor = 0;
    } else if view_data.dashboard.cursor >= nav_len {
        view_data.dashboard.cursor = nav_len.saturating_sub(1);
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
            if nav_len > 0 {
                view_data.dashboard.cursor =
                    (view_data.dashboard.cursor + 1).min(nav_len.saturating_sub(1));
            }
        }
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
            view_data.dashboard.cursor = view_data.dashboard.cursor.saturating_sub(1);
        }
        (KeyCode::Char('g'), _) => {
            view_data.dashboard.cursor = 0;
        }
        (KeyCode::Char('G'), _) => {
            if nav_len > 0 {
                view_data.dashboard.cursor = nav_len - 1;
            }
        }
        (KeyCode::Enter, _) => {
            if let Some((entry, _)) = entries.get(view_data.dashboard.cursor)
                && let Some(target) = entry.target()
            {
                view_data.dashboard.visible = false;
                view_data.pending_row_selection = Some(PendingRowSelection {
                    tab: target.tab,
                    row_id: target.row_id,
                });
                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::SetActiveTab(target.tab),
                    internal_tx,
                );
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("dashboard -> {}", target.tab.label()),
                );
            }
        }
        (KeyCode::Char('D'), _) => {
            view_data.dashboard.visible = false;
            emit_status(state, view_data, internal_tx, "dashboard hidden");
        }
        (KeyCode::Char('f'), _) => {
            view_data.dashboard.visible = false;
            dispatch_and_refresh(state, runtime, view_data, AppCommand::NextTab, internal_tx);
        }
        (KeyCode::Char('b'), _) => {
            view_data.dashboard.visible = false;
            dispatch_and_refresh(state, runtime, view_data, AppCommand::PrevTab, internal_tx);
        }
        (KeyCode::Char('?'), _) => {
            view_data.help_visible = true;
        }
        _ => {}
    }

    true
}

fn selected_row_metadata(view_data: &ViewData) -> Option<(i64, bool)> {
    let projection = active_projection(view_data)?;
    let row = projection.rows.get(view_data.table_state.selected_row)?;
    match row.cells.first() {
        Some(TableCell::Integer(id)) => Some((*id, row.deleted)),
        _ => None,
    }
}

fn handle_table_key(
    state: &mut AppState,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    key: KeyEvent,
) -> bool {
    let can_use_table_keys = !view_data.dashboard.visible
        && !view_data.help_visible
        && state.chat == micasa_app::ChatVisibility::Hidden
        && !matches!(state.mode, AppMode::Form(_))
        && state.active_tab != TabKind::Dashboard
        && view_data.active_tab_snapshot.is_some();
    if !can_use_table_keys {
        return false;
    }

    let Some(command) = table_command_for_key(key) else {
        return false;
    };

    let event = apply_table_command(view_data, command);
    if let TableEvent::Status(status) = event {
        emit_status(state, view_data, internal_tx, status.message());
    }
    true
}
fn table_command_for_key(key: KeyEvent) -> Option<TableCommand> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Some(TableCommand::MoveRow(1)),
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Some(TableCommand::MoveRow(-1)),
        (KeyCode::Char('h'), _) | (KeyCode::Left, _) => Some(TableCommand::MoveColumn(-1)),
        (KeyCode::Char('l'), _) | (KeyCode::Right, _) => Some(TableCommand::MoveColumn(1)),
        (KeyCode::Char('g'), _) => Some(TableCommand::JumpFirstRow),
        (KeyCode::Char('G'), _) => Some(TableCommand::JumpLastRow),
        (KeyCode::Char('^'), _) => Some(TableCommand::JumpFirstColumn),
        (KeyCode::Char('$'), _) => Some(TableCommand::JumpLastColumn),
        (KeyCode::Char('s'), KeyModifiers::NONE) => Some(TableCommand::CycleSort),
        (KeyCode::Char('S'), _) => Some(TableCommand::ClearSort),
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => Some(TableCommand::ClearPins),
        (KeyCode::Char('n'), KeyModifiers::NONE) => Some(TableCommand::TogglePin),
        (KeyCode::Char('N'), _) => Some(TableCommand::ToggleFilter),
        _ => None,
    }
}

fn apply_table_command(view_data: &mut ViewData, command: TableCommand) -> TableEvent {
    match command {
        TableCommand::MoveRow(delta) => {
            move_row(view_data, delta);
            TableEvent::CursorUpdated
        }
        TableCommand::MoveColumn(delta) => {
            move_col(view_data, delta);
            TableEvent::CursorUpdated
        }
        TableCommand::JumpFirstRow => {
            view_data.table_state.selected_row = 0;
            TableEvent::CursorUpdated
        }
        TableCommand::JumpLastRow => {
            if let Some(projection) = active_projection(view_data) {
                view_data.table_state.selected_row = projection.row_count().saturating_sub(1);
            }
            TableEvent::CursorUpdated
        }
        TableCommand::JumpFirstColumn => {
            view_data.table_state.selected_col = 0;
            TableEvent::CursorUpdated
        }
        TableCommand::JumpLastColumn => {
            if let Some(projection) = active_projection(view_data) {
                view_data.table_state.selected_col = projection.column_count().saturating_sub(1);
            }
            TableEvent::CursorUpdated
        }
        TableCommand::CycleSort => TableEvent::Status(cycle_sort(view_data)),
        TableCommand::ClearSort => {
            view_data.table_state.sort = None;
            clamp_table_cursor(view_data);
            TableEvent::Status(TableStatus::SortCleared)
        }
        TableCommand::TogglePin => TableEvent::Status(toggle_pin(view_data)),
        TableCommand::ToggleFilter => TableEvent::Status(toggle_filter(view_data)),
        TableCommand::ClearPins => {
            view_data.table_state.pin = None;
            view_data.table_state.filter_active = false;
            clamp_table_cursor(view_data);
            TableEvent::Status(TableStatus::PinsCleared)
        }
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

    if view_data.dashboard.visible {
        let area = centered_rect(85, 78, frame.area());
        frame.render_widget(Clear, area);
        let dashboard = Paragraph::new(render_dashboard_overlay_text(
            &view_data.dashboard.snapshot,
            view_data.dashboard.cursor,
        ))
        .block(
            Block::default()
                .title("dashboard")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Cyan)),
        );
        frame.render_widget(dashboard, area);
    }

    if state.chat == micasa_app::ChatVisibility::Visible {
        let area = centered_rect(70, 45, frame.area());
        frame.render_widget(Clear, area);
        let chat = Paragraph::new("chat open (Rust parity in progress)\nPress esc to close")
            .block(Block::default().title("LLM").borders(Borders::ALL));
        frame.render_widget(chat, area);
    }

    if view_data.help_visible {
        let area = centered_rect(80, 72, frame.area());
        frame.render_widget(Clear, area);
        let help = Paragraph::new(help_overlay_text())
            .block(Block::default().title("help").borders(Borders::ALL));
        frame.render_widget(help, area);
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

fn dashboard_nav_entries(snapshot: &DashboardSnapshot) -> Vec<(DashboardNavEntry, String)> {
    let mut entries = Vec::new();

    if !snapshot.incidents.is_empty() {
        entries.push((
            DashboardNavEntry::Section(DashboardSection::Incidents),
            format!(
                "{} ({})",
                DashboardSection::Incidents.label(),
                snapshot.incidents.len()
            ),
        ));
        for incident in &snapshot.incidents {
            entries.push((
                DashboardNavEntry::Incident(incident.incident_id),
                format!(
                    "{} | {} | {}d",
                    incident.title,
                    incident.severity.as_str(),
                    incident.days_open.max(0)
                ),
            ));
        }
    }

    if !snapshot.overdue.is_empty() {
        entries.push((
            DashboardNavEntry::Section(DashboardSection::Overdue),
            format!(
                "{} ({})",
                DashboardSection::Overdue.label(),
                snapshot.overdue.len()
            ),
        ));
        for entry in &snapshot.overdue {
            entries.push((
                DashboardNavEntry::Overdue(entry.maintenance_item_id),
                format!(
                    "{} | {}d overdue",
                    entry.item_name,
                    entry.days_from_now.abs()
                ),
            ));
        }
    }

    if !snapshot.upcoming.is_empty() {
        entries.push((
            DashboardNavEntry::Section(DashboardSection::Upcoming),
            format!(
                "{} ({})",
                DashboardSection::Upcoming.label(),
                snapshot.upcoming.len()
            ),
        ));
        for entry in &snapshot.upcoming {
            entries.push((
                DashboardNavEntry::Upcoming(entry.maintenance_item_id),
                format!(
                    "{} | due in {}d",
                    entry.item_name,
                    entry.days_from_now.max(0)
                ),
            ));
        }
    }

    if !snapshot.active_projects.is_empty() {
        entries.push((
            DashboardNavEntry::Section(DashboardSection::ActiveProjects),
            format!(
                "{} ({})",
                DashboardSection::ActiveProjects.label(),
                snapshot.active_projects.len()
            ),
        ));
        for project in &snapshot.active_projects {
            entries.push((
                DashboardNavEntry::ActiveProject(project.project_id),
                format!("{} | {}", project.title, project.status.as_str()),
            ));
        }
    }

    if !snapshot.expiring_warranties.is_empty() {
        entries.push((
            DashboardNavEntry::Section(DashboardSection::ExpiringSoon),
            format!(
                "{} ({})",
                DashboardSection::ExpiringSoon.label(),
                snapshot.expiring_warranties.len()
            ),
        ));
        for warranty in &snapshot.expiring_warranties {
            let suffix = if warranty.days_from_now < 0 {
                format!("{}d expired", warranty.days_from_now.abs())
            } else {
                format!("{}d left", warranty.days_from_now)
            };
            entries.push((
                DashboardNavEntry::ExpiringWarranty(warranty.appliance_id),
                format!("{} | {}", warranty.appliance_name, suffix),
            ));
        }
    }

    if !snapshot.recent_activity.is_empty() {
        entries.push((
            DashboardNavEntry::Section(DashboardSection::RecentActivity),
            format!(
                "{} ({})",
                DashboardSection::RecentActivity.label(),
                snapshot.recent_activity.len()
            ),
        ));
        for activity in &snapshot.recent_activity {
            let cost = activity
                .cost_cents
                .map(format_money)
                .unwrap_or_else(|| "n/a".to_owned());
            entries.push((
                DashboardNavEntry::RecentService(activity.service_log_entry_id),
                format!(
                    "{} | item {} | {}",
                    activity.serviced_at,
                    activity.maintenance_item_id.get(),
                    cost
                ),
            ));
        }
    }

    entries
}

fn render_dashboard_overlay_text(snapshot: &DashboardSnapshot, cursor: usize) -> String {
    let entries = dashboard_nav_entries(snapshot);
    if entries.is_empty() {
        return String::new();
    }

    let mut lines = Vec::with_capacity(entries.len() + 2);
    for (index, (entry, text)) in entries.iter().enumerate() {
        let is_cursor = index == cursor.min(entries.len().saturating_sub(1));
        let prefix = if is_cursor { "> " } else { "  " };
        let formatted = match entry {
            DashboardNavEntry::Section(_) => format!("{prefix}{text}"),
            _ => format!("{prefix}  {text}"),
        };
        lines.push(formatted);
    }
    lines.push(String::new());
    lines.push("j/k move | g/G top/bottom | enter jump | D close | b/f switch | ? help".to_owned());
    lines.join("\n")
}

fn help_overlay_text() -> &'static str {
    "global: ctrl+q quit | ctrl+c cancel llm | ctrl+o mag mode\n\
nav: j/k/h/l g/G ^/$ | b/f tabs | B/F first/last | tab house | D dashboard\n\
nav: s/S sort | n/N pin/filter | ctrl+n clear pins | i edit | @ chat | ? help\n\
edit: a add | e edit form | d del/restore | x show deleted | u undo | r redo | esc nav\n\
form: ctrl+s or enter submit | esc cancel\n\
dashboard: j/k g/G enter jump D close b/f switch ? help"
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

fn cycle_sort(view_data: &mut ViewData) -> TableStatus {
    let Some(projection) = active_projection(view_data) else {
        return TableStatus::SortUnavailable;
    };
    if projection.column_count() == 0 {
        return TableStatus::SortUnavailable;
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
        }) => TableStatus::SortAsc(label),
        Some(SortSpec {
            direction: SortDirection::Desc,
            ..
        }) => TableStatus::SortDesc(label),
        None => TableStatus::SortCleared,
    }
}

fn toggle_pin(view_data: &mut ViewData) -> TableStatus {
    let Some((column, value)) = selected_cell(view_data) else {
        return TableStatus::PinUnavailable;
    };

    if let Some(existing) = &view_data.table_state.pin
        && existing.column == column
        && existing.value == value
    {
        view_data.table_state.pin = None;
        view_data.table_state.filter_active = false;
        clamp_table_cursor(view_data);
        return TableStatus::PinOff;
    }

    view_data.table_state.pin = Some(PinnedCell {
        column,
        value: value.clone(),
    });
    clamp_table_cursor(view_data);
    TableStatus::PinOn(truncate_label(&value.display(), 14))
}

fn toggle_filter(view_data: &mut ViewData) -> TableStatus {
    if view_data.table_state.pin.is_none() {
        return TableStatus::SetPinFirst;
    }

    view_data.table_state.filter_active = !view_data.table_state.filter_active;
    clamp_table_cursor(view_data);
    if view_data.table_state.filter_active {
        TableStatus::FilterOn
    } else {
        TableStatus::FilterOff
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
    let mode = match state.mode {
        AppMode::Nav => "NAV",
        AppMode::Edit => "EDIT",
        AppMode::Form(_) => "FORM",
    };
    let default = "j/k/h/l g/G ^/$ | b/f tabs | s/S sort | n/N pin/filter | ctrl+n clear | @ chat | D dashboard | ctrl+q quit";
    match &state.status_line {
        Some(status) => format!("{mode} | {status} | {default}"),
        None => format!("{mode} | {default}"),
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
    internal_tx: &Sender<InternalEvent>,
) {
    let events = state.dispatch(command);
    if should_refresh_view(&events)
        && let Err(error) = refresh_view_data(state, runtime, view_data)
    {
        emit_status(
            state,
            view_data,
            internal_tx,
            format!("load failed: {error}"),
        );
    }
    if events
        .iter()
        .any(|event| matches!(event, AppEvent::StatusUpdated(_)))
    {
        view_data.status_token = view_data.status_token.saturating_add(1);
        schedule_status_clear(internal_tx, view_data.status_token);
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
    view_data.dashboard_counts = runtime.load_dashboard_counts()?;
    view_data.dashboard.snapshot = runtime.load_dashboard_snapshot()?;
    if !view_data.dashboard.snapshot.has_rows() {
        view_data.dashboard.visible = false;
    }
    let dashboard_entries = dashboard_nav_entries(&view_data.dashboard.snapshot);
    if dashboard_entries.is_empty() {
        view_data.dashboard.cursor = 0;
    } else {
        view_data.dashboard.cursor = view_data
            .dashboard
            .cursor
            .min(dashboard_entries.len().saturating_sub(1));
    }

    match state.active_tab {
        TabKind::Dashboard => {
            view_data.active_tab_snapshot = None;
        }
        tab => {
            if view_data.table_state.tab != Some(tab) {
                view_data.table_state = TableUiState::default();
                view_data.table_state.tab = Some(tab);
            }
            view_data.active_tab_snapshot = runtime.load_tab_snapshot(tab, state.show_deleted)?;
            clamp_table_cursor(view_data);
            apply_pending_row_selection(view_data);
        }
    }
    Ok(())
}

fn apply_pending_row_selection(view_data: &mut ViewData) {
    let Some(selection) = view_data.pending_row_selection else {
        return;
    };
    if view_data.table_state.tab != Some(selection.tab) {
        return;
    }
    let Some(snapshot) = &view_data.active_tab_snapshot else {
        view_data.pending_row_selection = None;
        return;
    };

    let mut projection = projection_for_snapshot(snapshot, &view_data.table_state);
    if let Some(index) = find_row_index_by_id(&projection, selection.row_id) {
        view_data.table_state.selected_row = index;
        view_data.pending_row_selection = None;
        return;
    }

    view_data.table_state.pin = None;
    view_data.table_state.filter_active = false;
    view_data.table_state.sort = None;
    projection = projection_for_snapshot(snapshot, &view_data.table_state);
    if let Some(index) = find_row_index_by_id(&projection, selection.row_id) {
        view_data.table_state.selected_row = index;
    }
    view_data.pending_row_selection = None;
}

fn find_row_index_by_id(projection: &TableProjection, row_id: i64) -> Option<usize> {
    projection.rows.iter().position(|row| {
        matches!(
            row.cells.first(),
            Some(TableCell::Integer(id)) if *id == row_id
        )
    })
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
    use super::{
        AppRuntime, DashboardIncident, DashboardSnapshot, LifecycleAction, TabSnapshot,
        TableCommand, TableEvent, TableStatus, ViewData, apply_table_command, handle_key_event,
        refresh_view_data, table_command_for_key,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use micasa_app::{
        AppMode, AppState, ChatVisibility, DashboardCounts, FormKind, FormPayload,
        IncidentSeverity, Project, ProjectStatus, ProjectTypeId, TabKind,
    };
    use std::sync::mpsc;
    use time::OffsetDateTime;

    #[derive(Debug, Default)]
    struct TestRuntime {
        submit_count: usize,
        lifecycle_count: usize,
        undo_count: usize,
        redo_count: usize,
        can_undo: bool,
        can_redo: bool,
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

    impl AppRuntime for TestRuntime {
        fn load_dashboard_counts(&mut self) -> anyhow::Result<DashboardCounts> {
            Ok(DashboardCounts {
                projects_due: 2,
                maintenance_due: 1,
                incidents_open: 3,
            })
        }

        fn load_dashboard_snapshot(&mut self) -> anyhow::Result<DashboardSnapshot> {
            Ok(DashboardSnapshot {
                incidents: vec![DashboardIncident {
                    incident_id: micasa_app::IncidentId::new(9),
                    title: "Leak".to_owned(),
                    severity: IncidentSeverity::Urgent,
                    days_open: 2,
                }],
                ..DashboardSnapshot::default()
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

        fn apply_lifecycle(
            &mut self,
            _tab: TabKind,
            _row_id: i64,
            _action: LifecycleAction,
        ) -> anyhow::Result<()> {
            self.lifecycle_count += 1;
            Ok(())
        }

        fn undo_last_edit(&mut self) -> anyhow::Result<bool> {
            self.undo_count += 1;
            Ok(self.can_undo)
        }

        fn redo_last_edit(&mut self) -> anyhow::Result<bool> {
            self.redo_count += 1;
            Ok(self.can_redo)
        }
    }

    fn view_data_for_test() -> ViewData {
        ViewData::default()
    }

    fn internal_tx() -> mpsc::Sender<super::InternalEvent> {
        let (tx, _rx) = mpsc::channel();
        tx
    }

    #[test]
    fn tab_key_cycles_tabs() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();

        let should_quit = handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
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
        let tx = internal_tx();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );
        assert_eq!(state.chat, ChatVisibility::Visible);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert_eq!(state.chat, ChatVisibility::Hidden);
    }

    #[test]
    fn edit_mode_a_key_enters_form_mode_for_tab() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            mode: AppMode::Edit,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
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
        let tx = internal_tx();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
        );
        assert_eq!(state.mode, AppMode::Edit);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        );
        assert_eq!(state.mode, AppMode::Form(FormKind::Project));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
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
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );

        assert_eq!(view_data.table_state.selected_row, 1);
        assert_eq!(view_data.table_state.selected_col, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT),
        );
        assert_eq!(view_data.table_state.selected_row, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
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
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert!(view_data.table_state.sort.is_some());

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        assert!(view_data.table_state.pin.is_some());

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.filter_active);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        assert!(view_data.table_state.pin.is_none());
        assert!(!view_data.table_state.filter_active);
    }

    #[test]
    fn table_command_mapping_covers_sort_and_filter_keys() {
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
            Some(TableCommand::CycleSort)
        );
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL)),
            Some(TableCommand::ClearPins)
        );
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT)),
            Some(TableCommand::ToggleFilter)
        );
    }

    #[test]
    fn apply_table_command_returns_typed_status_events() {
        let state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        let first_sort = apply_table_command(&mut view_data, TableCommand::CycleSort);
        assert_eq!(first_sort, TableEvent::Status(TableStatus::SortAsc("id")));

        let first_pin = apply_table_command(&mut view_data, TableCommand::TogglePin);
        assert!(matches!(
            first_pin,
            TableEvent::Status(TableStatus::PinOn(_))
        ));
    }

    #[test]
    fn quit_keys_exit() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();

        assert!(handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
        ));

        assert!(!handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ));
    }

    #[test]
    fn dashboard_overlay_jumps_to_target_tab() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT),
        );
        assert!(view_data.dashboard.visible);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(state.active_tab, TabKind::Incidents);
        assert!(!view_data.dashboard.visible);
    }

    #[test]
    fn edit_mode_delete_and_undo_redo_dispatch_runtime_calls() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            mode: AppMode::Edit,
            ..AppState::default()
        };
        let mut runtime = TestRuntime {
            can_undo: true,
            can_redo: true,
            ..TestRuntime::default()
        };
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );
        assert_eq!(runtime.lifecycle_count, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        );
        assert_eq!(runtime.undo_count, 1);
        assert_eq!(runtime.redo_count, 1);
    }
}
