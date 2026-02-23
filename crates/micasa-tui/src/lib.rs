// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use micasa_app::{
    AppCommand, AppEvent, AppMode, AppSetting, AppState, Appliance, ApplianceId, DashboardCounts,
    Document, DocumentEntityKind, FormKind, FormPayload, HouseProfile, Incident, IncidentId,
    IncidentSeverity, MaintenanceItem, MaintenanceItemId, Project, ProjectId, ProjectStatus, Quote,
    ServiceLogEntry, ServiceLogEntryId, SettingKey, SettingValue, SortDirection, TabKind, Vendor,
    VendorId,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use time::{Date, Month, OffsetDateTime};

const HALF_PAGE_ROWS: isize = 10;
const FULL_PAGE_ROWS: isize = 20;
const LINK_ARROW: &str = "→";
const DRILL_ARROW: &str = "↘";
const FILTER_MARK_ACTIVE: &str = "▼";
const FILTER_MARK_ACTIVE_INVERTED: &str = "▲";
const FILTER_MARK_PREVIEW: &str = "▽";
const FILTER_MARK_PREVIEW_INVERTED: &str = "△";

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
    Settings(Vec<AppSetting>),
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
            Self::Settings(_) => TabKind::Settings,
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
            Self::Settings(rows) => rows.len(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatHistoryRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatHistoryMessage {
    pub role: ChatHistoryRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPipelineResult {
    pub answer: String,
    pub sql: Option<String>,
    pub used_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatPipelineEvent {
    SqlChunk {
        request_id: u64,
        chunk: String,
    },
    SqlReady {
        request_id: u64,
        sql: String,
    },
    FallbackStarted {
        request_id: u64,
    },
    AnswerChunk {
        request_id: u64,
        chunk: String,
    },
    Completed {
        request_id: u64,
        result: ChatPipelineResult,
    },
    Failed {
        request_id: u64,
        error: String,
    },
}

impl ChatPipelineEvent {
    const fn request_id(&self) -> u64 {
        match self {
            Self::SqlChunk { request_id, .. }
            | Self::SqlReady { request_id, .. }
            | Self::FallbackStarted { request_id }
            | Self::AnswerChunk { request_id, .. }
            | Self::Completed { request_id, .. }
            | Self::Failed { request_id, .. } => *request_id,
        }
    }
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
    fn load_chat_history(&mut self) -> Result<Vec<String>>;
    fn append_chat_input(&mut self, input: &str) -> Result<()>;
    fn apply_lifecycle(&mut self, tab: TabKind, row_id: i64, action: LifecycleAction)
    -> Result<()>;
    fn undo_last_edit(&mut self) -> Result<bool>;
    fn redo_last_edit(&mut self) -> Result<bool>;
    fn set_show_dashboard_preference(&mut self, show: bool) -> Result<()>;
    fn list_chat_models(&mut self) -> Result<Vec<String>>;
    fn active_chat_model(&mut self) -> Result<Option<String>>;
    fn select_chat_model(&mut self, model: &str) -> Result<()>;
    fn run_chat_pipeline(
        &mut self,
        question: &str,
        history: &[ChatHistoryMessage],
    ) -> Result<ChatPipelineResult>;
    fn spawn_chat_pipeline(
        &mut self,
        request_id: u64,
        question: &str,
        history: &[ChatHistoryMessage],
        tx: Sender<InternalEvent>,
    ) -> Result<()> {
        let event = match self.run_chat_pipeline(question, history) {
            Ok(result) => {
                InternalEvent::ChatPipeline(ChatPipelineEvent::Completed { request_id, result })
            }
            Err(error) => InternalEvent::ChatPipeline(ChatPipelineEvent::Failed {
                request_id,
                error: error.to_string(),
            }),
        };
        tx.send(event)
            .map_err(|_| anyhow::anyhow!("chat event channel closed"))?;
        Ok(())
    }
    fn cancel_chat_pipeline(&mut self, _request_id: u64) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TableCell {
    Text(String),
    Integer(i64),
    OptionalInteger(Option<i64>),
    Decimal(Option<f64>),
    Date(Option<Date>),
    Money(Option<i64>),
    IntervalMonths(i32),
    ProjectStatus(ProjectStatus),
    IncidentStatus(micasa_app::IncidentStatus),
    IncidentSeverity(IncidentSeverity),
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
            Self::Money(Some(cents)) => format_compact_money(*cents),
            Self::Money(None) => String::new(),
            Self::IntervalMonths(months) => format_interval_months(*months),
            Self::ProjectStatus(status) => status_label_for_project_status(*status).to_owned(),
            Self::IncidentStatus(status) => status_label_for_incident_status(*status).to_owned(),
            Self::IncidentSeverity(severity) => {
                status_label_for_incident_severity(*severity).to_owned()
            }
        }
    }

    fn display_with_mag_mode(&self, mag_mode: bool) -> String {
        if !mag_mode {
            return self.display();
        }

        match self {
            Self::Text(value) => value.clone(),
            Self::Integer(value) => format_magnitude_i64(*value),
            Self::OptionalInteger(Some(value)) => format_magnitude_i64(*value),
            Self::OptionalInteger(None) => String::new(),
            Self::Decimal(Some(value)) => format_magnitude_f64(*value),
            Self::Decimal(None) => String::new(),
            Self::Date(Some(value)) => value.to_string(),
            Self::Date(None) => String::new(),
            Self::Money(Some(cents)) => format_magnitude_money_without_unit(*cents),
            Self::Money(None) => String::new(),
            Self::IntervalMonths(months) => format_interval_months(*months),
            Self::ProjectStatus(status) => status_label_for_project_status(*status).to_owned(),
            Self::IncidentStatus(status) => status_label_for_incident_status(*status).to_owned(),
            Self::IncidentSeverity(severity) => {
                status_label_for_incident_severity(*severity).to_owned()
            }
        }
    }

    fn is_null(&self) -> bool {
        matches!(
            self,
            Self::OptionalInteger(None)
                | Self::Decimal(None)
                | Self::Date(None)
                | Self::Money(None)
        )
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
            (Self::IntervalMonths(left), Self::IntervalMonths(right)) => left.cmp(right),
            (Self::ProjectStatus(left), Self::ProjectStatus(right)) => {
                status_label_for_project_status(*left).cmp(status_label_for_project_status(*right))
            }
            (Self::IncidentStatus(left), Self::IncidentStatus(right)) => {
                status_label_for_incident_status(*left)
                    .cmp(status_label_for_incident_status(*right))
            }
            (Self::IncidentSeverity(left), Self::IncidentSeverity(right)) => {
                status_label_for_incident_severity(*left)
                    .cmp(status_label_for_incident_severity(*right))
            }
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
    tag: Option<RowTag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowTag {
    ProjectStatus(ProjectStatus),
    Setting(SettingKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnActionKind {
    Link,
    Drill,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrillRequest {
    ServiceLogForMaintenance(MaintenanceItemId),
    MaintenanceForAppliance(ApplianceId),
    QuotesForProject(ProjectId),
    QuotesForVendor(VendorId),
    ServiceLogForVendor(VendorId),
    DocumentsForEntity {
        kind: DocumentEntityKind,
        entity_id: i64,
    },
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
    sorts: Vec<SortSpec>,
    pin: Option<PinnedCell>,
    filter_active: bool,
    filter_inverted: bool,
    hidden_columns: BTreeSet<usize>,
    hide_settled_projects: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableCommand {
    MoveRow(isize),
    MoveColumn(isize),
    MoveHalfPageDown,
    MoveHalfPageUp,
    MoveFullPageDown,
    MoveFullPageUp,
    JumpFirstRow,
    JumpLastRow,
    JumpFirstColumn,
    JumpLastColumn,
    CycleSort,
    ClearSort,
    TogglePin,
    ToggleFilter,
    ToggleFilterInversion,
    ClearPins,
    ToggleSettledProjects,
    HideCurrentColumn,
    ShowAllColumns,
    OpenColumnFinder,
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
    FilterInvertedOn,
    FilterInvertedOff,
    SettledHidden,
    SettledShown,
    SettledUnavailable,
    ColumnHidden(&'static str),
    ColumnAlreadyHidden(&'static str),
    KeepOneColumnVisible,
    ColumnsShown,
    ColumnFinderOpen,
    ColumnFinderClosed,
    ColumnFinderNoMatches,
    ColumnFinderJumped(&'static str),
    ColumnFinderUnavailable,
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
            Self::FilterInvertedOn => "filter inverted on".to_owned(),
            Self::FilterInvertedOff => "filter inverted off".to_owned(),
            Self::SettledHidden => "settled hidden".to_owned(),
            Self::SettledShown => "settled shown".to_owned(),
            Self::SettledUnavailable => "settled toggle only on projects".to_owned(),
            Self::ColumnHidden(label) => format!("column hidden: {label}"),
            Self::ColumnAlreadyHidden(label) => format!("column already hidden: {label}"),
            Self::KeepOneColumnVisible => "keep one column visible".to_owned(),
            Self::ColumnsShown => "all columns shown".to_owned(),
            Self::ColumnFinderOpen => "column finder open".to_owned(),
            Self::ColumnFinderClosed => "column finder closed".to_owned(),
            Self::ColumnFinderNoMatches => "no columns match".to_owned(),
            Self::ColumnFinderJumped(label) => format!("column jump: {label}"),
            Self::ColumnFinderUnavailable => "column finder unavailable".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TableEvent {
    CursorUpdated,
    Status(TableStatus),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChatMessage {
    role: ChatRole,
    body: String,
    sql: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChatCommand {
    ToggleSql,
    Help,
    Models,
    Model(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatPipelineStage {
    Sql,
    Summary,
    Fallback,
}

impl ChatPipelineStage {
    const fn label(self) -> &'static str {
        match self {
            Self::Sql => "sql",
            Self::Summary => "summary",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChatInFlight {
    request_id: u64,
    assistant_index: usize,
    stage: ChatPipelineStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormChoiceKind {
    None,
    ProjectStatus,
    IncidentStatus,
    IncidentSeverity,
    DocumentEntityKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormFieldSpec {
    label: &'static str,
    choices: FormChoiceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormUiState {
    kind: FormKind,
    field_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ChatModelPickerUiState {
    visible: bool,
    query: String,
    matches: Vec<String>,
    cursor: usize,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct ChatUiState {
    input: String,
    show_sql: bool,
    history: Vec<String>,
    history_cursor: Option<usize>,
    history_buffer: String,
    transcript: Vec<ChatMessage>,
    model_picker: ChatModelPickerUiState,
    in_flight: Option<ChatInFlight>,
    next_request_id: u64,
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

#[derive(Debug, Clone, PartialEq, Default)]
struct ColumnFinderUiState {
    visible: bool,
    query: String,
    cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct NotePreviewUiState {
    visible: bool,
    title: String,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct DatePickerUiState {
    visible: bool,
    tab: Option<TabKind>,
    row_id: Option<i64>,
    column: usize,
    field_label: String,
    original: Option<Date>,
    selected: Option<Date>,
}

#[derive(Debug, Clone, PartialEq)]
struct DetailStackEntry {
    title: String,
    snapshot: Option<TabSnapshot>,
    table_state: TableUiState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingRowSelection {
    tab: TabKind,
    row_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InternalEvent {
    ClearStatus { token: u64 },
    ChatPipeline(ChatPipelineEvent),
}

#[derive(Debug, Clone, PartialEq, Default)]
struct ViewData {
    dashboard_counts: DashboardCounts,
    dashboard: DashboardUiState,
    column_finder: ColumnFinderUiState,
    note_preview: NotePreviewUiState,
    date_picker: DatePickerUiState,
    form: Option<FormUiState>,
    detail_stack: Vec<DetailStackEntry>,
    chat: ChatUiState,
    help_visible: bool,
    mag_mode: bool,
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
        process_internal_events(state, &mut view_data, &internal_tx, &internal_rx);

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
    tx: &Sender<InternalEvent>,
    rx: &Receiver<InternalEvent>,
) {
    while let Ok(event) = rx.try_recv() {
        match event {
            InternalEvent::ClearStatus { token } if token == view_data.status_token => {
                state.dispatch(AppCommand::ClearStatus);
            }
            InternalEvent::ClearStatus { .. } => {}
            InternalEvent::ChatPipeline(event) => {
                handle_chat_pipeline_event(state, view_data, tx, event);
            }
        }
    }
}

fn handle_chat_pipeline_event(
    state: &mut AppState,
    view_data: &mut ViewData,
    tx: &Sender<InternalEvent>,
    event: ChatPipelineEvent,
) {
    let Some(in_flight) = view_data.chat.in_flight else {
        return;
    };
    if event.request_id() != in_flight.request_id {
        return;
    }

    let Some(message) = view_data.chat.transcript.get_mut(in_flight.assistant_index) else {
        view_data.chat.in_flight = None;
        return;
    };

    match event {
        ChatPipelineEvent::SqlChunk { chunk, .. } => {
            let sql = message.sql.get_or_insert_with(String::new);
            sql.push_str(&chunk);
            view_data.chat.in_flight = Some(ChatInFlight {
                stage: ChatPipelineStage::Sql,
                ..in_flight
            });
        }
        ChatPipelineEvent::SqlReady { sql, .. } => {
            message.sql = Some(sql);
            view_data.chat.in_flight = Some(ChatInFlight {
                stage: ChatPipelineStage::Summary,
                ..in_flight
            });
        }
        ChatPipelineEvent::FallbackStarted { .. } => {
            view_data.chat.in_flight = Some(ChatInFlight {
                stage: ChatPipelineStage::Fallback,
                ..in_flight
            });
        }
        ChatPipelineEvent::AnswerChunk { chunk, .. } => {
            message.body.push_str(&chunk);
        }
        ChatPipelineEvent::Completed { result, .. } => {
            message.body = result.answer;
            message.sql = result.sql;
            if result.used_fallback {
                emit_status(
                    state,
                    view_data,
                    tx,
                    "fallback mode: answered from data snapshot",
                );
            }
            view_data.chat.in_flight = None;
        }
        ChatPipelineEvent::Failed { error, .. } => {
            let message_text = format!(
                "chat query failed: {error}; verify [llm] config, model availability, and server reachability"
            );
            message.body = message_text.clone();
            message.sql = None;
            view_data.chat.in_flight = None;
            emit_status(state, view_data, tx, message_text);
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

    if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
        view_data.mag_mode = !view_data.mag_mode;
        let status = if view_data.mag_mode {
            "mag on"
        } else {
            "mag off"
        };
        emit_status(state, view_data, internal_tx, status);
        return false;
    }

    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if cancel_in_flight_chat(runtime, view_data, true).is_some() {
            emit_status(state, view_data, internal_tx, "chat canceled");
        } else {
            emit_status(
                state,
                view_data,
                internal_tx,
                "cancel requested; no in-flight LLM operation",
            );
        }
        return false;
    }

    if view_data.help_visible {
        if key.code == KeyCode::Esc || key.code == KeyCode::Char('?') {
            view_data.help_visible = false;
            emit_status(state, view_data, internal_tx, "help hidden");
        }
        return false;
    }

    if view_data.date_picker.visible {
        handle_date_picker_key(state, view_data, internal_tx, key);
        return false;
    }

    if view_data.note_preview.visible {
        view_data.note_preview = NotePreviewUiState::default();
        return false;
    }

    if view_data.column_finder.visible {
        handle_column_finder_key(state, view_data, internal_tx, key);
        return false;
    }

    if state.chat == micasa_app::ChatVisibility::Visible {
        handle_chat_overlay_key(state, runtime, view_data, internal_tx, key);
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
                if !view_data.detail_stack.is_empty() {
                    emit_status(state, view_data, internal_tx, "close detail first");
                    return false;
                }
                close_all_detail_snapshots(view_data);
                dispatch_and_refresh(state, runtime, view_data, AppCommand::NextTab, internal_tx);
                return false;
            }
            (KeyCode::Char('b'), KeyModifiers::NONE) => {
                if !view_data.detail_stack.is_empty() {
                    emit_status(state, view_data, internal_tx, "close detail first");
                    return false;
                }
                close_all_detail_snapshots(view_data);
                dispatch_and_refresh(state, runtime, view_data, AppCommand::PrevTab, internal_tx);
                return false;
            }
            (KeyCode::Char('F'), _) => {
                if !view_data.detail_stack.is_empty() {
                    emit_status(state, view_data, internal_tx, "close detail first");
                    return false;
                }
                close_all_detail_snapshots(view_data);
                dispatch_and_refresh(state, runtime, view_data, AppCommand::LastTab, internal_tx);
                return false;
            }
            (KeyCode::Char('B'), _) => {
                if !view_data.detail_stack.is_empty() {
                    emit_status(state, view_data, internal_tx, "close detail first");
                    return false;
                }
                close_all_detail_snapshots(view_data);
                dispatch_and_refresh(state, runtime, view_data, AppCommand::FirstTab, internal_tx);
                return false;
            }
            (KeyCode::Char('@'), KeyModifiers::NONE) => {
                dispatch_and_refresh(state, runtime, view_data, AppCommand::OpenChat, internal_tx);
                if let Err(error) = ensure_chat_history_loaded(runtime, view_data) {
                    emit_status(
                        state,
                        view_data,
                        internal_tx,
                        format!(
                            "chat history load failed: {error}; check DB path/permissions and retry"
                        ),
                    );
                }
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
                if !view_data.detail_stack.is_empty() {
                    emit_status(state, view_data, internal_tx, "close detail first");
                    return false;
                }
                close_all_detail_snapshots(view_data);
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
                close_all_detail_snapshots(view_data);
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
                    if let Err(error) =
                        runtime.set_show_dashboard_preference(view_data.dashboard.visible)
                    {
                        emit_status(
                            state,
                            view_data,
                            internal_tx,
                            format!(
                                "dashboard pref save failed: {error}; verify DB permissions and retry"
                            ),
                        );
                        return false;
                    }
                    emit_status(state, view_data, internal_tx, "dashboard open");
                } else {
                    if let Err(error) =
                        runtime.set_show_dashboard_preference(view_data.dashboard.visible)
                    {
                        emit_status(
                            state,
                            view_data,
                            internal_tx,
                            format!(
                                "dashboard pref save failed: {error}; verify DB permissions and retry"
                            ),
                        );
                        return false;
                    }
                    emit_status(state, view_data, internal_tx, "dashboard hidden");
                }
            }
            (KeyCode::Esc, _) => {
                if pop_detail_snapshot(view_data) {
                    emit_status(state, view_data, internal_tx, "detail closed");
                } else {
                    state.dispatch(AppCommand::ClearStatus);
                }
            }
            (KeyCode::Char('d'), KeyModifiers::NONE) => {
                apply_table_command(view_data, TableCommand::MoveHalfPageDown);
            }
            (KeyCode::Char('u'), KeyModifiers::NONE) => {
                apply_table_command(view_data, TableCommand::MoveHalfPageUp);
            }
            (KeyCode::Enter, _) => {
                handle_nav_enter(state, runtime, view_data, internal_tx);
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
            (KeyCode::Char('x'), KeyModifiers::NONE) => {
                dispatch_and_refresh(
                    state,
                    runtime,
                    view_data,
                    AppCommand::ToggleDeleted,
                    internal_tx,
                );
            }
            (KeyCode::Char('a'), KeyModifiers::NONE) => {
                if let Some(form_kind) = form_for_tab(state.active_tab) {
                    open_form_with_template(state, runtime, view_data, internal_tx, form_kind);
                } else {
                    emit_status(state, view_data, internal_tx, "form unavailable");
                }
            }
            (KeyCode::Char('e'), KeyModifiers::NONE) => {
                handle_inline_edit_request(state, runtime, view_data, internal_tx);
            }
            (KeyCode::Char('p'), KeyModifiers::NONE) => {
                open_form_with_template(
                    state,
                    runtime,
                    view_data,
                    internal_tx,
                    FormKind::HouseProfile,
                );
            }
            (KeyCode::Char('d'), KeyModifiers::NONE) => {
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
            (KeyCode::Char('u'), KeyModifiers::NONE) => match runtime.undo_last_edit() {
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
            (KeyCode::Char('r'), KeyModifiers::NONE) => match runtime.redo_last_edit() {
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
            (KeyCode::Char('d'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                apply_table_command(view_data, TableCommand::MoveHalfPageDown);
            }
            (KeyCode::Char('u'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
                apply_table_command(view_data, TableCommand::MoveHalfPageUp);
            }
            (KeyCode::PageDown, _) => {
                apply_table_command(view_data, TableCommand::MoveFullPageDown);
            }
            (KeyCode::PageUp, _) => {
                apply_table_command(view_data, TableCommand::MoveFullPageUp);
            }
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
            (KeyCode::Tab, KeyModifiers::NONE) => {
                let status = move_form_field_cursor(state, view_data, 1);
                emit_status(state, view_data, internal_tx, status);
            }
            (KeyCode::BackTab, _) => {
                let status = move_form_field_cursor(state, view_data, -1);
                emit_status(state, view_data, internal_tx, status);
            }
            (KeyCode::Char(ch), KeyModifiers::NONE) if ('1'..='9').contains(&ch) => {
                let choice_index = usize::from(ch as u8 - b'1');
                let status = apply_form_choice(state, view_data, choice_index);
                emit_status(state, view_data, internal_tx, status);
            }
            _ => {}
        },
    }

    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InlineEditTarget {
    Setting(AppSetting),
    DatePicker,
    Form(FormKind),
    Unavailable,
}

fn handle_inline_edit_request<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
) {
    match resolve_inline_edit_target(state, view_data) {
        InlineEditTarget::Setting(setting) => {
            apply_setting_edit(state, runtime, view_data, internal_tx, setting)
        }
        InlineEditTarget::DatePicker => {
            let _ = open_inline_date_picker(state, view_data, internal_tx);
        }
        InlineEditTarget::Form(kind) => {
            open_form_with_template(state, runtime, view_data, internal_tx, kind);
        }
        InlineEditTarget::Unavailable => {
            emit_status(state, view_data, internal_tx, "edit unavailable");
        }
    }
}

fn resolve_inline_edit_target(state: &AppState, view_data: &ViewData) -> InlineEditTarget {
    if state.active_tab == TabKind::Settings {
        if let Some(setting) = selected_setting(view_data) {
            return InlineEditTarget::Setting(setting);
        }
        return InlineEditTarget::Unavailable;
    }

    if matches!(selected_cell(view_data), Some((_, TableCell::Date(_)))) {
        return InlineEditTarget::DatePicker;
    }

    if let Some(kind) = form_for_tab(state.active_tab) {
        return InlineEditTarget::Form(kind);
    }

    InlineEditTarget::Unavailable
}

fn selected_setting(view_data: &ViewData) -> Option<AppSetting> {
    let TabSnapshot::Settings(settings) = view_data.active_tab_snapshot.as_ref()? else {
        return None;
    };
    settings.get(view_data.table_state.selected_row).cloned()
}

fn apply_setting_edit<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    setting: AppSetting,
) {
    match setting.key {
        SettingKey::UiShowDashboard => {
            let SettingValue::Bool(current) = setting.value else {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    "settings value invalid; expected on/off",
                );
                return;
            };
            let next = !current;
            if let Err(error) = runtime.set_show_dashboard_preference(next) {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("save setting failed: {error}; verify DB permissions and retry"),
                );
                return;
            }
            if let Err(error) = refresh_view_data(state, runtime, view_data) {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("reload failed: {error}"),
                );
                return;
            }
            let status = if next {
                "dashboard startup on"
            } else {
                "dashboard startup off"
            };
            emit_status(state, view_data, internal_tx, status);
        }
        SettingKey::LlmModel => {
            let mut models = match runtime.list_chat_models() {
                Ok(models) => models,
                Err(error) => {
                    emit_status(
                        state,
                        view_data,
                        internal_tx,
                        format!(
                            "model list failed: {error}; verify LLM server and use /models for details"
                        ),
                    );
                    return;
                }
            };
            models.sort();
            models.dedup();
            if models.is_empty() {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    "no models available; run `ollama pull <model>` and retry",
                );
                return;
            }

            let current =
                runtime
                    .active_chat_model()
                    .ok()
                    .flatten()
                    .or_else(|| match setting.value {
                        SettingValue::Text(value) => {
                            let trimmed = value.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                Some(trimmed.to_owned())
                            }
                        }
                        SettingValue::Bool(_) => None,
                    });

            let next = match current
                .as_ref()
                .and_then(|model| models.iter().position(|entry| entry == model))
            {
                Some(index) => models[(index + 1) % models.len()].clone(),
                None => models[0].clone(),
            };

            if let Err(error) = runtime.select_chat_model(&next) {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("model select failed: {error}"),
                );
                return;
            }
            if let Err(error) = refresh_view_data(state, runtime, view_data) {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("reload failed: {error}"),
                );
                return;
            }
            emit_status(state, view_data, internal_tx, format!("llm model {next}"));
        }
    }
}

fn open_form_with_template<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    form_kind: FormKind,
) {
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
    sync_form_ui_state(state, view_data);
}

fn sync_form_ui_state(state: &AppState, view_data: &mut ViewData) {
    let AppMode::Form(kind) = state.mode else {
        view_data.form = None;
        return;
    };

    let fields = form_field_specs(kind);
    let max_index = fields.len().saturating_sub(1);
    let next_index = match view_data.form {
        Some(form) if form.kind == kind => form.field_index.min(max_index),
        _ => 0,
    };
    view_data.form = Some(FormUiState {
        kind,
        field_index: next_index,
    });
}

fn move_form_field_cursor(state: &AppState, view_data: &mut ViewData, delta: isize) -> String {
    sync_form_ui_state(state, view_data);
    let Some(mut form) = view_data.form else {
        return "form unavailable".to_owned();
    };
    let fields = form_field_specs(form.kind);
    if fields.is_empty() {
        return "form has no fields".to_owned();
    }

    let len = fields.len() as isize;
    let next = (form.field_index as isize + delta).rem_euclid(len) as usize;
    form.field_index = next;
    view_data.form = Some(form);
    format_form_field_status(form.kind, form.field_index)
}

fn apply_form_choice(
    state: &mut AppState,
    view_data: &mut ViewData,
    choice_index: usize,
) -> String {
    sync_form_ui_state(state, view_data);
    let Some(form) = view_data.form else {
        return "form unavailable".to_owned();
    };
    let fields = form_field_specs(form.kind);
    if fields.is_empty() {
        return "form has no fields".to_owned();
    }
    let spec = fields[form.field_index.min(fields.len().saturating_sub(1))];

    let Some(payload) = state.form_payload.clone() else {
        return "form payload missing".to_owned();
    };

    let selection_number = choice_index + 1;
    let (updated, status) = match spec.choices {
        FormChoiceKind::None => {
            return format!("no choices for {}", spec.label);
        }
        FormChoiceKind::ProjectStatus => {
            const PROJECT_STATUS_CHOICES: [ProjectStatus; 7] = [
                ProjectStatus::Ideating,
                ProjectStatus::Planned,
                ProjectStatus::Quoted,
                ProjectStatus::Underway,
                ProjectStatus::Delayed,
                ProjectStatus::Completed,
                ProjectStatus::Abandoned,
            ];
            let Some(choice) = PROJECT_STATUS_CHOICES.get(choice_index).copied() else {
                return format!("choice {selection_number} unavailable");
            };
            match payload {
                FormPayload::Project(mut input) => {
                    input.status = choice;
                    (
                        FormPayload::Project(input),
                        format!("project status {}", choice.as_str()),
                    )
                }
                _ => return "form field mismatch; reopen form".to_owned(),
            }
        }
        FormChoiceKind::IncidentStatus => {
            const INCIDENT_STATUS_CHOICES: [micasa_app::IncidentStatus; 3] = [
                micasa_app::IncidentStatus::Open,
                micasa_app::IncidentStatus::InProgress,
                micasa_app::IncidentStatus::Resolved,
            ];
            let Some(choice) = INCIDENT_STATUS_CHOICES.get(choice_index).copied() else {
                return format!("choice {selection_number} unavailable");
            };
            match payload {
                FormPayload::Incident(mut input) => {
                    input.status = choice;
                    (
                        FormPayload::Incident(input),
                        format!("incident status {}", choice.as_str()),
                    )
                }
                _ => return "form field mismatch; reopen form".to_owned(),
            }
        }
        FormChoiceKind::IncidentSeverity => {
            const INCIDENT_SEVERITY_CHOICES: [IncidentSeverity; 3] = [
                IncidentSeverity::Urgent,
                IncidentSeverity::Soon,
                IncidentSeverity::Whenever,
            ];
            let Some(choice) = INCIDENT_SEVERITY_CHOICES.get(choice_index).copied() else {
                return format!("choice {selection_number} unavailable");
            };
            match payload {
                FormPayload::Incident(mut input) => {
                    input.severity = choice;
                    (
                        FormPayload::Incident(input),
                        format!("incident severity {}", choice.as_str()),
                    )
                }
                _ => return "form field mismatch; reopen form".to_owned(),
            }
        }
        FormChoiceKind::DocumentEntityKind => {
            const DOCUMENT_KIND_CHOICES: [DocumentEntityKind; 8] = [
                DocumentEntityKind::None,
                DocumentEntityKind::Project,
                DocumentEntityKind::Quote,
                DocumentEntityKind::Maintenance,
                DocumentEntityKind::Appliance,
                DocumentEntityKind::ServiceLog,
                DocumentEntityKind::Vendor,
                DocumentEntityKind::Incident,
            ];
            let Some(choice) = DOCUMENT_KIND_CHOICES.get(choice_index).copied() else {
                return format!("choice {selection_number} unavailable");
            };
            match payload {
                FormPayload::Document(mut input) => {
                    input.entity_kind = choice;
                    (
                        FormPayload::Document(input),
                        format!("entity {}", choice.as_str()),
                    )
                }
                _ => return "form field mismatch; reopen form".to_owned(),
            }
        }
    };

    let _events = state.dispatch(AppCommand::SetFormPayload(updated));
    status
}

fn format_form_field_status(kind: FormKind, index: usize) -> String {
    let fields = form_field_specs(kind);
    if fields.is_empty() {
        return "form has no fields".to_owned();
    }
    let field = fields[index.min(fields.len().saturating_sub(1))];
    format!("field {} ({}/{})", field.label, index + 1, fields.len())
}

fn form_field_specs(kind: FormKind) -> &'static [FormFieldSpec] {
    match kind {
        FormKind::HouseProfile => &[
            FormFieldSpec {
                label: "nickname",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "city",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "state",
                choices: FormChoiceKind::None,
            },
        ],
        FormKind::Project => &[
            FormFieldSpec {
                label: "title",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "type",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "status",
                choices: FormChoiceKind::ProjectStatus,
            },
            FormFieldSpec {
                label: "budget",
                choices: FormChoiceKind::None,
            },
        ],
        FormKind::Quote => &[
            FormFieldSpec {
                label: "project",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "vendor",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "total",
                choices: FormChoiceKind::None,
            },
        ],
        FormKind::MaintenanceItem => &[
            FormFieldSpec {
                label: "item",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "category",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "interval",
                choices: FormChoiceKind::None,
            },
        ],
        FormKind::ServiceLogEntry => &[
            FormFieldSpec {
                label: "item",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "date",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "vendor",
                choices: FormChoiceKind::None,
            },
        ],
        FormKind::Incident => &[
            FormFieldSpec {
                label: "title",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "status",
                choices: FormChoiceKind::IncidentStatus,
            },
            FormFieldSpec {
                label: "severity",
                choices: FormChoiceKind::IncidentSeverity,
            },
            FormFieldSpec {
                label: "noticed",
                choices: FormChoiceKind::None,
            },
        ],
        FormKind::Appliance => &[
            FormFieldSpec {
                label: "name",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "brand",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "location",
                choices: FormChoiceKind::None,
            },
        ],
        FormKind::Vendor => &[
            FormFieldSpec {
                label: "name",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "contact",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "email",
                choices: FormChoiceKind::None,
            },
        ],
        FormKind::Document => &[
            FormFieldSpec {
                label: "title",
                choices: FormChoiceKind::None,
            },
            FormFieldSpec {
                label: "entity",
                choices: FormChoiceKind::DocumentEntityKind,
            },
            FormFieldSpec {
                label: "file",
                choices: FormChoiceKind::None,
            },
        ],
    }
}

fn open_inline_date_picker(
    state: &mut AppState,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
) -> bool {
    let Some((column, value)) = selected_cell(view_data) else {
        emit_status(state, view_data, internal_tx, "no cell selected");
        return false;
    };

    let TableCell::Date(original) = value else {
        return false;
    };

    let selected = original.unwrap_or_else(|| OffsetDateTime::now_utc().date());
    let label = active_projection(view_data)
        .and_then(|projection| projection.columns.get(column).copied())
        .unwrap_or("date")
        .to_owned();

    view_data.date_picker = DatePickerUiState {
        visible: true,
        tab: view_data.table_state.tab,
        row_id: selected_row_metadata(view_data).map(|(row_id, _)| row_id),
        column,
        field_label: label,
        original,
        selected: Some(selected),
    };
    emit_status(state, view_data, internal_tx, "date picker open");
    true
}

fn handle_date_picker_key(
    state: &mut AppState,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    key: KeyEvent,
) {
    let Some(current) = view_data.date_picker.selected else {
        view_data.date_picker = DatePickerUiState::default();
        return;
    };

    let next = match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            view_data.date_picker = DatePickerUiState::default();
            emit_status(state, view_data, internal_tx, "date edit canceled");
            return;
        }
        (KeyCode::Enter, _) => {
            let picked = current.to_string();
            view_data.date_picker = DatePickerUiState::default();
            emit_status(
                state,
                view_data,
                internal_tx,
                format!("date picked {picked}; open full form to persist"),
            );
            return;
        }
        (KeyCode::Char('h'), _) | (KeyCode::Left, _) => shift_date_by_days(current, -1),
        (KeyCode::Char('l'), _) | (KeyCode::Right, _) => shift_date_by_days(current, 1),
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => shift_date_by_days(current, 7),
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => shift_date_by_days(current, -7),
        (KeyCode::Char('H'), _) => shift_date_by_months(current, -1),
        (KeyCode::Char('L'), _) => shift_date_by_months(current, 1),
        (KeyCode::Char('['), _) => shift_date_by_years(current, -1),
        (KeyCode::Char(']'), _) => shift_date_by_years(current, 1),
        _ => None,
    };

    if let Some(date) = next {
        view_data.date_picker.selected = Some(date);
    }
}

fn shift_date_by_days(date: Date, days: i64) -> Option<Date> {
    date.checked_add(time::Duration::days(days))
}

fn shift_date_by_years(date: Date, years: i32) -> Option<Date> {
    shift_date_by_months(date, years.saturating_mul(12))
}

fn shift_date_by_months(date: Date, months: i32) -> Option<Date> {
    let base_month = i32::from(date.month() as u8);
    let total_month = base_month - 1 + months;
    let year = date.year() + total_month.div_euclid(12);
    let month_number = (total_month.rem_euclid(12) + 1) as u8;
    let month = Month::try_from(month_number).ok()?;
    let day = date.day();
    let max_day = last_day_of_month(year, month)?;
    let clamped_day = day.min(max_day);
    Date::from_calendar_date(year, month, clamped_day).ok()
}

fn last_day_of_month(year: i32, month: Month) -> Option<u8> {
    let (next_year, next_month) = if month == Month::December {
        (year + 1, Month::January)
    } else {
        let next = Month::try_from((month as u8) + 1).ok()?;
        (year, next)
    };

    let first_next_month = Date::from_calendar_date(next_year, next_month, 1).ok()?;
    let last = first_next_month - time::Duration::days(1);
    Some(last.day())
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
                close_all_detail_snapshots(view_data);
                view_data.dashboard.visible = false;
                if let Err(error) = runtime.set_show_dashboard_preference(false) {
                    emit_status(
                        state,
                        view_data,
                        internal_tx,
                        format!(
                            "dashboard pref save failed: {error}; verify DB permissions and retry"
                        ),
                    );
                    return true;
                }
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
            if let Err(error) = runtime.set_show_dashboard_preference(false) {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("dashboard pref save failed: {error}; verify DB permissions and retry"),
                );
                return true;
            }
            emit_status(state, view_data, internal_tx, "dashboard hidden");
        }
        (KeyCode::Char('f'), _) => {
            view_data.dashboard.visible = false;
            if let Err(error) = runtime.set_show_dashboard_preference(false) {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("dashboard pref save failed: {error}; verify DB permissions and retry"),
                );
                return true;
            }
            dispatch_and_refresh(state, runtime, view_data, AppCommand::NextTab, internal_tx);
        }
        (KeyCode::Char('b'), _) => {
            view_data.dashboard.visible = false;
            if let Err(error) = runtime.set_show_dashboard_preference(false) {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("dashboard pref save failed: {error}; verify DB permissions and retry"),
                );
                return true;
            }
            dispatch_and_refresh(state, runtime, view_data, AppCommand::PrevTab, internal_tx);
        }
        (KeyCode::Char('?'), _) => {
            view_data.help_visible = true;
        }
        _ => {}
    }

    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColumnFinderMatch {
    column: usize,
    label: &'static str,
    hidden: bool,
}

fn handle_column_finder_key(
    state: &mut AppState,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    key: KeyEvent,
) {
    let mut close_finder = false;
    let mut emit = None::<TableStatus>;

    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            close_finder = true;
            emit = Some(TableStatus::ColumnFinderClosed);
        }
        (KeyCode::Up, _) => {
            view_data.column_finder.cursor = view_data.column_finder.cursor.saturating_sub(1);
        }
        (KeyCode::Char('p'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            view_data.column_finder.cursor = view_data.column_finder.cursor.saturating_sub(1);
        }
        (KeyCode::Down, _) => {
            view_data.column_finder.cursor = view_data.column_finder.cursor.saturating_add(1);
        }
        (KeyCode::Char('n'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            view_data.column_finder.cursor = view_data.column_finder.cursor.saturating_add(1);
        }
        (KeyCode::Backspace, _) => {
            view_data.column_finder.query.pop();
        }
        (KeyCode::Char('u'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            view_data.column_finder.query.clear();
        }
        (KeyCode::Char(ch), modifiers)
            if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
        {
            view_data.column_finder.query.push(ch);
        }
        (KeyCode::Enter, _) => {
            if let Some(projection) = active_projection(view_data) {
                let matches = column_finder_matches(
                    &projection,
                    &view_data.table_state.hidden_columns,
                    &view_data.column_finder.query,
                );
                if matches.is_empty() {
                    emit = Some(TableStatus::ColumnFinderNoMatches);
                } else {
                    let selected = matches[view_data.column_finder.cursor.min(matches.len() - 1)];
                    view_data
                        .table_state
                        .hidden_columns
                        .remove(&selected.column);
                    view_data.table_state.selected_col = selected.column;
                    clamp_table_cursor(view_data);
                    close_finder = true;
                    emit = Some(TableStatus::ColumnFinderJumped(selected.label));
                }
            } else {
                close_finder = true;
                emit = Some(TableStatus::ColumnFinderUnavailable);
            }
        }
        _ => {}
    }

    if close_finder {
        view_data.column_finder = ColumnFinderUiState::default();
    } else if let Some(projection) = active_projection(view_data) {
        let matches = column_finder_matches(
            &projection,
            &view_data.table_state.hidden_columns,
            &view_data.column_finder.query,
        );
        if matches.is_empty() {
            view_data.column_finder.cursor = 0;
        } else {
            view_data.column_finder.cursor = view_data
                .column_finder
                .cursor
                .min(matches.len().saturating_sub(1));
        }
    }

    if let Some(status) = emit {
        emit_status(state, view_data, internal_tx, status.message());
    }
}

fn open_column_finder(view_data: &mut ViewData) -> TableStatus {
    let Some(projection) = active_projection(view_data) else {
        return TableStatus::ColumnFinderUnavailable;
    };
    if projection.column_count() == 0 {
        return TableStatus::ColumnFinderUnavailable;
    }

    view_data.column_finder.visible = true;
    view_data.column_finder.query.clear();
    let matches = column_finder_matches(&projection, &view_data.table_state.hidden_columns, "");
    view_data.column_finder.cursor = matches
        .iter()
        .position(|entry| entry.column == view_data.table_state.selected_col)
        .unwrap_or(0);

    TableStatus::ColumnFinderOpen
}

fn column_finder_matches(
    projection: &TableProjection,
    hidden_columns: &BTreeSet<usize>,
    query: &str,
) -> Vec<ColumnFinderMatch> {
    projection
        .columns
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, label)| {
            if column_label_matches_query(label, query) {
                Some(ColumnFinderMatch {
                    column: index,
                    label,
                    hidden: hidden_columns.contains(&index),
                })
            } else {
                None
            }
        })
        .collect()
}

fn column_label_matches_query(label: &str, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let mut needle = query.chars().filter(|ch| !ch.is_whitespace());
    let mut target = needle.next();
    if target.is_none() {
        return true;
    }

    for label_char in label.chars() {
        let Some(needle_char) = target else {
            break;
        };
        if label_char.eq_ignore_ascii_case(&needle_char) {
            target = needle.next();
            if target.is_none() {
                return true;
            }
        }
    }
    false
}

fn push_detail_snapshot(view_data: &mut ViewData, title: impl Into<String>, snapshot: TabSnapshot) {
    view_data.detail_stack.push(DetailStackEntry {
        title: title.into(),
        snapshot: view_data.active_tab_snapshot.clone(),
        table_state: view_data.table_state.clone(),
    });
    let detail_state = TableUiState {
        tab: Some(snapshot.tab_kind()),
        ..TableUiState::default()
    };
    view_data.active_tab_snapshot = Some(snapshot);
    view_data.table_state = detail_state;
    view_data.column_finder = ColumnFinderUiState::default();
    view_data.note_preview = NotePreviewUiState::default();
    view_data.date_picker = DatePickerUiState::default();
    clamp_table_cursor(view_data);
}

fn pop_detail_snapshot(view_data: &mut ViewData) -> bool {
    let Some(previous) = view_data.detail_stack.pop() else {
        return false;
    };
    view_data.active_tab_snapshot = previous.snapshot;
    view_data.table_state = previous.table_state;
    view_data.column_finder = ColumnFinderUiState::default();
    view_data.note_preview = NotePreviewUiState::default();
    view_data.date_picker = DatePickerUiState::default();
    clamp_table_cursor(view_data);
    true
}

fn close_all_detail_snapshots(view_data: &mut ViewData) {
    while pop_detail_snapshot(view_data) {}
}

fn filter_snapshot_for_drill(snapshot: TabSnapshot, request: DrillRequest) -> TabSnapshot {
    match (snapshot, request) {
        (TabSnapshot::ServiceLog(rows), DrillRequest::ServiceLogForMaintenance(item_id)) => {
            TabSnapshot::ServiceLog(
                rows.into_iter()
                    .filter(|row| row.maintenance_item_id == item_id)
                    .collect(),
            )
        }
        (TabSnapshot::ServiceLog(rows), DrillRequest::ServiceLogForVendor(vendor_id)) => {
            TabSnapshot::ServiceLog(
                rows.into_iter()
                    .filter(|row| row.vendor_id == Some(vendor_id))
                    .collect(),
            )
        }
        (TabSnapshot::Maintenance(rows), DrillRequest::MaintenanceForAppliance(appliance_id)) => {
            TabSnapshot::Maintenance(
                rows.into_iter()
                    .filter(|row| row.appliance_id == Some(appliance_id))
                    .collect(),
            )
        }
        (TabSnapshot::Quotes(rows), DrillRequest::QuotesForProject(project_id)) => {
            TabSnapshot::Quotes(
                rows.into_iter()
                    .filter(|row| row.project_id == project_id)
                    .collect(),
            )
        }
        (TabSnapshot::Quotes(rows), DrillRequest::QuotesForVendor(vendor_id)) => {
            TabSnapshot::Quotes(
                rows.into_iter()
                    .filter(|row| row.vendor_id == vendor_id)
                    .collect(),
            )
        }
        (TabSnapshot::Documents(rows), DrillRequest::DocumentsForEntity { kind, entity_id }) => {
            TabSnapshot::Documents(
                rows.into_iter()
                    .filter(|row| row.entity_kind == kind && row.entity_id == entity_id)
                    .collect(),
            )
        }
        (snapshot, _) => snapshot,
    }
}

fn ensure_chat_history_loaded<R: AppRuntime>(
    runtime: &mut R,
    view_data: &mut ViewData,
) -> Result<()> {
    if view_data.chat.history.is_empty() {
        view_data.chat.history = runtime.load_chat_history()?;
        view_data.chat.history_cursor = None;
        view_data.chat.history_buffer.clear();
    }
    Ok(())
}

fn handle_chat_overlay_key<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    key: KeyEvent,
) {
    if handle_chat_model_picker_key(state, runtime, view_data, internal_tx, key) {
        return;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            if cancel_in_flight_chat(runtime, view_data, true).is_some() {
                emit_status(state, view_data, internal_tx, "chat canceled");
            }
            view_data.chat.model_picker = ChatModelPickerUiState::default();
            dispatch_and_refresh(
                state,
                runtime,
                view_data,
                AppCommand::CloseChat,
                internal_tx,
            );
            return;
        }
        (KeyCode::Char('s'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            view_data.chat.show_sql = !view_data.chat.show_sql;
            if view_data.chat.show_sql {
                emit_status(state, view_data, internal_tx, "chat sql on");
            } else {
                emit_status(state, view_data, internal_tx, "chat sql off");
            }
            return;
        }
        (KeyCode::Up, _) => chat_history_prev(view_data),
        (KeyCode::Char('p'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            chat_history_prev(view_data);
        }
        (KeyCode::Down, _) => chat_history_next(view_data),
        (KeyCode::Char('n'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            chat_history_next(view_data);
        }
        (KeyCode::Enter, _) => submit_chat_input(state, runtime, view_data, internal_tx),
        (KeyCode::Backspace, _) => {
            view_data.chat.input.pop();
            view_data.chat.history_cursor = None;
        }
        (KeyCode::Char(ch), modifiers) => {
            if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT {
                view_data.chat.input.push(ch);
                view_data.chat.history_cursor = None;
            }
        }
        _ => {}
    }

    refresh_chat_model_picker(runtime, view_data);
}

fn handle_chat_model_picker_key<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
    key: KeyEvent,
) -> bool {
    if !view_data.chat.model_picker.visible {
        return false;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Up, _) => {
            view_data.chat.model_picker.cursor =
                view_data.chat.model_picker.cursor.saturating_sub(1);
            true
        }
        (KeyCode::Char('p'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            view_data.chat.model_picker.cursor =
                view_data.chat.model_picker.cursor.saturating_sub(1);
            true
        }
        (KeyCode::Down, _) => {
            let max = view_data.chat.model_picker.matches.len().saturating_sub(1);
            view_data.chat.model_picker.cursor = (view_data.chat.model_picker.cursor + 1).min(max);
            true
        }
        (KeyCode::Char('n'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            let max = view_data.chat.model_picker.matches.len().saturating_sub(1);
            view_data.chat.model_picker.cursor = (view_data.chat.model_picker.cursor + 1).min(max);
            true
        }
        (KeyCode::Esc, _) => {
            view_data.chat.model_picker = ChatModelPickerUiState::default();
            emit_status(state, view_data, internal_tx, "model picker hidden");
            true
        }
        (KeyCode::Enter, _) => {
            let Some(model) = view_data
                .chat
                .model_picker
                .matches
                .get(view_data.chat.model_picker.cursor)
                .cloned()
            else {
                emit_status(state, view_data, internal_tx, "no model match to select");
                return true;
            };
            view_data.chat.model_picker = ChatModelPickerUiState::default();
            view_data.chat.input = format!("/model {model}");
            submit_chat_input(state, runtime, view_data, internal_tx);
            true
        }
        _ => false,
    }
}

fn refresh_chat_model_picker<R: AppRuntime>(runtime: &mut R, view_data: &mut ViewData) {
    let Some(raw_query) = view_data.chat.input.strip_prefix("/model ") else {
        view_data.chat.model_picker = ChatModelPickerUiState::default();
        return;
    };

    view_data.chat.model_picker.visible = true;
    view_data.chat.model_picker.query = raw_query.to_owned();
    view_data.chat.model_picker.error = None;

    match runtime.list_chat_models() {
        Ok(models) => {
            let query = raw_query.trim();
            let mut matches = models
                .into_iter()
                .filter(|model| chat_model_matches_query(model, query))
                .collect::<Vec<_>>();
            matches.sort();
            view_data.chat.model_picker.matches = matches;
            if view_data.chat.model_picker.matches.is_empty() {
                view_data.chat.model_picker.cursor = 0;
            } else {
                view_data.chat.model_picker.cursor = view_data
                    .chat
                    .model_picker
                    .cursor
                    .min(view_data.chat.model_picker.matches.len().saturating_sub(1));
            }
        }
        Err(error) => {
            view_data.chat.model_picker.matches.clear();
            view_data.chat.model_picker.cursor = 0;
            view_data.chat.model_picker.error = Some(format!("model list failed: {error}"));
        }
    }
}

fn chat_model_matches_query(model: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let model_lc = model.to_ascii_lowercase();
    let query_lc = query.to_ascii_lowercase();
    if model_lc.contains(&query_lc) {
        return true;
    }

    let mut query_chars = query_lc.chars();
    let mut current = query_chars.next();
    for ch in model_lc.chars() {
        let Some(needle) = current else {
            return true;
        };
        if ch == needle {
            current = query_chars.next();
        }
    }
    current.is_none()
}

fn submit_chat_input<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
) {
    let input = view_data.chat.input.trim().to_owned();
    if input.is_empty() {
        return;
    }

    view_data.chat.input.clear();
    view_data.chat.history_cursor = None;
    view_data.chat.history_buffer.clear();
    view_data.chat.model_picker = ChatModelPickerUiState::default();

    if view_data.chat.history.last() != Some(&input) {
        view_data.chat.history.push(input.clone());
    }

    if let Err(error) = runtime.append_chat_input(&input) {
        emit_status(
            state,
            view_data,
            internal_tx,
            format!("chat history save failed: {error}; check DB permissions and retry"),
        );
    }

    view_data.chat.transcript.push(ChatMessage {
        role: ChatRole::User,
        body: input.clone(),
        sql: None,
    });

    if let Some(command) = parse_chat_command(&input) {
        match command {
            ChatCommand::ToggleSql => {
                view_data.chat.show_sql = !view_data.chat.show_sql;
                let status = if view_data.chat.show_sql {
                    "chat sql on"
                } else {
                    "chat sql off"
                };
                emit_status(state, view_data, internal_tx, status);
            }
            ChatCommand::Help => {
                view_data.chat.transcript.push(ChatMessage {
                    role: ChatRole::Assistant,
                    body: "/help, /models, /model <name>, /sql".to_owned(),
                    sql: None,
                });
            }
            ChatCommand::Models => {
                let active = runtime.active_chat_model();
                match runtime.list_chat_models() {
                    Ok(models) => {
                        let active_model = active.unwrap_or(None);
                        view_data.chat.transcript.push(ChatMessage {
                            role: ChatRole::Assistant,
                            body: render_model_list_message(&models, active_model.as_deref()),
                            sql: None,
                        });
                    }
                    Err(error) => {
                        view_data.chat.transcript.push(ChatMessage {
                            role: ChatRole::Assistant,
                            body: format!("model list failed: {error}"),
                            sql: None,
                        });
                    }
                }
            }
            ChatCommand::Model(model) => match runtime.select_chat_model(&model) {
                Ok(()) => {
                    view_data.chat.transcript.push(ChatMessage {
                        role: ChatRole::Assistant,
                        body: format!("model set: {model}"),
                        sql: None,
                    });
                    emit_status(state, view_data, internal_tx, format!("model {model}"));
                }
                Err(error) => {
                    view_data.chat.transcript.push(ChatMessage {
                        role: ChatRole::Assistant,
                        body: format!("model switch failed: {error}"),
                        sql: None,
                    });
                }
            },
        }
        return;
    }

    if cancel_in_flight_chat(runtime, view_data, true).is_some() {
        emit_status(state, view_data, internal_tx, "prior chat canceled");
    }

    let history = build_chat_pipeline_history(&view_data.chat.transcript);
    let request_id = next_chat_request_id(&mut view_data.chat);
    view_data.chat.transcript.push(ChatMessage {
        role: ChatRole::Assistant,
        body: String::new(),
        sql: None,
    });
    let assistant_index = view_data.chat.transcript.len().saturating_sub(1);
    view_data.chat.in_flight = Some(ChatInFlight {
        request_id,
        assistant_index,
        stage: ChatPipelineStage::Sql,
    });

    if let Err(error) =
        runtime.spawn_chat_pipeline(request_id, &input, &history, internal_tx.clone())
    {
        let message = format!(
            "chat query failed: {error}; verify [llm] config, model availability, and server reachability"
        );
        if let Some(in_flight) = view_data.chat.in_flight.take()
            && let Some(response) = view_data.chat.transcript.get_mut(in_flight.assistant_index)
        {
            response.body = message.clone();
            response.sql = None;
        }
        emit_status(state, view_data, internal_tx, message);
    }
}

fn build_chat_pipeline_history(transcript: &[ChatMessage]) -> Vec<ChatHistoryMessage> {
    if transcript.is_empty() {
        return Vec::new();
    }

    let keep = transcript.len().saturating_sub(1);
    transcript
        .iter()
        .take(keep)
        .filter_map(|message| {
            let content = message.body.trim();
            if content.is_empty() {
                return None;
            }

            let role = match message.role {
                ChatRole::User => ChatHistoryRole::User,
                ChatRole::Assistant => ChatHistoryRole::Assistant,
            };
            Some(ChatHistoryMessage {
                role,
                content: content.to_owned(),
            })
        })
        .collect()
}

fn next_chat_request_id(chat: &mut ChatUiState) -> u64 {
    chat.next_request_id = chat.next_request_id.saturating_add(1);
    if chat.next_request_id == 0 {
        chat.next_request_id = 1;
    }
    chat.next_request_id
}

fn cancel_in_flight_chat<R: AppRuntime>(
    runtime: &mut R,
    view_data: &mut ViewData,
    annotate_partial: bool,
) -> Option<u64> {
    let in_flight = view_data.chat.in_flight.take()?;
    let _ = runtime.cancel_chat_pipeline(in_flight.request_id);

    if in_flight.assistant_index < view_data.chat.transcript.len() {
        let response = &mut view_data.chat.transcript[in_flight.assistant_index];
        let has_body = !response.body.trim().is_empty();
        let has_sql = response
            .sql
            .as_ref()
            .map(|sql| !sql.trim().is_empty())
            .unwrap_or(false);

        if !has_body && !has_sql {
            view_data.chat.transcript.remove(in_flight.assistant_index);
        } else if annotate_partial {
            let body = response.body.trim_end();
            if body.is_empty() {
                response.body = "(interrupted)".to_owned();
            } else {
                response.body = format!("{body}\n(interrupted)");
            }
        }
    }

    Some(in_flight.request_id)
}

fn parse_chat_command(input: &str) -> Option<ChatCommand> {
    if input == "/sql" {
        return Some(ChatCommand::ToggleSql);
    }
    if input == "/help" {
        return Some(ChatCommand::Help);
    }
    if input == "/models" {
        return Some(ChatCommand::Models);
    }
    if let Some(model) = input.strip_prefix("/model") {
        return Some(ChatCommand::Model(model.trim().to_owned()));
    }
    None
}

fn render_model_list_message(models: &[String], active_model: Option<&str>) -> String {
    if models.is_empty() {
        return "no models reported by server; pull one first (`ollama pull <name>`)".to_owned();
    }

    let mut lines = Vec::with_capacity(models.len() + 1);
    lines.push("models:".to_owned());
    for model in models {
        let marker = if active_model == Some(model.as_str()) {
            "*"
        } else {
            "-"
        };
        lines.push(format!("{marker} {model}"));
    }
    lines.join("\n")
}

fn chat_history_prev(view_data: &mut ViewData) {
    if view_data.chat.history.is_empty() {
        return;
    }

    match view_data.chat.history_cursor {
        None => {
            view_data.chat.history_buffer = view_data.chat.input.clone();
            view_data.chat.history_cursor = Some(view_data.chat.history.len().saturating_sub(1));
        }
        Some(cursor) if cursor > 0 => {
            view_data.chat.history_cursor = Some(cursor - 1);
        }
        Some(_) => {}
    }

    if let Some(cursor) = view_data.chat.history_cursor {
        view_data.chat.input = view_data.chat.history[cursor].clone();
    }
}

fn chat_history_next(view_data: &mut ViewData) {
    let Some(cursor) = view_data.chat.history_cursor else {
        return;
    };

    if cursor + 1 < view_data.chat.history.len() {
        let next = cursor + 1;
        view_data.chat.history_cursor = Some(next);
        view_data.chat.input = view_data.chat.history[next].clone();
    } else {
        view_data.chat.history_cursor = None;
        view_data.chat.input = view_data.chat.history_buffer.clone();
        view_data.chat.history_buffer.clear();
    }
}

fn handle_nav_enter<R: AppRuntime>(
    state: &mut AppState,
    runtime: &mut R,
    view_data: &mut ViewData,
    internal_tx: &Sender<InternalEvent>,
) {
    let Some(tab) = view_data.table_state.tab else {
        return;
    };
    let row_id = selected_row_metadata(view_data).map(|(id, _)| id);
    let Some((column, value)) = selected_cell(view_data) else {
        return;
    };

    if is_note_preview_column(tab, column) {
        if let TableCell::Text(text) = value {
            if text.trim().is_empty() {
                emit_status(state, view_data, internal_tx, "no note to preview");
                return;
            }
            view_data.note_preview.visible = true;
            view_data.note_preview.title = note_preview_title(tab).to_owned();
            view_data.note_preview.text = text;
        } else {
            emit_status(state, view_data, internal_tx, "no note to preview");
        }
        return;
    }

    if let Some(row_id) = row_id
        && let Some(request) = drill_request_for(tab, column, row_id)
    {
        let target_tab = match request {
            DrillRequest::ServiceLogForMaintenance(_) => TabKind::ServiceLog,
            DrillRequest::ServiceLogForVendor(_) => TabKind::ServiceLog,
            DrillRequest::MaintenanceForAppliance(_) => TabKind::Maintenance,
            DrillRequest::QuotesForProject(_) => TabKind::Quotes,
            DrillRequest::QuotesForVendor(_) => TabKind::Quotes,
            DrillRequest::DocumentsForEntity { .. } => TabKind::Documents,
        };
        match runtime.load_tab_snapshot(target_tab, state.show_deleted) {
            Ok(Some(snapshot)) => {
                let filtered = filter_snapshot_for_drill(snapshot, request);
                let title = drill_title_for(tab, selected_row_label(view_data), request);
                push_detail_snapshot(view_data, title, filtered);
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("drill {}", target_tab.label()),
                );
            }
            Ok(None) => {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("drill unavailable for {}", target_tab.label()),
                );
            }
            Err(error) => {
                emit_status(
                    state,
                    view_data,
                    internal_tx,
                    format!("drill load failed: {error}; verify DB and retry"),
                );
            }
        }
        return;
    }

    let Some(target_tab) = linked_tab_for_column(tab, column) else {
        emit_status(state, view_data, internal_tx, "press i to edit");
        return;
    };

    let Some(target_row_id) = link_target_id(&value) else {
        emit_status(state, view_data, internal_tx, "nothing to follow");
        return;
    };

    close_all_detail_snapshots(view_data);
    view_data.pending_row_selection = Some(PendingRowSelection {
        tab: target_tab,
        row_id: target_row_id,
    });
    dispatch_and_refresh(
        state,
        runtime,
        view_data,
        AppCommand::SetActiveTab(target_tab),
        internal_tx,
    );

    let selected_target = view_data.table_state.tab == Some(target_tab)
        && selected_row_metadata(view_data)
            .map(|(row_id, _)| row_id == target_row_id)
            .unwrap_or(false);
    if selected_target {
        emit_status(
            state,
            view_data,
            internal_tx,
            format!("follow -> {}", target_tab.label()),
        );
    } else {
        emit_status(
            state,
            view_data,
            internal_tx,
            format!(
                "linked item {target_row_id} not found in {}; enter edit mode (`i`), toggle deleted (`x`), retry",
                target_tab.label()
            ),
        );
    }
}

fn drill_request_for(tab: TabKind, column: usize, row_id: i64) -> Option<DrillRequest> {
    if row_id <= 0 {
        return None;
    }
    match (tab, column) {
        (TabKind::Projects, 5) => Some(DrillRequest::QuotesForProject(ProjectId::new(row_id))),
        (TabKind::Projects, 6) => Some(DrillRequest::DocumentsForEntity {
            kind: DocumentEntityKind::Project,
            entity_id: row_id,
        }),
        (TabKind::Maintenance, 7) => Some(DrillRequest::ServiceLogForMaintenance(
            MaintenanceItemId::new(row_id),
        )),
        (TabKind::Incidents, 7) => Some(DrillRequest::DocumentsForEntity {
            kind: DocumentEntityKind::Incident,
            entity_id: row_id,
        }),
        (TabKind::Appliances, 6) => Some(DrillRequest::MaintenanceForAppliance(ApplianceId::new(
            row_id,
        ))),
        (TabKind::Appliances, 7) => Some(DrillRequest::DocumentsForEntity {
            kind: DocumentEntityKind::Appliance,
            entity_id: row_id,
        }),
        (TabKind::Vendors, 5) => Some(DrillRequest::QuotesForVendor(VendorId::new(row_id))),
        (TabKind::Vendors, 6) => Some(DrillRequest::ServiceLogForVendor(VendorId::new(row_id))),
        _ => None,
    }
}

fn drill_title_for(tab: TabKind, selected_label: String, request: DrillRequest) -> String {
    let label = selected_label.trim();
    match (tab, request) {
        (TabKind::Maintenance, DrillRequest::ServiceLogForMaintenance(_)) => {
            if label.is_empty() {
                "service log".to_owned()
            } else {
                format!("service log ({label})")
            }
        }
        (TabKind::Appliances, DrillRequest::MaintenanceForAppliance(_)) => {
            if label.is_empty() {
                "maintenance".to_owned()
            } else {
                format!("maintenance ({label})")
            }
        }
        (TabKind::Projects, DrillRequest::QuotesForProject(_))
        | (TabKind::Vendors, DrillRequest::QuotesForVendor(_)) => {
            if label.is_empty() {
                "quotes".to_owned()
            } else {
                format!("quotes ({label})")
            }
        }
        (TabKind::Projects, DrillRequest::DocumentsForEntity { .. })
        | (TabKind::Incidents, DrillRequest::DocumentsForEntity { .. })
        | (TabKind::Appliances, DrillRequest::DocumentsForEntity { .. }) => {
            if label.is_empty() {
                "documents".to_owned()
            } else {
                format!("documents ({label})")
            }
        }
        (TabKind::Vendors, DrillRequest::ServiceLogForVendor(_)) => {
            if label.is_empty() {
                "jobs".to_owned()
            } else {
                format!("jobs ({label})")
            }
        }
        _ => "detail".to_owned(),
    }
}

fn selected_row_label(view_data: &ViewData) -> String {
    let Some(projection) = active_projection(view_data) else {
        return String::new();
    };
    let Some(row) = projection.rows.get(view_data.table_state.selected_row) else {
        return String::new();
    };
    if let Some(cell) = row.cells.get(1) {
        cell.display()
    } else {
        String::new()
    }
}

fn is_note_preview_column(tab: TabKind, column: usize) -> bool {
    matches!(
        (tab, column),
        (TabKind::ServiceLog, 5) | (TabKind::Documents, 5)
    )
}

fn column_action_for(tab: TabKind, column: usize) -> Option<ColumnActionKind> {
    if is_note_preview_column(tab, column) {
        return Some(ColumnActionKind::Note);
    }
    if linked_tab_for_column(tab, column).is_some() {
        return Some(ColumnActionKind::Link);
    }
    if matches!(
        (tab, column),
        (TabKind::Projects, 5)
            | (TabKind::Projects, 6)
            | (TabKind::Maintenance, 7)
            | (TabKind::Incidents, 7)
            | (TabKind::Appliances, 6)
            | (TabKind::Appliances, 7)
            | (TabKind::Vendors, 5)
            | (TabKind::Vendors, 6)
    ) {
        return Some(ColumnActionKind::Drill);
    }
    None
}

fn note_preview_title(tab: TabKind) -> &'static str {
    match tab {
        TabKind::ServiceLog => "service notes",
        TabKind::Documents => "document notes",
        _ => "notes",
    }
}

fn linked_tab_for_column(tab: TabKind, column: usize) -> Option<TabKind> {
    match (tab, column) {
        (TabKind::Quotes, 1) => Some(TabKind::Projects),
        (TabKind::Quotes, 2) => Some(TabKind::Vendors),
        (TabKind::Maintenance, 3) => Some(TabKind::Appliances),
        (TabKind::ServiceLog, 1) => Some(TabKind::Maintenance),
        (TabKind::ServiceLog, 3) => Some(TabKind::Vendors),
        _ => None,
    }
}

fn link_target_id(value: &TableCell) -> Option<i64> {
    let id = match value {
        TableCell::Integer(value) => *value,
        TableCell::OptionalInteger(Some(value)) => *value,
        _ => return None,
    };
    if id > 0 { Some(id) } else { None }
}

fn cell_has_link_target(value: &TableCell) -> bool {
    link_target_id(value).is_some()
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
    if !table_command_allowed_in_mode(state.mode, command) {
        return false;
    }

    let event = apply_table_command(view_data, command);
    if let TableEvent::Status(status) = event {
        emit_status(state, view_data, internal_tx, status.message());
    }
    true
}

fn table_command_allowed_in_mode(mode: AppMode, command: TableCommand) -> bool {
    match mode {
        AppMode::Nav => true,
        AppMode::Edit => matches!(
            command,
            TableCommand::MoveRow(_)
                | TableCommand::MoveColumn(_)
                | TableCommand::MoveHalfPageDown
                | TableCommand::MoveHalfPageUp
                | TableCommand::MoveFullPageDown
                | TableCommand::MoveFullPageUp
                | TableCommand::JumpFirstRow
                | TableCommand::JumpLastRow
                | TableCommand::JumpFirstColumn
                | TableCommand::JumpLastColumn
        ),
        AppMode::Form(_) => false,
    }
}

fn table_command_for_key(key: KeyEvent) -> Option<TableCommand> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Some(TableCommand::MoveRow(1)),
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Some(TableCommand::MoveRow(-1)),
        (KeyCode::Char('h'), _) | (KeyCode::Left, _) => Some(TableCommand::MoveColumn(-1)),
        (KeyCode::Char('l'), _) | (KeyCode::Right, _) => Some(TableCommand::MoveColumn(1)),
        (KeyCode::Char('d'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(TableCommand::MoveHalfPageDown)
        }
        (KeyCode::Char('u'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(TableCommand::MoveHalfPageUp)
        }
        (KeyCode::PageDown, _) => Some(TableCommand::MoveFullPageDown),
        (KeyCode::PageUp, _) => Some(TableCommand::MoveFullPageUp),
        (KeyCode::Char('g'), _) => Some(TableCommand::JumpFirstRow),
        (KeyCode::Char('G'), _) => Some(TableCommand::JumpLastRow),
        (KeyCode::Char('^'), _) => Some(TableCommand::JumpFirstColumn),
        (KeyCode::Char('$'), _) => Some(TableCommand::JumpLastColumn),
        (KeyCode::Char('s'), KeyModifiers::NONE) => Some(TableCommand::CycleSort),
        (KeyCode::Char('S'), _) => Some(TableCommand::ClearSort),
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => Some(TableCommand::ClearPins),
        (KeyCode::Char('n'), KeyModifiers::NONE) => Some(TableCommand::TogglePin),
        (KeyCode::Char('N'), _) => Some(TableCommand::ToggleFilter),
        (KeyCode::Char('!'), _) => Some(TableCommand::ToggleFilterInversion),
        (KeyCode::Char('t'), KeyModifiers::NONE) => Some(TableCommand::ToggleSettledProjects),
        (KeyCode::Char('c'), KeyModifiers::NONE) => Some(TableCommand::HideCurrentColumn),
        (KeyCode::Char('C'), _) => Some(TableCommand::ShowAllColumns),
        (KeyCode::Char('/'), _) => Some(TableCommand::OpenColumnFinder),
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
        TableCommand::MoveHalfPageDown => {
            move_row(view_data, HALF_PAGE_ROWS);
            TableEvent::CursorUpdated
        }
        TableCommand::MoveHalfPageUp => {
            move_row(view_data, -HALF_PAGE_ROWS);
            TableEvent::CursorUpdated
        }
        TableCommand::MoveFullPageDown => {
            move_row(view_data, FULL_PAGE_ROWS);
            TableEvent::CursorUpdated
        }
        TableCommand::MoveFullPageUp => {
            move_row(view_data, -FULL_PAGE_ROWS);
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
            if let Some(projection) = active_projection(view_data) {
                view_data.table_state.selected_col =
                    first_visible_column(&projection, &view_data.table_state.hidden_columns)
                        .unwrap_or(0);
            } else {
                view_data.table_state.selected_col = 0;
            }
            TableEvent::CursorUpdated
        }
        TableCommand::JumpLastColumn => {
            if let Some(projection) = active_projection(view_data) {
                view_data.table_state.selected_col =
                    last_visible_column(&projection, &view_data.table_state.hidden_columns)
                        .unwrap_or_else(|| projection.column_count().saturating_sub(1));
            }
            TableEvent::CursorUpdated
        }
        TableCommand::CycleSort => TableEvent::Status(cycle_sort(view_data)),
        TableCommand::ClearSort => {
            view_data.table_state.sorts.clear();
            clamp_table_cursor(view_data);
            TableEvent::Status(TableStatus::SortCleared)
        }
        TableCommand::TogglePin => TableEvent::Status(toggle_pin(view_data)),
        TableCommand::ToggleFilter => TableEvent::Status(toggle_filter(view_data)),
        TableCommand::ToggleFilterInversion => {
            TableEvent::Status(toggle_filter_inversion(view_data))
        }
        TableCommand::ClearPins => {
            view_data.table_state.pin = None;
            view_data.table_state.filter_active = false;
            view_data.table_state.filter_inverted = false;
            clamp_table_cursor(view_data);
            TableEvent::Status(TableStatus::PinsCleared)
        }
        TableCommand::ToggleSettledProjects => {
            if view_data.table_state.tab != Some(TabKind::Projects) {
                return TableEvent::Status(TableStatus::SettledUnavailable);
            }
            view_data.table_state.hide_settled_projects =
                !view_data.table_state.hide_settled_projects;
            clamp_table_cursor(view_data);
            if view_data.table_state.hide_settled_projects {
                TableEvent::Status(TableStatus::SettledHidden)
            } else {
                TableEvent::Status(TableStatus::SettledShown)
            }
        }
        TableCommand::HideCurrentColumn => {
            let Some(projection) = active_projection(view_data) else {
                return TableEvent::Status(TableStatus::SortUnavailable);
            };
            let visible =
                visible_column_indices(&projection, &view_data.table_state.hidden_columns);
            if visible.len() <= 1 {
                return TableEvent::Status(TableStatus::KeepOneColumnVisible);
            }
            let selected = coerce_visible_column(
                &projection,
                &view_data.table_state.hidden_columns,
                view_data.table_state.selected_col,
            )
            .unwrap_or(visible[0]);
            let label = projection
                .columns
                .get(selected)
                .copied()
                .unwrap_or("column");
            if !view_data.table_state.hidden_columns.insert(selected) {
                return TableEvent::Status(TableStatus::ColumnAlreadyHidden(label));
            }
            if view_data
                .table_state
                .pin
                .as_ref()
                .is_some_and(|pin| pin.column == selected)
            {
                view_data.table_state.pin = None;
                view_data.table_state.filter_active = false;
                view_data.table_state.filter_inverted = false;
            }
            clamp_table_cursor(view_data);
            TableEvent::Status(TableStatus::ColumnHidden(label))
        }
        TableCommand::ShowAllColumns => {
            view_data.table_state.hidden_columns.clear();
            clamp_table_cursor(view_data);
            TableEvent::Status(TableStatus::ColumnsShown)
        }
        TableCommand::OpenColumnFinder => TableEvent::Status(open_column_finder(view_data)),
    }
}

fn active_tab_filter_marker(table_state: &TableUiState) -> Option<&'static str> {
    if table_state.filter_active && table_state.filter_inverted {
        Some(FILTER_MARK_ACTIVE_INVERTED)
    } else if table_state.filter_active {
        Some(FILTER_MARK_ACTIVE)
    } else if table_state.filter_inverted {
        Some(FILTER_MARK_PREVIEW_INVERTED)
    } else if table_state.pin.is_some() {
        Some(FILTER_MARK_PREVIEW)
    } else {
        None
    }
}

fn tab_title(tab: TabKind, state: &AppState, table_state: &TableUiState) -> String {
    if state.active_tab != tab {
        return format!(" {} ", tab.label());
    }

    if let Some(marker) = active_tab_filter_marker(table_state) {
        format!(" {} {} ", tab.label(), marker)
    } else {
        format!(" {} ", tab.label())
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

    if view_data.detail_stack.is_empty() {
        let selected = TabKind::ALL
            .iter()
            .position(|tab| *tab == state.active_tab)
            .unwrap_or(0);
        let tab_titles = TabKind::ALL
            .iter()
            .map(|tab| tab_title(*tab, state, &view_data.table_state))
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
    } else {
        let breadcrumb = Paragraph::new(render_breadcrumb_text(state, view_data))
            .block(Block::default().title("micasa").borders(Borders::ALL));
        frame.render_widget(breadcrumb, layout[0]);
    }

    if state.active_tab == TabKind::Dashboard {
        let body = Paragraph::new(render_dashboard_text(state, view_data))
            .block(Block::default().borders(Borders::ALL).title("dashboard"));
        frame.render_widget(body, layout[1]);
    } else {
        render_table(frame, layout[1], state, view_data);
    }

    let status = status_text(state, view_data);
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
            view_data.mag_mode,
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
        let chat = Paragraph::new(render_chat_overlay_text(
            &view_data.chat,
            view_data.mag_mode,
        ))
        .block(Block::default().title("LLM").borders(Borders::ALL));
        frame.render_widget(chat, area);
    }

    if view_data.column_finder.visible {
        let area = centered_rect(64, 58, frame.area());
        frame.render_widget(Clear, area);
        let finder = Paragraph::new(render_column_finder_overlay_text(view_data)).block(
            Block::default()
                .title("jump to column")
                .borders(Borders::ALL),
        );
        frame.render_widget(finder, area);
    }

    if view_data.note_preview.visible {
        let area = centered_rect(70, 52, frame.area());
        frame.render_widget(Clear, area);
        let preview = Paragraph::new(render_note_preview_overlay_text(&view_data.note_preview))
            .block(Block::default().title("notes").borders(Borders::ALL));
        frame.render_widget(preview, area);
    }

    if view_data.date_picker.visible {
        let area = centered_rect(48, 30, frame.area());
        frame.render_widget(Clear, area);
        let picker = Paragraph::new(render_date_picker_overlay_text(&view_data.date_picker))
            .block(Block::default().title("date").borders(Borders::ALL));
        frame.render_widget(picker, area);
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
        format!(
            "projects due: {}",
            format_magnitude_usize(view_data.dashboard_counts.projects_due, view_data.mag_mode)
        ),
        format!(
            "maintenance due: {}",
            format_magnitude_usize(
                view_data.dashboard_counts.maintenance_due,
                view_data.mag_mode
            )
        ),
        format!(
            "incidents open: {}",
            format_magnitude_usize(
                view_data.dashboard_counts.incidents_open,
                view_data.mag_mode
            )
        ),
    ]
    .join("\n")
}

fn render_breadcrumb_text(state: &AppState, view_data: &ViewData) -> String {
    let mut parts = vec![state.active_tab.label().to_owned()];
    for detail in &view_data.detail_stack {
        parts.push(detail.title.clone());
    }
    parts.join(" > ")
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
                    status_label_for_incident_severity(incident.severity),
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
                format!(
                    "{} | {}",
                    project.title,
                    status_label_for_project_status(project.status)
                ),
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

fn render_dashboard_overlay_text(
    snapshot: &DashboardSnapshot,
    cursor: usize,
    mag_mode: bool,
) -> String {
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
    apply_mag_mode_to_text(&lines.join("\n"), mag_mode)
}

fn render_chat_overlay_text(chat: &ChatUiState, mag_mode: bool) -> String {
    let mut lines = Vec::new();
    let in_flight = chat
        .in_flight
        .map(|task| format!(" | llm: {}", task.stage.label()))
        .unwrap_or_default();
    lines.push(format!(
        "sql: {} | history: {}{}",
        if chat.show_sql { "on" } else { "off" },
        chat.history.len(),
        in_flight
    ));
    lines.push(String::new());

    let keep = chat.transcript.len().saturating_sub(12);
    for message in chat.transcript.iter().skip(keep) {
        let label = match message.role {
            ChatRole::User => "you",
            ChatRole::Assistant => "llm",
        };
        lines.push(format!(
            "{label}: {}",
            apply_mag_mode_to_text(&message.body, mag_mode)
        ));
        if chat.show_sql
            && let Some(sql) = &message.sql
        {
            for segment in sql.lines() {
                lines.push(format!(
                    "  sql: {}",
                    apply_mag_mode_to_text(segment, mag_mode)
                ));
            }
        }
    }

    if chat.transcript.is_empty() {
        lines.push("Ask a question or run /help.".to_owned());
    }

    lines.push(String::new());
    lines.push(format!(
        "> {}",
        apply_mag_mode_to_text(&chat.input, mag_mode)
    ));

    if chat.model_picker.visible {
        lines.push(String::new());
        lines.push(format!("model query: {}", chat.model_picker.query.trim()));
        if let Some(error) = &chat.model_picker.error {
            lines.push(error.clone());
        } else if chat.model_picker.matches.is_empty() {
            lines.push("(no model matches)".to_owned());
        } else {
            let start = chat.model_picker.cursor.saturating_sub(3);
            let end = (start + 8).min(chat.model_picker.matches.len());
            for (index, model) in chat
                .model_picker
                .matches
                .iter()
                .enumerate()
                .take(end)
                .skip(start)
            {
                let prefix = if index == chat.model_picker.cursor {
                    "> "
                } else {
                    "  "
                };
                lines.push(format!("{prefix}{model}"));
            }
            lines.push("up/down pick | enter select | esc close".to_owned());
        }
    }

    lines.push(
        "enter send | up/down history | ctrl+s sql | /models | /model | /sql | /help | esc close"
            .to_owned(),
    );
    lines.join("\n")
}

fn render_date_picker_overlay_text(date_picker: &DatePickerUiState) -> String {
    let selected = date_picker
        .selected
        .map(|date| date.to_string())
        .unwrap_or_else(|| "-".to_owned());
    let original = date_picker
        .original
        .map(|date| date.to_string())
        .unwrap_or_else(|| "(empty)".to_owned());
    let tab_label = date_picker
        .tab
        .map(|tab| tab.label().to_owned())
        .unwrap_or_else(|| "-".to_owned());
    let row_label = date_picker
        .row_id
        .map(|row_id| row_id.to_string())
        .unwrap_or_else(|| "-".to_owned());

    [
        format!("target: {tab_label}#{row_label} c{}", date_picker.column),
        format!("field: {}", date_picker.field_label),
        format!("orig: {original}"),
        format!("pick: {selected}"),
        String::new(),
        "h/l day | j/k week | H/L month | [/] year".to_owned(),
        "enter pick | esc cancel".to_owned(),
    ]
    .join("\n")
}

fn render_column_finder_overlay_text(view_data: &ViewData) -> String {
    let mut lines = Vec::new();
    lines.push(format!("query: {}", view_data.column_finder.query));
    lines.push(String::new());

    let Some(projection) = active_projection(view_data) else {
        lines.push("no active table".to_owned());
        lines.push(String::new());
        lines.push("esc close".to_owned());
        return lines.join("\n");
    };

    let matches = column_finder_matches(
        &projection,
        &view_data.table_state.hidden_columns,
        &view_data.column_finder.query,
    );
    if matches.is_empty() {
        lines.push("(no matches)".to_owned());
    } else {
        let position = view_data
            .column_finder
            .cursor
            .min(matches.len().saturating_sub(1))
            + 1;
        lines.push(format!("{position}/{} matches", matches.len()));
        lines.push(String::new());
        let start = view_data.column_finder.cursor.saturating_sub(4);
        let end = (start + 10).min(matches.len());
        for (index, entry) in matches.iter().enumerate().take(end).skip(start) {
            let prefix = if index == view_data.column_finder.cursor {
                "> "
            } else {
                "  "
            };
            let hidden = if entry.hidden { " [hidden]" } else { "" };
            let highlighted = highlight_column_label(entry.label, &view_data.column_finder.query);
            lines.push(format!("{prefix}{highlighted}{hidden}"));
        }
    }

    lines.push(String::new());
    lines.push("type filter | up/down pick | enter jump | esc close".to_owned());
    lines.join("\n")
}

fn highlight_column_label(label: &str, query: &str) -> String {
    if query.trim().is_empty() {
        return label.to_owned();
    }
    let mut needle = query.chars().filter(|ch| !ch.is_whitespace()).peekable();
    if needle.peek().is_none() {
        return label.to_owned();
    }

    let mut out = String::new();
    let mut current = needle.next();
    for ch in label.chars() {
        match current {
            Some(needle_char) if ch.eq_ignore_ascii_case(&needle_char) => {
                out.push('[');
                out.push(ch);
                out.push(']');
                current = needle.next();
            }
            _ => out.push(ch),
        }
    }
    out
}

fn render_note_preview_overlay_text(note_preview: &NotePreviewUiState) -> String {
    [
        note_preview.title.clone(),
        String::new(),
        note_preview.text.clone(),
        String::new(),
        "press any key to close".to_owned(),
    ]
    .join("\n")
}

fn help_overlay_text() -> &'static str {
    "global: ctrl+q quit | ctrl+c cancel llm | ctrl+o mag mode\n\
nav: j/k/h/l g/G ^/$ d/u pgup/pgdn | b/f tabs | B/F first/last | tab house | D dashboard\n\
nav: enter follow/drill/preview | s/S sort | t settled | c/C cols | / col jump\n\
nav: n/N pin/filter | ctrl+n clear pins | i edit | @ chat | ? help\n\
nav: ! invert filter\n\
edit: a add | e edit (setting/date/form) | d del/restore | x show deleted | u undo | r redo | ctrl+d/u pgup/pgdn | esc nav\n\
form: tab/shift+tab field | 1-9 choose | ctrl+s or enter submit | esc cancel\n\
date picker: h/l day j/k week H/L month [/] year enter pick esc cancel\n\
chat model picker: type /model <query> | up/down or ctrl+p/ctrl+n | enter select | esc dismiss\n\
col finder: type filter | up/down | enter jump | esc close\n\
note preview: any key close\n\
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
    let mut visible_columns =
        visible_column_indices(&projection, &view_data.table_state.hidden_columns);
    if visible_columns.is_empty() {
        visible_columns = (0..projection.column_count()).collect();
    }
    let columns = visible_columns.len();
    let widths = vec![Constraint::Min(8); columns.max(1)];

    let header_cells = visible_columns.iter().map(|full_index| {
        let label = header_label_for_column(&projection, &view_data.table_state, *full_index);
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
            && if view_data.table_state.filter_inverted {
                pin_match
            } else {
                !pin_match
            };

        let cells = visible_columns
            .iter()
            .copied()
            .map(|column_index| {
                let cell_text = row
                    .cells
                    .get(column_index)
                    .map(|cell| cell.display_with_mag_mode(view_data.mag_mode))
                    .unwrap_or_default();
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
                Cell::from(cell_text).style(style)
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

fn header_label_for_column(
    projection: &TableProjection,
    table_state: &TableUiState,
    column_index: usize,
) -> String {
    let mut label = projection.columns[column_index].to_owned();
    if column_has_money_cells(projection, column_index) {
        label.push(' ');
        label.push('$');
    }
    if let Some(tab) = table_state.tab {
        match column_action_for(tab, column_index) {
            Some(ColumnActionKind::Link) => {
                if projection
                    .rows
                    .iter()
                    .filter_map(|row| row.cells.get(column_index))
                    .any(cell_has_link_target)
                {
                    label.push(' ');
                    label.push_str(LINK_ARROW);
                }
            }
            Some(ColumnActionKind::Drill) => {
                label.push(' ');
                label.push_str(DRILL_ARROW);
            }
            Some(ColumnActionKind::Note) | None => {}
        }
    }

    if let Some((position, sort)) = table_state
        .sorts
        .iter()
        .enumerate()
        .find(|(_, sort)| sort.column == column_index)
    {
        if table_state.sorts.len() == 1 {
            let suffix = match sort.direction {
                SortDirection::Asc => " ↑",
                SortDirection::Desc => " ↓",
            };
            label.push_str(suffix);
        } else {
            let marker = match sort.direction {
                SortDirection::Asc => " ▲",
                SortDirection::Desc => " ▼",
            };
            label.push_str(marker);
            label.push_str(&(position + 1).to_string());
        }
    }

    label
}

fn column_has_money_cells(projection: &TableProjection, column_index: usize) -> bool {
    projection
        .rows
        .iter()
        .filter_map(|row| row.cells.get(column_index))
        .any(|cell| matches!(cell, TableCell::Money(_)))
}

fn table_title(projection: &TableProjection, table_state: &TableUiState) -> String {
    let visible_columns = visible_column_indices(projection, &table_state.hidden_columns);
    let visible_count = if visible_columns.is_empty() {
        projection.column_count()
    } else {
        visible_columns.len()
    };
    let mut parts = vec![format!(
        "{} r:{} c:{}/{}",
        projection.title,
        projection.row_count(),
        visible_count,
        projection.column_count(),
    )];

    if !table_state.sorts.is_empty() {
        let labels = table_state
            .sorts
            .iter()
            .enumerate()
            .filter_map(|(index, sort)| {
                projection.columns.get(sort.column).map(|label| {
                    let direction = match sort.direction {
                        SortDirection::Asc => "asc",
                        SortDirection::Desc => "desc",
                    };
                    format!("{label}:{direction}#{}", index + 1)
                })
            })
            .collect::<Vec<_>>();
        if !labels.is_empty() {
            parts.push(format!("sort {}", labels.join(",")));
        }
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
    if table_state.filter_inverted {
        parts.push("invert on".to_owned());
    }
    if table_state.hide_settled_projects && table_state.tab == Some(TabKind::Projects) {
        parts.push("settled hidden".to_owned());
    }
    let deleted_count = projection.rows.iter().filter(|row| row.deleted).count();
    if deleted_count > 0 {
        parts.push(format!("del {deleted_count}"));
    }
    let hidden_count = projection.column_count().saturating_sub(visible_count);
    if hidden_count > 0 {
        parts.push(format!("hidden {hidden_count}"));
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

fn cell_matches_pin_value(value: &TableCell, pin: &TableCell) -> bool {
    match (value, pin) {
        (TableCell::Text(value), TableCell::Text(pin)) => {
            value.trim().to_lowercase() == pin.trim().to_lowercase()
        }
        _ => value == pin,
    }
}

fn row_matches_pin(row: &TableRowProjection, table_state: &TableUiState) -> bool {
    match &table_state.pin {
        Some(pin) => row
            .cells
            .get(pin.column)
            .map(|value| cell_matches_pin_value(value, &pin.value))
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

    if table_state.hide_settled_projects {
        projection.rows.retain(|row| {
            !matches!(
                row.tag,
                Some(RowTag::ProjectStatus(
                    ProjectStatus::Completed | ProjectStatus::Abandoned
                ))
            )
        });
    }

    if !table_state.sorts.is_empty() {
        let column_count = projection.column_count();
        projection.rows.sort_by(|left, right| {
            for sort in &table_state.sorts {
                if sort.column >= column_count {
                    continue;
                }
                let left_value = left.cells.get(sort.column);
                let right_value = right.cells.get(sort.column);
                let left_null = left_value.map(TableCell::is_null).unwrap_or(true);
                let right_null = right_value.map(TableCell::is_null).unwrap_or(true);
                if left_null && right_null {
                    continue;
                }
                if left_null {
                    return Ordering::Greater;
                }
                if right_null {
                    return Ordering::Less;
                }
                let order = match (left_value, right_value) {
                    (Some(left), Some(right)) => match sort.direction {
                        SortDirection::Asc => left.cmp_value(right),
                        SortDirection::Desc => left.cmp_value(right).reverse(),
                    },
                    _ => Ordering::Equal,
                };
                if order != Ordering::Equal {
                    return order;
                }
            }

            let left_id = match left.cells.first() {
                Some(TableCell::Integer(id)) => Some(*id),
                _ => None,
            };
            let right_id = match right.cells.first() {
                Some(TableCell::Integer(id)) => Some(*id),
                _ => None,
            };
            left_id.cmp(&right_id)
        });
    }

    if table_state.filter_active
        && let Some(pin) = &table_state.pin
    {
        projection.rows.retain(|row| {
            let pin_match = row
                .cells
                .get(pin.column)
                .map(|value| cell_matches_pin_value(value, &pin.value))
                .unwrap_or(false);
            if table_state.filter_inverted {
                !pin_match
            } else {
                pin_match
            }
        });
    }

    projection
}

fn visible_column_indices(
    projection: &TableProjection,
    hidden_columns: &BTreeSet<usize>,
) -> Vec<usize> {
    (0..projection.column_count())
        .filter(|index| !hidden_columns.contains(index))
        .collect()
}

fn first_visible_column(
    projection: &TableProjection,
    hidden_columns: &BTreeSet<usize>,
) -> Option<usize> {
    visible_column_indices(projection, hidden_columns)
        .into_iter()
        .next()
}

fn last_visible_column(
    projection: &TableProjection,
    hidden_columns: &BTreeSet<usize>,
) -> Option<usize> {
    visible_column_indices(projection, hidden_columns)
        .into_iter()
        .last()
}

fn coerce_visible_column(
    projection: &TableProjection,
    hidden_columns: &BTreeSet<usize>,
    selected_col: usize,
) -> Option<usize> {
    let visible = visible_column_indices(projection, hidden_columns);
    if visible.is_empty() {
        return None;
    }

    match visible.binary_search(&selected_col) {
        Ok(index) => Some(visible[index]),
        Err(index) => {
            if index >= visible.len() {
                visible.last().copied()
            } else {
                Some(visible[index])
            }
        }
    }
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
                        tag: None,
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
            columns: vec![
                "id", "title", "status", "budget", "actual", "quotes", "docs",
            ],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.title.clone()),
                        TableCell::ProjectStatus(row.status),
                        TableCell::Money(row.budget_cents),
                        TableCell::Money(row.actual_cents),
                        TableCell::Text(String::new()),
                        TableCell::Text(String::new()),
                    ],
                    deleted: row.deleted_at.is_some(),
                    tag: Some(RowTag::ProjectStatus(row.status)),
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
                    tag: None,
                })
                .collect(),
        },
        TabSnapshot::Maintenance(rows) => TableProjection {
            title: "maintenance",
            columns: vec![
                "id",
                "item",
                "cat",
                "appliance",
                "last",
                "every",
                "cost",
                "log",
            ],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.name.clone()),
                        TableCell::Integer(row.category_id.get()),
                        TableCell::OptionalInteger(row.appliance_id.map(|id| id.get())),
                        TableCell::Date(row.last_serviced_at),
                        TableCell::IntervalMonths(row.interval_months),
                        TableCell::Money(row.cost_cents),
                        TableCell::Text(String::new()),
                    ],
                    deleted: row.deleted_at.is_some(),
                    tag: None,
                })
                .collect(),
        },
        TabSnapshot::ServiceLog(rows) => TableProjection {
            title: "service",
            columns: vec!["id", "maint", "date", "vendor", "cost", "notes"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Integer(row.maintenance_item_id.get()),
                        TableCell::Date(Some(row.serviced_at)),
                        TableCell::OptionalInteger(row.vendor_id.map(|id| id.get())),
                        TableCell::Money(row.cost_cents),
                        TableCell::Text(row.notes.clone()),
                    ],
                    deleted: row.deleted_at.is_some(),
                    tag: None,
                })
                .collect(),
        },
        TabSnapshot::Incidents(rows) => TableProjection {
            title: "incidents",
            columns: vec![
                "id", "title", "status", "sev", "noticed", "resolved", "cost", "docs",
            ],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.title.clone()),
                        TableCell::IncidentStatus(row.status),
                        TableCell::IncidentSeverity(row.severity),
                        TableCell::Date(Some(row.date_noticed)),
                        TableCell::Date(row.date_resolved),
                        TableCell::Money(row.cost_cents),
                        TableCell::Text(String::new()),
                    ],
                    deleted: row.deleted_at.is_some(),
                    tag: None,
                })
                .collect(),
        },
        TabSnapshot::Appliances(rows) => TableProjection {
            title: "appliances",
            columns: vec![
                "id", "name", "brand", "location", "warranty", "cost", "maint", "docs",
            ],
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
                        TableCell::Text(String::new()),
                        TableCell::Text(String::new()),
                    ],
                    deleted: row.deleted_at.is_some(),
                    tag: None,
                })
                .collect(),
        },
        TabSnapshot::Vendors(rows) => TableProjection {
            title: "vendors",
            columns: vec!["id", "name", "contact", "email", "phone", "quotes", "jobs"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.name.clone()),
                        TableCell::Text(row.contact_name.clone()),
                        TableCell::Text(row.email.clone()),
                        TableCell::Text(row.phone.clone()),
                        TableCell::Text(String::new()),
                        TableCell::Text(String::new()),
                    ],
                    deleted: row.deleted_at.is_some(),
                    tag: None,
                })
                .collect(),
        },
        TabSnapshot::Documents(rows) => TableProjection {
            title: "documents",
            columns: vec!["id", "title", "file", "entity", "size", "notes"],
            rows: rows
                .iter()
                .map(|row| TableRowProjection {
                    cells: vec![
                        TableCell::Integer(row.id.get()),
                        TableCell::Text(row.title.clone()),
                        TableCell::Text(row.file_name.clone()),
                        TableCell::Text(row.entity_kind.as_str().to_owned()),
                        TableCell::Integer(row.size_bytes),
                        TableCell::Text(row.notes.clone()),
                    ],
                    deleted: row.deleted_at.is_some(),
                    tag: None,
                })
                .collect(),
        },
        TabSnapshot::Settings(rows) => TableProjection {
            title: "settings",
            columns: vec!["id", "setting", "value"],
            rows: rows
                .iter()
                .enumerate()
                .map(|(index, setting)| TableRowProjection {
                    cells: vec![
                        TableCell::Integer((index + 1) as i64),
                        TableCell::Text(setting.key.label().to_owned()),
                        TableCell::Text(setting.value.display()),
                    ],
                    deleted: false,
                    tag: Some(RowTag::Setting(setting.key)),
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

fn format_compact_money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let absolute = cents.unsigned_abs();
    let dollars = (absolute as f64) / 100.0;
    if dollars < 1000.0 {
        return format!("{sign}{dollars:.2}");
    }

    let (value, suffix) = if dollars < 1_000_000.0 {
        (dollars / 1000.0, "k")
    } else if dollars < 1_000_000_000.0 {
        (dollars / 1_000_000.0, "M")
    } else {
        (dollars / 1_000_000_000.0, "B")
    };

    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract().abs() < f64::EPSILON {
        format!("{sign}{rounded:.0}{suffix}")
    } else {
        format!("{sign}{rounded:.1}{suffix}")
    }
}

fn format_interval_months(months: i32) -> String {
    if months <= 0 {
        return String::new();
    }

    let years = months / 12;
    let remainder = months % 12;
    match (years, remainder) {
        (0, m) => format!("{m}m"),
        (y, 0) => format!("{y}y"),
        (y, m) => format!("{y}y {m}m"),
    }
}

fn status_label_for_project_status(status: ProjectStatus) -> &'static str {
    match status {
        ProjectStatus::Ideating => "idea",
        ProjectStatus::Planned => "plan",
        ProjectStatus::Quoted => "bid",
        ProjectStatus::Underway => "wip",
        ProjectStatus::Delayed => "hold",
        ProjectStatus::Completed => "done",
        ProjectStatus::Abandoned => "drop",
    }
}

fn status_label_for_incident_status(status: micasa_app::IncidentStatus) -> &'static str {
    match status {
        micasa_app::IncidentStatus::Open => "open",
        micasa_app::IncidentStatus::InProgress => "act",
        micasa_app::IncidentStatus::Resolved => "resolved",
    }
}

fn status_label_for_incident_severity(severity: IncidentSeverity) -> &'static str {
    match severity {
        IncidentSeverity::Urgent => "urg",
        IncidentSeverity::Soon => "soon",
        IncidentSeverity::Whenever => "low",
    }
}

fn format_magnitude_i64(value: i64) -> String {
    if value == 0 {
        return "0".to_owned();
    }
    let sign = if value < 0 { "-" } else { "" };
    let magnitude = rounded_log10(value.unsigned_abs() as f64);
    format!("{sign}↑{magnitude}")
}

fn format_magnitude_f64(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let magnitude = rounded_log10(value.abs());
    format!("{sign}↑{magnitude}")
}

fn format_magnitude_money(cents: i64) -> String {
    if cents == 0 {
        return "$ ↑-∞".to_owned();
    }
    let sign = if cents < 0 { "-" } else { "" };
    let dollars = (cents.unsigned_abs() as f64) / 100.0;
    let magnitude = rounded_log10(dollars);
    format!("{sign}$ ↑{magnitude}")
}

fn format_magnitude_money_without_unit(cents: i64) -> String {
    if cents == 0 {
        return "↑-∞".to_owned();
    }
    let sign = if cents < 0 { "-" } else { "" };
    let dollars = (cents.unsigned_abs() as f64) / 100.0;
    let magnitude = rounded_log10(dollars);
    format!("{sign}↑{magnitude}")
}

fn format_magnitude_usize(value: usize, mag_mode: bool) -> String {
    if !mag_mode {
        return value.to_string();
    }
    if value == 0 {
        "0".to_owned()
    } else {
        format!("↑{}", rounded_log10(value as f64))
    }
}

fn apply_mag_mode_to_text(input: &str, mag_mode: bool) -> String {
    if !mag_mode {
        return input.to_owned();
    }

    let mut out = String::with_capacity(input.len());
    let chars = input.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        if let Some((formatted, consumed)) = parse_mag_money_token(&chars, index) {
            out.push_str(&formatted);
            index += consumed;
            continue;
        }
        if let Some((formatted, consumed)) = parse_mag_number_token(&chars, index) {
            out.push_str(&formatted);
            index += consumed;
            continue;
        }

        out.push(chars[index]);
        index += 1;
    }

    out
}

fn rounded_log10(value: f64) -> i32 {
    value.abs().log10().round() as i32
}

fn is_word_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || value == '_'
}

fn is_word_boundary_before(chars: &[char], index: usize) -> bool {
    index == 0 || !is_word_char(chars[index.saturating_sub(1)])
}

fn is_word_boundary_after(chars: &[char], index: usize) -> bool {
    chars.get(index).is_none_or(|value| !is_word_char(*value))
}

fn parse_numeric_token(chars: &[char], start: usize) -> Option<usize> {
    if chars.get(start).is_none_or(|value| !value.is_ascii_digit()) {
        return None;
    }

    let mut end = start;
    while chars
        .get(end)
        .is_some_and(|value| value.is_ascii_digit() || *value == ',')
    {
        end += 1;
    }

    if chars.get(end) == Some(&'.') {
        let mut frac_end = end + 1;
        while chars
            .get(frac_end)
            .is_some_and(|value| value.is_ascii_digit())
        {
            frac_end += 1;
        }
        if frac_end > end + 1 {
            end = frac_end;
        }
    }

    Some(end)
}

fn parse_mag_money_token(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut cursor = start;
    let mut is_negative = false;
    if chars.get(cursor) == Some(&'-') && chars.get(cursor + 1) == Some(&'$') {
        is_negative = true;
        cursor += 1;
    }
    if chars.get(cursor) != Some(&'$') {
        return None;
    }
    let numeric_start = cursor + 1;
    let numeric_end = parse_numeric_token(chars, numeric_start)?;
    let numeric = chars[numeric_start..numeric_end]
        .iter()
        .collect::<String>()
        .replace(',', "");
    let value = numeric.parse::<f64>().ok()?;
    let mut cents = (value * 100.0).round() as i64;
    if is_negative {
        cents = -cents;
    }
    Some((format_magnitude_money(cents), numeric_end - start))
}

fn parse_mag_number_token(chars: &[char], start: usize) -> Option<(String, usize)> {
    if !is_word_boundary_before(chars, start) {
        return None;
    }
    let end = parse_numeric_token(chars, start)?;
    if !is_word_boundary_after(chars, end) {
        return None;
    }
    let numeric = chars[start..end]
        .iter()
        .collect::<String>()
        .replace(',', "");
    let value = numeric.parse::<f64>().ok()?;
    let formatted = if value == 0.0 {
        "0".to_owned()
    } else {
        format!("↑{}", rounded_log10(value))
    };
    Some((formatted, end - start))
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
    let visible = visible_column_indices(&projection, &view_data.table_state.hidden_columns);
    if visible.is_empty() {
        view_data.table_state.selected_col = 0;
        return;
    }

    let current = coerce_visible_column(
        &projection,
        &view_data.table_state.hidden_columns,
        view_data.table_state.selected_col,
    )
    .unwrap_or(visible[0]);
    let current_index = visible
        .iter()
        .position(|index| *index == current)
        .unwrap_or(0);
    let next_index = if delta.is_negative() {
        current_index.saturating_sub(delta.unsigned_abs())
    } else {
        current_index.saturating_add(delta as usize)
    };
    view_data.table_state.selected_col = visible[next_index.min(visible.len().saturating_sub(1))];
}

fn selected_cell(view_data: &ViewData) -> Option<(usize, TableCell)> {
    let projection = active_projection(view_data)?;
    let row = projection.rows.get(view_data.table_state.selected_row)?;
    let col = coerce_visible_column(
        &projection,
        &view_data.table_state.hidden_columns,
        view_data.table_state.selected_col,
    )?;
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

    let Some(column) = coerce_visible_column(
        &projection,
        &view_data.table_state.hidden_columns,
        view_data.table_state.selected_col,
    ) else {
        return TableStatus::SortUnavailable;
    };
    let label = projection.columns[column];

    if let Some(index) = view_data
        .table_state
        .sorts
        .iter()
        .position(|sort| sort.column == column)
    {
        match view_data.table_state.sorts[index].direction {
            SortDirection::Asc => {
                view_data.table_state.sorts[index].direction = SortDirection::Desc;
            }
            SortDirection::Desc => {
                view_data.table_state.sorts.remove(index);
            }
        }
    } else {
        view_data.table_state.sorts.push(SortSpec {
            column,
            direction: SortDirection::Asc,
        });
    }

    clamp_table_cursor(view_data);
    match view_data
        .table_state
        .sorts
        .iter()
        .find(|sort| sort.column == column)
        .map(|sort| sort.direction)
    {
        Some(SortDirection::Asc) => TableStatus::SortAsc(label),
        Some(SortDirection::Desc) => TableStatus::SortDesc(label),
        None => TableStatus::SortCleared,
    }
}

fn toggle_pin(view_data: &mut ViewData) -> TableStatus {
    let Some((column, value)) = selected_cell(view_data) else {
        return TableStatus::PinUnavailable;
    };

    if let Some(existing) = &view_data.table_state.pin
        && existing.column == column
        && cell_matches_pin_value(&existing.value, &value)
    {
        view_data.table_state.pin = None;
        view_data.table_state.filter_active = false;
        view_data.table_state.filter_inverted = false;
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

fn toggle_filter_inversion(view_data: &mut ViewData) -> TableStatus {
    view_data.table_state.filter_inverted = !view_data.table_state.filter_inverted;
    clamp_table_cursor(view_data);
    if view_data.table_state.filter_inverted {
        TableStatus::FilterInvertedOn
    } else {
        TableStatus::FilterInvertedOff
    }
}

fn clamp_table_cursor(view_data: &mut ViewData) {
    let Some(snapshot) = &view_data.active_tab_snapshot else {
        view_data.table_state.selected_col = 0;
        view_data.table_state.selected_row = 0;
        return;
    };

    let mut projection = projection_for_snapshot(snapshot, &view_data.table_state);

    let original_sort_len = view_data.table_state.sorts.len();
    view_data
        .table_state
        .sorts
        .retain(|sort| sort.column < projection.column_count());
    if view_data.table_state.sorts.len() != original_sort_len {
        projection = projection_for_snapshot(snapshot, &view_data.table_state);
    }

    if let Some(pin) = &view_data.table_state.pin
        && pin.column >= projection.column_count()
    {
        view_data.table_state.pin = None;
        view_data.table_state.filter_active = false;
        view_data.table_state.filter_inverted = false;
        projection = projection_for_snapshot(snapshot, &view_data.table_state);
    }

    if projection.column_count() == 0 {
        view_data.table_state.selected_col = 0;
    } else {
        if visible_column_indices(&projection, &view_data.table_state.hidden_columns).is_empty() {
            view_data.table_state.hidden_columns.clear();
        }
        view_data.table_state.selected_col = coerce_visible_column(
            &projection,
            &view_data.table_state.hidden_columns,
            view_data.table_state.selected_col,
        )
        .unwrap_or(0);
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

fn status_text(state: &AppState, view_data: &ViewData) -> String {
    // Match legacy UX: overlays suppress the main status/keybinding bar.
    if status_hidden_by_overlay(view_data) {
        return String::new();
    }

    let mode = match state.mode {
        AppMode::Nav => "NAV",
        AppMode::Edit => "EDIT",
        AppMode::Form(_) => "FORM",
    };
    let enter_hint = contextual_enter_hint(view_data);
    let mag_label = if view_data.mag_mode { "on" } else { "off" };
    let mut default = format!(
        "j/k/h/l g/G ^/$ d/u pg | enter {enter_hint} | s/S/t c/C / | n/N ctrl+n | @ chat D | ctrl+o mag:{mag_label} | ctrl+q"
    );
    if matches!(state.mode, AppMode::Form(_))
        && let Some(form) = view_data.form
    {
        default = format!(
            "{} | {default}",
            format_form_field_status(form.kind, form.field_index)
        );
    }
    match &state.status_line {
        Some(status) => format!("{mode} | {status} | {default}"),
        None => format!("{mode} | {default}"),
    }
}

fn status_hidden_by_overlay(view_data: &ViewData) -> bool {
    view_data.dashboard.visible
        || view_data.help_visible
        || view_data.note_preview.visible
        || view_data.column_finder.visible
        || view_data.date_picker.visible
}

fn contextual_enter_hint(view_data: &ViewData) -> &'static str {
    let Some(tab) = view_data.table_state.tab else {
        return "open";
    };
    if tab == TabKind::Settings {
        return "edit";
    }
    let Some((column, value)) = selected_cell(view_data) else {
        return "open";
    };

    match column_action_for(tab, column) {
        Some(ColumnActionKind::Note) => "preview",
        Some(ColumnActionKind::Drill) => "drill",
        Some(ColumnActionKind::Link) => {
            if cell_has_link_target(&value) {
                "follow"
            } else {
                "none"
            }
        }
        None => "open",
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
        TabKind::Settings => None,
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
    sync_form_ui_state(state, view_data);
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
    sync_form_ui_state(state, view_data);
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
    view_data.table_state.filter_inverted = false;
    view_data.table_state.sorts.clear();
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
        AppRuntime, ChatHistoryMessage, ChatHistoryRole, ChatPipelineResult, DashboardIncident,
        DashboardMaintenance, DashboardProject, DashboardServiceEntry, DashboardSnapshot,
        DashboardWarranty, LifecycleAction, TabSnapshot, TableCommand, TableEvent, TableStatus,
        ViewData, apply_mag_mode_to_text, apply_table_command, coerce_visible_column,
        contextual_enter_hint, dashboard_nav_entries, first_visible_column, format_compact_money,
        format_interval_months, format_magnitude_money, format_magnitude_usize,
        handle_date_picker_key, handle_key_event, header_label_for_column, help_overlay_text,
        highlight_column_label, last_visible_column, refresh_view_data, render_breadcrumb_text,
        render_chat_overlay_text, render_dashboard_overlay_text, render_dashboard_text,
        render_date_picker_overlay_text, render_note_preview_overlay_text, shift_date_by_months,
        shift_date_by_years, status_label_for_incident_severity, status_label_for_incident_status,
        status_label_for_project_status, status_text, table_command_for_key, table_title,
        visible_column_indices,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use micasa_app::{
        AppMode, AppSetting, AppState, ChatVisibility, DashboardCounts, FormKind, FormPayload,
        IncidentSeverity, Project, ProjectStatus, ProjectTypeId, SettingKey, SettingValue,
        SortDirection, TabKind,
    };
    use std::collections::BTreeSet;
    use std::sync::mpsc;
    use time::{Date, Month, OffsetDateTime};

    #[derive(Debug, Default)]
    struct TestRuntime {
        submit_count: usize,
        lifecycle_count: usize,
        undo_count: usize,
        redo_count: usize,
        can_undo: bool,
        can_redo: bool,
        chat_history: Vec<String>,
        show_dashboard_pref: Option<bool>,
        available_models: Vec<String>,
        active_model: Option<String>,
        pipeline_result: Option<ChatPipelineResult>,
        pipeline_error: Option<String>,
        last_pipeline_question: Option<String>,
        last_pipeline_history: Vec<ChatHistoryMessage>,
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

        fn sample_quote(id: i64, project_id: i64, vendor_id: i64) -> micasa_app::Quote {
            micasa_app::Quote {
                id: micasa_app::QuoteId::new(id),
                project_id: micasa_app::ProjectId::new(project_id),
                vendor_id: micasa_app::VendorId::new(vendor_id),
                total_cents: 11_000,
                labor_cents: None,
                materials_cents: None,
                other_cents: None,
                received_date: None,
                notes: String::new(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                deleted_at: None,
            }
        }

        fn sample_service_log(
            id: i64,
            maintenance_item_id: i64,
            vendor_id: Option<i64>,
            notes: &str,
        ) -> micasa_app::ServiceLogEntry {
            micasa_app::ServiceLogEntry {
                id: micasa_app::ServiceLogEntryId::new(id),
                maintenance_item_id: micasa_app::MaintenanceItemId::new(maintenance_item_id),
                serviced_at: Date::from_calendar_date(2026, Month::January, 5).expect("valid date"),
                vendor_id: vendor_id.map(micasa_app::VendorId::new),
                cost_cents: Some(25_00),
                notes: notes.to_owned(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                deleted_at: None,
            }
        }

        fn sample_appliance(id: i64, name: &str) -> micasa_app::Appliance {
            micasa_app::Appliance {
                id: micasa_app::ApplianceId::new(id),
                name: name.to_owned(),
                brand: "brand".to_owned(),
                model_number: String::new(),
                serial_number: String::new(),
                purchase_date: None,
                warranty_expiry: None,
                location: "garage".to_owned(),
                cost_cents: None,
                notes: String::new(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                deleted_at: None,
            }
        }

        fn sample_maintenance(
            id: i64,
            appliance_id: Option<i64>,
            name: &str,
        ) -> micasa_app::MaintenanceItem {
            micasa_app::MaintenanceItem {
                id: micasa_app::MaintenanceItemId::new(id),
                name: name.to_owned(),
                category_id: micasa_app::MaintenanceCategoryId::new(1),
                appliance_id: appliance_id.map(micasa_app::ApplianceId::new),
                last_serviced_at: None,
                interval_months: 6,
                manual_url: String::new(),
                manual_text: String::new(),
                notes: String::new(),
                cost_cents: None,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                deleted_at: None,
            }
        }

        fn sample_vendor(id: i64, name: &str) -> micasa_app::Vendor {
            micasa_app::Vendor {
                id: micasa_app::VendorId::new(id),
                name: name.to_owned(),
                contact_name: "Alex".to_owned(),
                email: format!("{name}@example.com").to_ascii_lowercase(),
                phone: "555-1000".to_owned(),
                website: "https://example.com".to_owned(),
                notes: String::new(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                deleted_at: None,
            }
        }

        fn sample_incident(id: i64, title: &str) -> micasa_app::Incident {
            micasa_app::Incident {
                id: micasa_app::IncidentId::new(id),
                title: title.to_owned(),
                description: String::new(),
                status: micasa_app::IncidentStatus::Open,
                severity: IncidentSeverity::Soon,
                date_noticed: Date::from_calendar_date(2026, Month::January, 3)
                    .expect("valid date"),
                date_resolved: None,
                location: "basement".to_owned(),
                cost_cents: Some(50_00),
                appliance_id: Some(micasa_app::ApplianceId::new(4)),
                vendor_id: Some(micasa_app::VendorId::new(7)),
                notes: String::new(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                deleted_at: None,
            }
        }

        fn sample_document(
            id: i64,
            kind: micasa_app::DocumentEntityKind,
            entity_id: i64,
            title: &str,
            notes: &str,
        ) -> micasa_app::Document {
            micasa_app::Document {
                id: micasa_app::DocumentId::new(id),
                title: title.to_owned(),
                file_name: format!("{title}.pdf").to_ascii_lowercase(),
                entity_kind: kind,
                entity_id,
                mime_type: "application/pdf".to_owned(),
                size_bytes: 1_024,
                checksum_sha256: format!("sha256-{id}"),
                data: vec![id as u8],
                notes: notes.to_owned(),
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
                TabKind::Quotes => Some(TabSnapshot::Quotes(vec![
                    Self::sample_quote(11, 2, 7),
                    Self::sample_quote(12, 1, 7),
                    Self::sample_quote(13, 1, 8),
                ])),
                TabKind::Maintenance => Some(TabSnapshot::Maintenance(vec![
                    Self::sample_maintenance(2, Some(4), "HVAC filter"),
                    Self::sample_maintenance(3, Some(5), "Water softener clean"),
                ])),
                TabKind::ServiceLog => Some(TabSnapshot::ServiceLog(vec![
                    Self::sample_service_log(19, 2, Some(7), "Inspect vent before summer."),
                    Self::sample_service_log(20, 3, Some(8), "Flush brine tank."),
                ])),
                TabKind::Incidents => Some(TabSnapshot::Incidents(vec![
                    Self::sample_incident(6, "Basement leak"),
                    Self::sample_incident(7, "Sump alarm"),
                ])),
                TabKind::Appliances => Some(TabSnapshot::Appliances(vec![
                    Self::sample_appliance(4, "Furnace"),
                    Self::sample_appliance(5, "Water softener"),
                ])),
                TabKind::Vendors => Some(TabSnapshot::Vendors(vec![
                    Self::sample_vendor(7, "Acme HVAC"),
                    Self::sample_vendor(8, "Budget Plumbing"),
                ])),
                TabKind::Documents => Some(TabSnapshot::Documents(vec![
                    Self::sample_document(
                        31,
                        micasa_app::DocumentEntityKind::Project,
                        2,
                        "Project Scope",
                        "Scope notes",
                    ),
                    Self::sample_document(
                        32,
                        micasa_app::DocumentEntityKind::Appliance,
                        4,
                        "Furnace Manual",
                        "Maintenance guidance",
                    ),
                    Self::sample_document(
                        33,
                        micasa_app::DocumentEntityKind::Incident,
                        6,
                        "Leak Photo",
                        "Basement leak evidence",
                    ),
                    Self::sample_document(
                        34,
                        micasa_app::DocumentEntityKind::Project,
                        1,
                        "Alpha Estimate",
                        "Older estimate",
                    ),
                ])),
                TabKind::Settings => Some(TabSnapshot::Settings(vec![
                    AppSetting {
                        key: SettingKey::UiShowDashboard,
                        value: SettingValue::Bool(self.show_dashboard_pref.unwrap_or(true)),
                    },
                    AppSetting {
                        key: SettingKey::LlmModel,
                        value: SettingValue::Text(self.active_model.clone().unwrap_or_default()),
                    },
                ])),
            };
            Ok(snapshot)
        }

        fn submit_form(&mut self, payload: &FormPayload) -> anyhow::Result<()> {
            payload.validate()?;
            self.submit_count += 1;
            Ok(())
        }

        fn load_chat_history(&mut self) -> anyhow::Result<Vec<String>> {
            Ok(self.chat_history.clone())
        }

        fn append_chat_input(&mut self, input: &str) -> anyhow::Result<()> {
            if self
                .chat_history
                .last()
                .map(|last| last == input)
                .unwrap_or(false)
            {
                return Ok(());
            }
            self.chat_history.push(input.to_owned());
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

        fn set_show_dashboard_preference(&mut self, show: bool) -> anyhow::Result<()> {
            self.show_dashboard_pref = Some(show);
            Ok(())
        }

        fn list_chat_models(&mut self) -> anyhow::Result<Vec<String>> {
            Ok(self.available_models.clone())
        }

        fn active_chat_model(&mut self) -> anyhow::Result<Option<String>> {
            Ok(self.active_model.clone())
        }

        fn select_chat_model(&mut self, model: &str) -> anyhow::Result<()> {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                return Err(anyhow::anyhow!("usage: /model <name>"));
            }
            if !self.available_models.iter().any(|entry| entry == trimmed) {
                return Err(anyhow::anyhow!(
                    "model `{trimmed}` not available; use /models first"
                ));
            }
            self.active_model = Some(trimmed.to_owned());
            Ok(())
        }

        fn run_chat_pipeline(
            &mut self,
            question: &str,
            history: &[ChatHistoryMessage],
        ) -> anyhow::Result<ChatPipelineResult> {
            self.last_pipeline_question = Some(question.to_owned());
            self.last_pipeline_history = history.to_vec();

            if let Some(error) = self.pipeline_error.take() {
                return Err(anyhow::anyhow!("{error}"));
            }

            Ok(self.pipeline_result.clone().unwrap_or(ChatPipelineResult {
                answer: "stub answer".to_owned(),
                sql: Some("SELECT 1".to_owned()),
                used_fallback: false,
            }))
        }
    }

    fn view_data_for_test() -> ViewData {
        ViewData::default()
    }

    fn projection_for_visibility_test() -> super::TableProjection {
        super::TableProjection {
            title: "projects",
            columns: vec!["id", "title", "status", "notes"],
            rows: vec![],
        }
    }

    fn internal_tx() -> mpsc::Sender<super::InternalEvent> {
        let (tx, _rx) = mpsc::channel();
        tx
    }

    fn internal_channel() -> (
        mpsc::Sender<super::InternalEvent>,
        mpsc::Receiver<super::InternalEvent>,
    ) {
        mpsc::channel()
    }

    fn pump_internal(
        state: &mut AppState,
        view_data: &mut ViewData,
        tx: &mpsc::Sender<super::InternalEvent>,
        rx: &mpsc::Receiver<super::InternalEvent>,
    ) {
        super::process_internal_events(state, view_data, tx, rx);
    }

    fn run_key_script(
        state: &mut AppState,
        runtime: &mut TestRuntime,
        view_data: &mut ViewData,
        tx: &mpsc::Sender<super::InternalEvent>,
        rx: &mpsc::Receiver<super::InternalEvent>,
        keys: &[KeyEvent],
    ) {
        for key in keys {
            let _ = handle_key_event(state, runtime, view_data, tx, *key);
            pump_internal(state, view_data, tx, rx);
        }
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
    fn ctrl_o_toggles_mag_mode() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        );
        assert!(view_data.mag_mode);
        assert_eq!(apply_mag_mode_to_text("cost 1250", true), "cost ↑3");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        );
        assert!(!view_data.mag_mode);
    }

    #[test]
    fn magnitude_formatters_encode_order_of_magnitude() {
        assert_eq!(format_magnitude_money(0), "$ ↑-∞");
        assert_eq!(format_magnitude_money(50_000), "$ ↑3");
        assert_eq!(format_magnitude_money(523_423), "$ ↑4");
        assert_eq!(format_magnitude_money(-130_000_000), "-$ ↑6");

        assert_eq!(format_magnitude_usize(0, true), "0");
        assert_eq!(format_magnitude_usize(9, true), "↑1");
        assert_eq!(format_magnitude_usize(42, true), "↑2");
        assert_eq!(format_magnitude_usize(1_234, true), "↑3");
        assert_eq!(format_magnitude_usize(1_234, false), "1234");
    }

    #[test]
    fn compact_money_formatter_matches_go_shapes() {
        assert_eq!(format_compact_money(50_000), "500.00");
        assert_eq!(format_compact_money(523_423), "5.2k");
        assert_eq!(format_compact_money(4_500_000), "45k");
        assert_eq!(format_compact_money(130_000_000), "1.3M");
        assert_eq!(format_compact_money(-500), "-5.00");
    }

    #[test]
    fn interval_formatter_compacts_months_to_year_month_shape() {
        assert_eq!(format_interval_months(0), "");
        assert_eq!(format_interval_months(-3), "");
        assert_eq!(format_interval_months(1), "1m");
        assert_eq!(format_interval_months(11), "11m");
        assert_eq!(format_interval_months(12), "1y");
        assert_eq!(format_interval_months(24), "2y");
        assert_eq!(format_interval_months(18), "1y 6m");
        assert_eq!(format_interval_months(27), "2y 3m");
    }

    #[test]
    fn status_label_helpers_map_expected_short_forms() {
        assert_eq!(
            status_label_for_project_status(ProjectStatus::Ideating),
            "idea"
        );
        assert_eq!(
            status_label_for_project_status(ProjectStatus::Planned),
            "plan"
        );
        assert_eq!(
            status_label_for_project_status(ProjectStatus::Quoted),
            "bid"
        );
        assert_eq!(
            status_label_for_project_status(ProjectStatus::Underway),
            "wip"
        );
        assert_eq!(
            status_label_for_project_status(ProjectStatus::Delayed),
            "hold"
        );
        assert_eq!(
            status_label_for_project_status(ProjectStatus::Completed),
            "done"
        );
        assert_eq!(
            status_label_for_project_status(ProjectStatus::Abandoned),
            "drop"
        );

        assert_eq!(
            status_label_for_incident_status(micasa_app::IncidentStatus::Open),
            "open"
        );
        assert_eq!(
            status_label_for_incident_status(micasa_app::IncidentStatus::InProgress),
            "act"
        );
        assert_eq!(
            status_label_for_incident_status(micasa_app::IncidentStatus::Resolved),
            "resolved"
        );

        assert_eq!(
            status_label_for_incident_severity(IncidentSeverity::Urgent),
            "urg"
        );
        assert_eq!(
            status_label_for_incident_severity(IncidentSeverity::Soon),
            "soon"
        );
        assert_eq!(
            status_label_for_incident_severity(IncidentSeverity::Whenever),
            "low"
        );
    }

    #[test]
    fn projection_pipeline_compacts_status_interval_and_money_surfaces() {
        let project = Project {
            id: micasa_app::ProjectId::new(9),
            title: "Kitchen".to_owned(),
            project_type_id: ProjectTypeId::new(1),
            status: ProjectStatus::Planned,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: Some(523_423),
            actual_cents: Some(4_500_000),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            deleted_at: None,
        };
        let maintenance = micasa_app::MaintenanceItem {
            id: micasa_app::MaintenanceItemId::new(17),
            name: "HVAC filter".to_owned(),
            category_id: micasa_app::MaintenanceCategoryId::new(1),
            appliance_id: None,
            last_serviced_at: None,
            interval_months: 27,
            manual_url: String::new(),
            manual_text: String::new(),
            notes: String::new(),
            cost_cents: Some(10_000),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            deleted_at: None,
        };
        let incident = micasa_app::Incident {
            id: micasa_app::IncidentId::new(21),
            appliance_id: None,
            vendor_id: None,
            title: "Leak".to_owned(),
            description: String::new(),
            location: String::new(),
            status: micasa_app::IncidentStatus::Open,
            severity: IncidentSeverity::Urgent,
            date_noticed: Date::from_calendar_date(2026, Month::February, 12).expect("date"),
            date_resolved: None,
            cost_cents: Some(12_345),
            notes: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            deleted_at: None,
        };

        let project_snapshot = TabSnapshot::Projects(vec![project]);
        let maintenance_snapshot = TabSnapshot::Maintenance(vec![maintenance]);
        let incident_snapshot = TabSnapshot::Incidents(vec![incident]);
        let project_table_state = super::TableUiState {
            tab: Some(TabKind::Projects),
            ..super::TableUiState::default()
        };
        let maintenance_table_state = super::TableUiState {
            tab: Some(TabKind::Maintenance),
            ..super::TableUiState::default()
        };
        let incident_table_state = super::TableUiState {
            tab: Some(TabKind::Incidents),
            ..super::TableUiState::default()
        };

        let project_projection =
            super::projection_for_snapshot(&project_snapshot, &project_table_state);
        let maintenance_projection =
            super::projection_for_snapshot(&maintenance_snapshot, &maintenance_table_state);
        let incident_projection =
            super::projection_for_snapshot(&incident_snapshot, &incident_table_state);

        let project_row = &project_projection.rows[0];
        assert_eq!(project_row.cells[2].display(), "plan");
        assert_eq!(project_row.cells[3].display(), "5.2k");
        assert_eq!(project_row.cells[4].display(), "45k");
        assert_eq!(project_row.cells[3].display_with_mag_mode(true), "↑4");
        assert_eq!(
            header_label_for_column(&project_projection, &project_table_state, 3),
            "budget $"
        );
        assert_eq!(
            header_label_for_column(&project_projection, &project_table_state, 4),
            "actual $"
        );

        let maintenance_row = &maintenance_projection.rows[0];
        assert_eq!(maintenance_row.cells[5].display(), "2y 3m");
        assert_eq!(
            header_label_for_column(&maintenance_projection, &maintenance_table_state, 6),
            "cost $"
        );

        let incident_row = &incident_projection.rows[0];
        assert_eq!(incident_row.cells[2].display(), "open");
        assert_eq!(incident_row.cells[3].display(), "urg");
    }

    #[test]
    fn apply_mag_mode_to_text_formats_money_and_bare_numbers() {
        assert_eq!(
            apply_mag_mode_to_text("You spent $5,234.23 on kitchen.", true),
            "You spent $ ↑4 on kitchen."
        );
        assert_eq!(
            apply_mag_mode_to_text("Budget is $10,000.00 and actual is $8,500.00.", true),
            "Budget is $ ↑4 and actual is $ ↑4."
        );
        assert_eq!(
            apply_mag_mode_to_text("Loss of -$500.00 this month.", true),
            "Loss of -$ ↑3 this month."
        );
        assert_eq!(
            apply_mag_mode_to_text("The project is underway.", true),
            "The project is underway."
        );
        assert_eq!(apply_mag_mode_to_text("Just $5.00.", true), "Just $ ↑1.");
        assert_eq!(
            apply_mag_mode_to_text("There is 1 flooring project.", true),
            "There is ↑0 flooring project."
        );
        assert_eq!(
            apply_mag_mode_to_text("You have 42 maintenance items.", true),
            "You have ↑2 maintenance items."
        );
        assert_eq!(
            apply_mag_mode_to_text("Total is 1,000 items.", true),
            "Total is ↑3 items."
        );
        assert_eq!(
            apply_mag_mode_to_text("Found 3 projects totaling $15,000.00.", true),
            "Found ↑0 projects totaling $ ↑4."
        );
    }

    #[test]
    fn table_cell_mag_mode_skips_text_and_dates() {
        let date = Date::from_calendar_date(2026, Month::February, 12).expect("valid date");
        let text_cell = super::TableCell::Text("5551234567".to_owned());
        let date_cell = super::TableCell::Date(Some(date));
        assert_eq!(text_cell.display_with_mag_mode(true), "5551234567");
        assert_eq!(date_cell.display_with_mag_mode(true), "2026-02-12");
    }

    #[test]
    fn table_cell_mag_mode_formats_numeric_types() {
        let integer_cell = super::TableCell::Integer(42);
        let optional_integer_cell = super::TableCell::OptionalInteger(Some(1_000));
        let decimal_cell = super::TableCell::Decimal(Some(0.5));
        let zero_money_cell = super::TableCell::Money(Some(0));
        let money_cell = super::TableCell::Money(Some(523_423));
        assert_eq!(integer_cell.display_with_mag_mode(true), "↑2");
        assert_eq!(optional_integer_cell.display_with_mag_mode(true), "↑3");
        assert_eq!(decimal_cell.display_with_mag_mode(true), "↑0");
        assert_eq!(zero_money_cell.display_with_mag_mode(true), "↑-∞");
        assert_eq!(money_cell.display_with_mag_mode(true), "↑4");
        assert_eq!(
            super::TableCell::OptionalInteger(None).display_with_mag_mode(true),
            ""
        );
        assert_eq!(
            super::TableCell::Decimal(None).display_with_mag_mode(true),
            ""
        );
        assert_eq!(
            super::TableCell::Money(None).display_with_mag_mode(true),
            ""
        );
    }

    #[test]
    fn dashboard_toggle_persists_preference() {
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
        assert_eq!(runtime.show_dashboard_pref, Some(true));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT),
        );
        assert_eq!(runtime.show_dashboard_pref, Some(false));
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
    fn form_for_tab_maps_each_tab_to_expected_form_kind() {
        let cases = [
            (TabKind::Dashboard, None),
            (TabKind::House, Some(FormKind::HouseProfile)),
            (TabKind::Projects, Some(FormKind::Project)),
            (TabKind::Quotes, Some(FormKind::Quote)),
            (TabKind::Maintenance, Some(FormKind::MaintenanceItem)),
            (TabKind::ServiceLog, Some(FormKind::ServiceLogEntry)),
            (TabKind::Incidents, Some(FormKind::Incident)),
            (TabKind::Appliances, Some(FormKind::Appliance)),
            (TabKind::Vendors, Some(FormKind::Vendor)),
            (TabKind::Documents, Some(FormKind::Document)),
            (TabKind::Settings, None),
        ];

        for (tab, expected) in cases {
            assert_eq!(super::form_for_tab(tab), expected);
        }
    }

    #[test]
    fn edit_mode_e_routes_to_form_or_unavailable_by_tab_capability() {
        let tx = internal_tx();

        let mut vendor_state = AppState {
            active_tab: TabKind::Vendors,
            mode: AppMode::Edit,
            ..AppState::default()
        };
        let mut vendor_runtime = TestRuntime::default();
        let mut vendor_view_data = view_data_for_test();
        refresh_view_data(&vendor_state, &mut vendor_runtime, &mut vendor_view_data)
            .expect("refresh should work");

        handle_key_event(
            &mut vendor_state,
            &mut vendor_runtime,
            &mut vendor_view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        assert_eq!(vendor_state.mode, AppMode::Form(FormKind::Vendor));

        let mut dash_state = AppState {
            active_tab: TabKind::Dashboard,
            mode: AppMode::Edit,
            ..AppState::default()
        };
        let mut dash_runtime = TestRuntime::default();
        let mut dash_view_data = view_data_for_test();
        refresh_view_data(&dash_state, &mut dash_runtime, &mut dash_view_data)
            .expect("refresh should work");

        handle_key_event(
            &mut dash_state,
            &mut dash_runtime,
            &mut dash_view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        assert_eq!(dash_state.mode, AppMode::Edit);
        assert_eq!(dash_state.status_line.as_deref(), Some("edit unavailable"));
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
    fn form_mode_shortcuts_move_fields_and_apply_choice() {
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

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert_eq!(state.status_line.as_deref(), Some("field type (2/4)"));
        assert_eq!(
            view_data.form,
            Some(super::FormUiState {
                kind: FormKind::Project,
                field_index: 1,
            })
        );

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        assert_eq!(state.status_line.as_deref(), Some("field title (1/4)"));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert_eq!(state.status_line.as_deref(), Some("field status (3/4)"));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE),
        );
        assert_eq!(state.status_line.as_deref(), Some("project status quoted"));
        assert!(matches!(
            state.form_payload.as_ref(),
            Some(FormPayload::Project(input)) if input.status == ProjectStatus::Quoted
        ));
    }

    #[test]
    fn edit_mode_date_picker_supports_navigation_and_pick() {
        let mut state = AppState {
            active_tab: TabKind::ServiceLog,
            mode: AppMode::Edit,
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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 2);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        assert!(view_data.date_picker.visible);
        assert_eq!(
            view_data.date_picker.selected,
            Some(Date::from_calendar_date(2026, Month::January, 5).expect("valid date"))
        );

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
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
            KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE),
        );

        assert_eq!(
            view_data.date_picker.selected,
            Some(Date::from_calendar_date(2026, Month::December, 13).expect("valid date"))
        );

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(!view_data.date_picker.visible);
        assert_eq!(
            state.status_line.as_deref(),
            Some("date picked 2026-12-13; open full form to persist")
        );
    }

    #[test]
    fn date_picker_arrow_keys_match_hjkl_navigation() {
        let mut state = AppState::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        view_data.date_picker.visible = true;
        view_data.date_picker.selected =
            Some(Date::from_calendar_date(2026, Month::January, 31).expect("valid date"));

        handle_date_picker_key(
            &mut state,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        );
        handle_date_picker_key(
            &mut state,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        );
        assert_eq!(
            view_data.date_picker.selected,
            Some(Date::from_calendar_date(2026, Month::February, 8).expect("valid date"))
        );

        handle_date_picker_key(
            &mut state,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        );
        handle_date_picker_key(
            &mut state,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        );
        assert_eq!(
            view_data.date_picker.selected,
            Some(Date::from_calendar_date(2026, Month::January, 31).expect("valid date"))
        );
    }

    #[test]
    fn shift_date_by_months_clamps_from_jan_31_non_leap_year() {
        let date = Date::from_calendar_date(2025, Month::January, 31).expect("valid date");
        let shifted = shift_date_by_months(date, 1).expect("month shift should succeed");
        assert_eq!(
            shifted,
            Date::from_calendar_date(2025, Month::February, 28).expect("valid date")
        );
    }

    #[test]
    fn shift_date_by_months_clamps_from_jan_31_leap_year() {
        let date = Date::from_calendar_date(2024, Month::January, 31).expect("valid date");
        let shifted = shift_date_by_months(date, 1).expect("month shift should succeed");
        assert_eq!(
            shifted,
            Date::from_calendar_date(2024, Month::February, 29).expect("valid date")
        );
    }

    #[test]
    fn shift_date_by_years_clamps_from_feb_29_to_feb_28() {
        let date = Date::from_calendar_date(2024, Month::February, 29).expect("valid date");
        let shifted = shift_date_by_years(date, 1).expect("year shift should succeed");
        assert_eq!(
            shifted,
            Date::from_calendar_date(2025, Month::February, 28).expect("valid date")
        );
    }

    #[test]
    fn date_picker_month_navigation_key_clamps_end_of_month() {
        let mut state = AppState::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        view_data.date_picker.visible = true;
        view_data.date_picker.selected =
            Some(Date::from_calendar_date(2025, Month::January, 31).expect("valid date"));

        handle_date_picker_key(
            &mut state,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT),
        );

        assert_eq!(
            view_data.date_picker.selected,
            Some(Date::from_calendar_date(2025, Month::February, 28).expect("valid date"))
        );
    }

    #[test]
    fn shift_date_by_days_crosses_month_boundary() {
        let mut state = AppState::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        view_data.date_picker.visible = true;
        view_data.date_picker.selected =
            Some(Date::from_calendar_date(2026, Month::January, 31).expect("valid date"));

        handle_date_picker_key(
            &mut state,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );

        assert_eq!(
            view_data.date_picker.selected,
            Some(Date::from_calendar_date(2026, Month::February, 1).expect("valid date"))
        );
    }

    #[test]
    fn date_picker_year_navigation_key_clamps_feb_29() {
        let mut state = AppState::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        view_data.date_picker.visible = true;
        view_data.date_picker.selected =
            Some(Date::from_calendar_date(2024, Month::February, 29).expect("valid date"));

        handle_date_picker_key(
            &mut state,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE),
        );

        assert_eq!(
            view_data.date_picker.selected,
            Some(Date::from_calendar_date(2025, Month::February, 28).expect("valid date"))
        );
    }

    #[test]
    fn open_date_picker_on_empty_date_cell_defaults_to_today() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
            mode: AppMode::Edit,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..4 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );

        assert!(view_data.date_picker.visible);
        assert_eq!(view_data.date_picker.field_label, "recv");
        assert_eq!(view_data.date_picker.original, None);
        assert_eq!(
            view_data.date_picker.selected,
            Some(OffsetDateTime::now_utc().date())
        );
    }

    #[test]
    fn date_picker_overlay_text_renders_target_and_hints() {
        let picker = super::DatePickerUiState {
            visible: true,
            tab: Some(TabKind::ServiceLog),
            row_id: Some(19),
            column: 2,
            field_label: "date".to_owned(),
            original: Some(Date::from_calendar_date(2026, Month::January, 5).expect("valid date")),
            selected: Some(
                Date::from_calendar_date(2026, Month::February, 12).expect("valid date"),
            ),
        };

        let rendered = render_date_picker_overlay_text(&picker);
        assert!(rendered.contains("target: service#19 c2"));
        assert!(rendered.contains("field: date"));
        assert!(rendered.contains("orig: 2026-01-05"));
        assert!(rendered.contains("pick: 2026-02-12"));
        assert!(rendered.contains("h/l day | j/k week | H/L month | [/] year"));
        assert!(rendered.contains("enter pick | esc cancel"));
    }

    #[test]
    fn settings_tab_inline_edit_toggles_dashboard_preference() {
        let mut state = AppState {
            active_tab: TabKind::Settings,
            mode: AppMode::Edit,
            ..AppState::default()
        };
        let mut runtime = TestRuntime {
            show_dashboard_pref: Some(true),
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
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        assert_eq!(runtime.show_dashboard_pref, Some(false));
        assert_eq!(state.status_line.as_deref(), Some("dashboard startup off"));

        match view_data.active_tab_snapshot.as_ref() {
            Some(TabSnapshot::Settings(rows)) => {
                assert_eq!(rows[0].key, SettingKey::UiShowDashboard);
                assert_eq!(rows[0].value, SettingValue::Bool(false));
            }
            _ => panic!("expected settings snapshot"),
        }
    }

    #[test]
    fn settings_tab_inline_edit_cycles_llm_model() {
        let mut state = AppState {
            active_tab: TabKind::Settings,
            mode: AppMode::Edit,
            ..AppState::default()
        };
        let mut runtime = TestRuntime {
            available_models: vec!["qwen3".to_owned(), "qwen3:32b".to_owned()],
            active_model: Some("qwen3".to_owned()),
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
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_row, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        assert_eq!(runtime.active_model.as_deref(), Some("qwen3:32b"));
        assert_eq!(state.status_line.as_deref(), Some("llm model qwen3:32b"));
    }

    #[test]
    fn edit_mode_date_picker_esc_cancels_without_closing_chat() {
        let mut state = AppState {
            active_tab: TabKind::ServiceLog,
            mode: AppMode::Edit,
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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
        );
        assert!(view_data.date_picker.visible);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(!view_data.date_picker.visible);
        assert_eq!(state.mode, AppMode::Edit);
        assert_eq!(state.status_line.as_deref(), Some("date edit canceled"));
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
    fn page_navigation_keys_move_rows_in_nav_and_edit_modes() {
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
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_row, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_row, 0);

        state.mode = AppMode::Edit;
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
        );
        assert_eq!(view_data.table_state.selected_row, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_row, 0);
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
        assert!(!view_data.table_state.sorts.is_empty());

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
    fn filter_preview_and_active_modes_match_pinned_rows() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 2);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        assert!(view_data.table_state.pin.is_some());
        assert!(!view_data.table_state.filter_active);

        let preview_projection = super::active_projection(&view_data).expect("preview projection");
        assert_eq!(preview_projection.row_count(), 3, "preview keeps all rows");
        let preview_matches = preview_projection
            .rows
            .iter()
            .filter(|row| super::row_matches_pin(row, &view_data.table_state))
            .count();
        assert_eq!(preview_matches, 2, "two quote rows share vendor id 7");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.filter_active);

        let active_projection = super::active_projection(&view_data).expect("active projection");
        assert_eq!(
            active_projection.row_count(),
            2,
            "active filter hides non-matches"
        );
        assert!(
            active_projection
                .rows
                .iter()
                .all(|row| super::row_matches_pin(row, &view_data.table_state))
        );
    }

    #[test]
    fn filter_inversion_flips_preview_and_active_match_behavior() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 2);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.filter_inverted);
        assert!(!view_data.table_state.filter_active);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.filter_active);
        let inverted_active = super::active_projection(&view_data).expect("active projection");
        assert_eq!(inverted_active.row_count(), 1);
        assert!(
            inverted_active
                .rows
                .iter()
                .all(|row| !super::row_matches_pin(row, &view_data.table_state))
        );

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
        );
        assert!(!view_data.table_state.filter_inverted);
        let normal_active = super::active_projection(&view_data).expect("active projection");
        assert_eq!(normal_active.row_count(), 2);
        assert!(
            normal_active
                .rows
                .iter()
                .all(|row| super::row_matches_pin(row, &view_data.table_state))
        );
    }

    #[test]
    fn clear_pins_resets_filter_inversion() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.filter_inverted);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        assert!(view_data.table_state.pin.is_none());
        assert!(!view_data.table_state.filter_active);
        assert!(!view_data.table_state.filter_inverted);
    }

    #[test]
    fn hide_pinned_column_clears_pin_and_deactivates_filter() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 2);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.pin.is_some());
        assert!(view_data.table_state.filter_active);
        assert!(view_data.table_state.filter_inverted);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        );
        assert!(view_data.table_state.hidden_columns.contains(&2));
        assert!(view_data.table_state.pin.is_none());
        assert!(!view_data.table_state.filter_active);
        assert!(!view_data.table_state.filter_inverted);
    }

    #[test]
    fn pin_and_filter_keys_are_blocked_while_dashboard_overlay_is_visible() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
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
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.pin.is_none());
        assert!(!view_data.table_state.filter_active);
        assert!(!view_data.table_state.filter_inverted);
    }

    #[test]
    fn invert_toggle_round_trip_without_pin() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        assert!(!view_data.table_state.filter_inverted);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.filter_inverted);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
        );
        assert!(!view_data.table_state.filter_inverted);
    }

    #[test]
    fn filter_marker_transitions_follow_keybindings() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        let tab_title = |state: &AppState, view_data: &ViewData| {
            super::tab_title(state.active_tab, state, &view_data.table_state)
        };

        assert!(!tab_title(&state, &view_data).contains(super::FILTER_MARK_PREVIEW));
        assert!(!tab_title(&state, &view_data).contains(super::FILTER_MARK_ACTIVE));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        assert!(tab_title(&state, &view_data).contains(super::FILTER_MARK_PREVIEW));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        assert!(tab_title(&state, &view_data).contains(super::FILTER_MARK_ACTIVE));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
        );
        assert!(tab_title(&state, &view_data).contains(super::FILTER_MARK_ACTIVE_INVERTED));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        assert!(tab_title(&state, &view_data).contains(super::FILTER_MARK_PREVIEW_INVERTED));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        let title = tab_title(&state, &view_data);
        assert!(!title.contains(super::FILTER_MARK_ACTIVE));
        assert!(!title.contains(super::FILTER_MARK_ACTIVE_INVERTED));
        assert!(!title.contains(super::FILTER_MARK_PREVIEW));
        assert!(!title.contains(super::FILTER_MARK_PREVIEW_INVERTED));
    }

    #[test]
    fn inverted_null_pin_filter_keeps_only_non_null_rows() {
        let snapshot = TabSnapshot::ServiceLog(vec![
            TestRuntime::sample_service_log(1, 2, None, "no vendor"),
            TestRuntime::sample_service_log(2, 2, Some(7), "vendor one"),
            TestRuntime::sample_service_log(3, 2, Some(8), "vendor two"),
        ]);

        let normal = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                tab: Some(TabKind::ServiceLog),
                pin: Some(super::PinnedCell {
                    column: 3,
                    value: super::TableCell::OptionalInteger(None),
                }),
                filter_active: true,
                ..super::TableUiState::default()
            },
        );
        assert_eq!(normal.row_count(), 1);
        assert!(matches!(
            normal.rows[0].cells.get(3),
            Some(super::TableCell::OptionalInteger(None))
        ));

        let inverted = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                tab: Some(TabKind::ServiceLog),
                pin: Some(super::PinnedCell {
                    column: 3,
                    value: super::TableCell::OptionalInteger(None),
                }),
                filter_active: true,
                filter_inverted: true,
                ..super::TableUiState::default()
            },
        );
        assert_eq!(inverted.row_count(), 2);
        assert!(inverted.rows.iter().all(|row| matches!(
            row.cells.get(3),
            Some(super::TableCell::OptionalInteger(Some(_)))
        )));
    }

    #[test]
    fn text_pin_matching_is_case_insensitive() {
        let snapshot = TabSnapshot::Projects(vec![
            TestRuntime::sample_project(1, "Plan"),
            TestRuntime::sample_project(2, "PLAN"),
            TestRuntime::sample_project(3, "Done"),
        ]);

        let preview_state = super::TableUiState {
            tab: Some(TabKind::Projects),
            pin: Some(super::PinnedCell {
                column: 1,
                value: super::TableCell::Text("plan".to_owned()),
            }),
            ..super::TableUiState::default()
        };

        let preview = super::projection_for_snapshot(&snapshot, &preview_state);
        let preview_matches = preview
            .rows
            .iter()
            .filter(|row| super::row_matches_pin(row, &preview_state))
            .count();
        assert_eq!(preview_matches, 2);

        let active = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                filter_active: true,
                ..preview_state
            },
        );
        assert_eq!(active.row_count(), 2);
        assert!(active.rows.iter().all(|row| {
            matches!(
                row.cells.get(1),
                Some(super::TableCell::Text(value))
                    if value.eq_ignore_ascii_case("plan")
            )
        }));
    }

    #[test]
    fn toggle_pin_with_different_text_case_clears_existing_pin() {
        let mut view_data = view_data_for_test();
        view_data.active_tab_snapshot = Some(TabSnapshot::Projects(vec![
            TestRuntime::sample_project(1, "Plan"),
            TestRuntime::sample_project(2, "PLAN"),
        ]));
        view_data.table_state.tab = Some(TabKind::Projects);
        view_data.table_state.selected_col = 1;
        view_data.table_state.selected_row = 0;

        let first = super::toggle_pin(&mut view_data);
        assert!(matches!(first, super::TableStatus::PinOn(_)));
        assert!(view_data.table_state.pin.is_some());

        view_data.table_state.selected_row = 1;
        let second = super::toggle_pin(&mut view_data);
        assert_eq!(second, super::TableStatus::PinOff);
        assert!(view_data.table_state.pin.is_none());
    }

    #[test]
    fn multi_column_sort_cycles_per_column_and_keeps_priority() {
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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.sorts.len(), 1);
        assert_eq!(view_data.table_state.sorts[0].column, 1);
        assert_eq!(view_data.table_state.sorts[0].direction, SortDirection::Asc);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.sorts.len(), 2);
        assert_eq!(view_data.table_state.sorts[0].column, 1);
        assert_eq!(view_data.table_state.sorts[1].column, 2);
        assert_eq!(view_data.table_state.sorts[1].direction, SortDirection::Asc);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.sorts.len(), 2);
        assert_eq!(
            view_data.table_state.sorts[1].direction,
            SortDirection::Desc
        );

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.sorts.len(), 1);
        assert_eq!(view_data.table_state.sorts[0].column, 1);
    }

    #[test]
    fn sort_keeps_null_money_last_regardless_of_direction() {
        let low = TestRuntime::sample_project(2, "Low");
        let high = TestRuntime::sample_project(3, "High");
        let mut missing = TestRuntime::sample_project(1, "Missing");
        missing.budget_cents = None;

        let snapshot = TabSnapshot::Projects(vec![high, missing, low]);

        let asc_projection = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                sorts: vec![super::SortSpec {
                    column: 3,
                    direction: SortDirection::Asc,
                }],
                ..super::TableUiState::default()
            },
        );
        let asc_ids = asc_projection
            .rows
            .iter()
            .filter_map(|row| match row.cells.first() {
                Some(super::TableCell::Integer(id)) => Some(*id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(asc_ids, vec![2, 3, 1]);

        let desc_projection = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                sorts: vec![super::SortSpec {
                    column: 3,
                    direction: SortDirection::Desc,
                }],
                ..super::TableUiState::default()
            },
        );
        let desc_ids = desc_projection
            .rows
            .iter()
            .filter_map(|row| match row.cells.first() {
                Some(super::TableCell::Integer(id)) => Some(*id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(desc_ids, vec![3, 2, 1]);
    }

    #[test]
    fn sort_uses_id_tiebreaker_for_equal_sort_values() {
        let p3 = TestRuntime::sample_project(3, "Same");
        let p1 = TestRuntime::sample_project(1, "Same");
        let p2 = TestRuntime::sample_project(2, "Same");

        let snapshot = TabSnapshot::Projects(vec![p3, p1, p2]);
        let projection = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                sorts: vec![super::SortSpec {
                    column: 1,
                    direction: SortDirection::Desc,
                }],
                ..super::TableUiState::default()
            },
        );
        let ids = projection
            .rows
            .iter()
            .filter_map(|row| match row.cells.first() {
                Some(super::TableCell::Integer(id)) => Some(*id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn sort_text_is_case_insensitive_for_projects() {
        let p1 = TestRuntime::sample_project(1, "charlie");
        let p2 = TestRuntime::sample_project(2, "Alice");
        let p3 = TestRuntime::sample_project(3, "bob");
        let snapshot = TabSnapshot::Projects(vec![p1, p2, p3]);

        let projection = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                sorts: vec![super::SortSpec {
                    column: 1,
                    direction: SortDirection::Asc,
                }],
                ..super::TableUiState::default()
            },
        );
        let titles = projection
            .rows
            .iter()
            .filter_map(|row| match row.cells.get(1) {
                Some(super::TableCell::Text(value)) => Some(value.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["Alice", "bob", "charlie"]);
    }

    #[test]
    fn sort_money_ascending_orders_projects_by_budget() {
        let mut p1 = TestRuntime::sample_project(1, "one");
        let mut p2 = TestRuntime::sample_project(2, "two");
        let mut p3 = TestRuntime::sample_project(3, "three");
        p1.budget_cents = Some(20_000);
        p2.budget_cents = Some(5_000);
        p3.budget_cents = Some(100_000);
        let snapshot = TabSnapshot::Projects(vec![p1, p2, p3]);

        let projection = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                sorts: vec![super::SortSpec {
                    column: 3,
                    direction: SortDirection::Asc,
                }],
                ..super::TableUiState::default()
            },
        );
        let ids = projection
            .rows
            .iter()
            .filter_map(|row| match row.cells.first() {
                Some(super::TableCell::Integer(id)) => Some(*id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![2, 1, 3]);
    }

    #[test]
    fn sort_date_descending_orders_incidents_by_noticed_date() {
        let mut i1 = TestRuntime::sample_incident(1, "first");
        let mut i2 = TestRuntime::sample_incident(2, "second");
        let mut i3 = TestRuntime::sample_incident(3, "third");
        i1.date_noticed = Date::from_calendar_date(2026, Month::January, 3).expect("valid date");
        i2.date_noticed = Date::from_calendar_date(2026, Month::February, 10).expect("valid date");
        i3.date_noticed = Date::from_calendar_date(2025, Month::December, 28).expect("valid date");
        let snapshot = TabSnapshot::Incidents(vec![i1, i2, i3]);

        let projection = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                sorts: vec![super::SortSpec {
                    column: 4,
                    direction: SortDirection::Desc,
                }],
                ..super::TableUiState::default()
            },
        );
        let ids = projection
            .rows
            .iter()
            .filter_map(|row| match row.cells.first() {
                Some(super::TableCell::Integer(id)) => Some(*id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![2, 1, 3]);
    }

    #[test]
    fn multi_key_sort_orders_quotes_by_project_then_vendor() {
        let q1 = TestRuntime::sample_quote(1, 2, 20);
        let q2 = TestRuntime::sample_quote(2, 1, 30);
        let q3 = TestRuntime::sample_quote(3, 1, 10);
        let q4 = TestRuntime::sample_quote(4, 2, 10);
        let snapshot = TabSnapshot::Quotes(vec![q1, q2, q3, q4]);

        let projection = super::projection_for_snapshot(
            &snapshot,
            &super::TableUiState {
                sorts: vec![
                    super::SortSpec {
                        column: 1,
                        direction: SortDirection::Asc,
                    },
                    super::SortSpec {
                        column: 2,
                        direction: SortDirection::Asc,
                    },
                ],
                ..super::TableUiState::default()
            },
        );

        let keys = projection
            .rows
            .iter()
            .filter_map(|row| match (row.cells.get(1), row.cells.get(2)) {
                (
                    Some(super::TableCell::Integer(project)),
                    Some(super::TableCell::Integer(vendor)),
                ) => Some((*project, *vendor)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(keys, vec![(1, 10), (1, 30), (2, 10), (2, 20)]);
    }

    #[test]
    fn hiding_columns_updates_cursor_and_skips_hidden_columns() {
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
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        );
        assert!(view_data.table_state.hidden_columns.contains(&0));
        assert_eq!(view_data.table_state.selected_col, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT),
        );
        assert!(view_data.table_state.hidden_columns.is_empty());
    }

    #[test]
    fn column_finder_jumps_to_hidden_column_and_unhides_it() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        view_data.table_state.hidden_columns.insert(3);
        super::clamp_table_cursor(&mut view_data);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );
        assert!(view_data.column_finder.visible);

        for key in ['b', 'u', 'd'] {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert!(!view_data.column_finder.visible);
        assert_eq!(view_data.table_state.selected_col, 3);
        assert!(!view_data.table_state.hidden_columns.contains(&3));
    }

    #[test]
    fn slash_opens_column_finder_in_nav_mode() {
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
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );

        assert!(view_data.column_finder.visible);
        assert_eq!(state.status_line.as_deref(), Some("column finder open"));
        let overlay = super::render_column_finder_overlay_text(&view_data);
        assert!(overlay.contains("query:"));
        assert!(overlay.contains("enter jump"));
    }

    #[test]
    fn slash_is_blocked_in_edit_mode() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            mode: AppMode::Edit,
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
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );

        assert!(!view_data.column_finder.visible);
        assert_eq!(state.mode, AppMode::Edit);
    }

    #[test]
    fn slash_is_blocked_while_dashboard_overlay_is_visible() {
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
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );

        assert!(view_data.dashboard.visible);
        assert!(!view_data.column_finder.visible);
    }

    #[test]
    fn column_finder_typing_backspace_and_ctrl_u_update_query() {
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
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );
        assert!(view_data.column_finder.visible);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.column_finder.query, "id");

        let projection =
            super::active_projection(&view_data).expect("column finder should have an active tab");
        let narrowed = super::column_finder_matches(
            &projection,
            &view_data.table_state.hidden_columns,
            &view_data.column_finder.query,
        );
        assert_eq!(narrowed.len(), 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        assert_eq!(view_data.column_finder.query, "i");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        );
        assert!(view_data.column_finder.query.is_empty());
    }

    #[test]
    fn column_finder_backspace_handles_multibyte_characters() {
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
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('ü'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.column_finder.query, "üx");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        assert_eq!(view_data.column_finder.query, "ü");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        assert!(view_data.column_finder.query.is_empty());
    }

    #[test]
    fn column_finder_cursor_clamps_when_query_narrows() {
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
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );
        view_data.column_finder.cursor = 999;

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
        );

        let projection =
            super::active_projection(&view_data).expect("column finder should have an active tab");
        let matches = super::column_finder_matches(
            &projection,
            &view_data.table_state.hidden_columns,
            &view_data.column_finder.query,
        );
        assert!(!matches.is_empty());
        assert_eq!(
            view_data.column_finder.cursor,
            matches.len().saturating_sub(1)
        );
    }

    #[test]
    fn column_finder_navigation_and_escape_behave_like_go() {
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
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.column_finder.cursor, 0);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        );
        assert_eq!(view_data.column_finder.cursor, 0);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        );
        assert_eq!(view_data.column_finder.cursor, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
        );
        assert_eq!(view_data.column_finder.cursor, 0);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(!view_data.column_finder.visible);
        assert_eq!(state.status_line.as_deref(), Some("column finder closed"));
    }

    #[test]
    fn column_finder_highlights_fuzzy_matches() {
        let rendered = highlight_column_label("budget", "bdg");
        assert_eq!(rendered, "[b]u[d][g]et");
    }

    #[test]
    fn enter_on_notes_column_opens_note_preview_overlay() {
        let mut state = AppState {
            active_tab: TabKind::ServiceLog,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..5 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 5);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(view_data.note_preview.visible);
        assert!(view_data.note_preview.text.contains("Inspect vent"));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );
        assert!(!view_data.note_preview.visible);
    }

    #[test]
    fn enter_on_empty_notes_column_does_not_open_preview() {
        let mut state = AppState {
            active_tab: TabKind::ServiceLog,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();

        view_data.active_tab_snapshot = Some(TabSnapshot::ServiceLog(vec![
            TestRuntime::sample_service_log(91, 2, Some(7), ""),
        ]));
        view_data.table_state.tab = Some(TabKind::ServiceLog);
        view_data.table_state.selected_row = 0;
        view_data.table_state.selected_col = 5;
        super::clamp_table_cursor(&mut view_data);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert!(!view_data.note_preview.visible);
        assert_eq!(state.status_line.as_deref(), Some("no note to preview"));
    }

    #[test]
    fn note_preview_closes_before_other_keys_apply() {
        let mut state = AppState {
            active_tab: TabKind::ServiceLog,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        view_data.note_preview.visible = true;
        view_data.note_preview.title = "service notes".to_owned();
        view_data.note_preview.text = "Inspect vent".to_owned();
        view_data.table_state.selected_row = 0;

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );

        assert!(!view_data.note_preview.visible);
        assert_eq!(view_data.table_state.selected_row, 0);
    }

    #[test]
    fn note_preview_overlay_text_renders_title_body_and_close_hint() {
        let rendered = render_note_preview_overlay_text(&super::NotePreviewUiState {
            visible: true,
            title: "service notes".to_owned(),
            text: "Inspect vent before summer.".to_owned(),
        });
        assert!(rendered.contains("service notes"));
        assert!(rendered.contains("Inspect vent before summer."));
        assert!(rendered.contains("press any key to close"));
    }

    #[test]
    fn contextual_enter_hint_is_preview_for_notes_column() {
        let state = AppState {
            active_tab: TabKind::ServiceLog,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        view_data.table_state.selected_col = 5;
        assert_eq!(contextual_enter_hint(&view_data), "preview");
    }

    #[test]
    fn edit_mode_blocks_non_navigation_table_commands() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            mode: AppMode::Edit,
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
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        );

        assert!(view_data.table_state.sorts.is_empty());
        assert!(view_data.table_state.hidden_columns.is_empty());

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_row, 1);
    }

    #[test]
    fn enter_in_nav_follows_linked_foreign_key() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(state.active_tab, TabKind::Projects);
        assert_eq!(view_data.table_state.selected_row, 1);
    }

    #[test]
    fn drilldown_enter_opens_detail_stack_and_esc_unwinds() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 6);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.detail_stack.len(), 1);
        assert_eq!(view_data.table_state.tab, Some(TabKind::Maintenance));

        for _ in 0..7 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 7);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.detail_stack.len(), 2);
        assert_eq!(view_data.table_state.tab, Some(TabKind::ServiceLog));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert_eq!(view_data.detail_stack.len(), 1);
        assert_eq!(view_data.table_state.tab, Some(TabKind::Maintenance));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(view_data.detail_stack.is_empty());
        assert_eq!(view_data.table_state.tab, Some(TabKind::Appliances));
    }

    #[test]
    fn esc_in_edit_mode_keeps_detail_stack_open() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.detail_stack.len(), 1);

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
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert_eq!(state.mode, AppMode::Nav);
        assert_eq!(view_data.detail_stack.len(), 1);
        assert_eq!(view_data.table_state.tab, Some(TabKind::Maintenance));
    }

    #[test]
    fn tab_switch_is_blocked_while_detail_stack_open() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.detail_stack.len(), 1);
        let before_tab = state.active_tab;

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
        );
        assert_eq!(state.active_tab, before_tab);
        assert_eq!(view_data.detail_stack.len(), 1);
        assert_eq!(view_data.table_state.tab, Some(TabKind::Maintenance));
        assert_eq!(
            state.status_line.as_deref(),
            Some("close detail first"),
            "blocking message should be actionable"
        );
    }

    #[test]
    fn tab_key_is_blocked_while_detail_stack_open() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.detail_stack.len(), 1);
        let before_tab = state.active_tab;

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert_eq!(state.active_tab, before_tab);
        assert_eq!(view_data.detail_stack.len(), 1);
        assert_eq!(
            state.status_line.as_deref(),
            Some("close detail first"),
            "blocking message should be actionable"
        );
    }

    #[test]
    fn following_link_from_detail_closes_detail_stack() {
        let mut state = AppState {
            active_tab: TabKind::Vendors,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..5 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Quotes));
        assert_eq!(view_data.detail_stack.len(), 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert!(view_data.detail_stack.is_empty());
        assert_eq!(state.active_tab, TabKind::Projects);
        assert_eq!(view_data.table_state.tab, Some(TabKind::Projects));
    }

    #[test]
    fn column_navigation_moves_within_detail_stack() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Maintenance));

        let initial_col = view_data.table_state.selected_col;
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_ne!(view_data.table_state.selected_col, initial_col);
    }

    #[test]
    fn breadcrumbs_multi_level_include_nested_drill_titles() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        let first_breadcrumb = render_breadcrumb_text(&state, &view_data);
        assert!(first_breadcrumb.contains("appliances"));
        assert!(first_breadcrumb.contains("maintenance (Furnace)"));

        for _ in 0..7 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        let nested_breadcrumb = render_breadcrumb_text(&state, &view_data);
        assert!(nested_breadcrumb.contains("maintenance (Furnace)"));
        assert!(nested_breadcrumb.contains("service log (HVAC filter)"));
    }

    #[test]
    fn selected_row_metadata_uses_detail_tab_rows() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        let selected = super::selected_row_metadata(&view_data).map(|(row_id, _)| row_id);
        assert_eq!(selected, Some(2));
    }

    #[test]
    fn selected_cell_uses_detail_tab_projection() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        let selected = super::selected_cell(&view_data).map(|(_, value)| value.display());
        assert_eq!(selected.as_deref(), Some("HVAC filter"));
    }

    #[test]
    fn sort_command_works_while_detail_stack_is_open() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(view_data.table_state.sorts.is_empty());

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );

        assert_eq!(view_data.table_state.sorts.len(), 1);
        assert_eq!(view_data.table_state.sorts[0].column, 0);
        assert_eq!(view_data.table_state.sorts[0].direction, SortDirection::Asc);
    }

    #[test]
    fn close_all_detail_snapshots_collapses_nested_stack() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..6 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        for _ in 0..7 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.detail_stack.len(), 2);

        super::close_all_detail_snapshots(&mut view_data);
        assert!(view_data.detail_stack.is_empty());
        assert_eq!(view_data.table_state.tab, Some(TabKind::Appliances));
    }

    #[test]
    fn close_all_detail_snapshots_is_noop_when_stack_is_empty() {
        let mut view_data = view_data_for_test();
        view_data.table_state.tab = Some(TabKind::Projects);
        view_data.table_state.selected_row = 1;
        view_data.table_state.selected_col = 2;
        assert!(view_data.detail_stack.is_empty());

        super::close_all_detail_snapshots(&mut view_data);

        assert!(view_data.detail_stack.is_empty());
        assert_eq!(view_data.table_state.tab, Some(TabKind::Projects));
        assert_eq!(view_data.table_state.selected_row, 1);
        assert_eq!(view_data.table_state.selected_col, 2);
    }

    #[test]
    fn push_and_pop_detail_snapshot_restore_parent_context() {
        let state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        view_data.table_state.selected_row = 1;
        view_data.table_state.selected_col = 1;
        view_data.table_state.sorts = vec![super::SortSpec {
            column: 1,
            direction: SortDirection::Desc,
        }];
        view_data.table_state.pin = Some(super::PinnedCell {
            column: 1,
            value: super::TableCell::Text("Beta".to_owned()),
        });
        view_data.column_finder.visible = true;
        view_data.column_finder.query = "ti".to_owned();
        view_data.note_preview.visible = true;
        view_data.note_preview.title = "notes".to_owned();
        view_data.note_preview.text = "detail text".to_owned();
        view_data.date_picker.visible = true;
        view_data.date_picker.column = 2;

        let parent_snapshot = view_data.active_tab_snapshot.clone();
        let parent_table_state = view_data.table_state.clone();

        super::push_detail_snapshot(
            &mut view_data,
            "maintenance (Furnace)",
            TabSnapshot::Maintenance(vec![TestRuntime::sample_maintenance(
                99,
                Some(2),
                "Filter swap",
            )]),
        );

        assert_eq!(view_data.detail_stack.len(), 1);
        assert_eq!(view_data.detail_stack[0].title, "maintenance (Furnace)");
        assert_eq!(view_data.table_state.tab, Some(TabKind::Maintenance));
        assert!(view_data.table_state.sorts.is_empty());
        assert!(view_data.table_state.pin.is_none());
        assert!(!view_data.column_finder.visible);
        assert!(!view_data.note_preview.visible);
        assert!(!view_data.date_picker.visible);

        view_data.column_finder.visible = true;
        view_data.note_preview.visible = true;
        view_data.date_picker.visible = true;
        let popped = super::pop_detail_snapshot(&mut view_data);
        assert!(popped);
        assert!(view_data.detail_stack.is_empty());
        assert_eq!(view_data.active_tab_snapshot, parent_snapshot);
        assert_eq!(view_data.table_state, parent_table_state);
        assert!(!view_data.column_finder.visible);
        assert!(!view_data.note_preview.visible);
        assert!(!view_data.date_picker.visible);
    }

    #[test]
    fn close_all_detail_snapshots_restore_root_table_state_after_nested_pushes() {
        let state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        view_data.table_state.selected_row = 1;
        view_data.table_state.selected_col = 1;
        view_data.table_state.sorts = vec![super::SortSpec {
            column: 1,
            direction: SortDirection::Asc,
        }];
        view_data.table_state.pin = Some(super::PinnedCell {
            column: 1,
            value: super::TableCell::Text("Beta".to_owned()),
        });
        let root_snapshot = view_data.active_tab_snapshot.clone();
        let root_table_state = view_data.table_state.clone();

        super::push_detail_snapshot(
            &mut view_data,
            "maintenance (Furnace)",
            TabSnapshot::Maintenance(vec![TestRuntime::sample_maintenance(
                99,
                Some(2),
                "Filter swap",
            )]),
        );
        view_data.table_state.selected_col = 7;

        super::push_detail_snapshot(
            &mut view_data,
            "service log (Filter swap)",
            TabSnapshot::ServiceLog(vec![TestRuntime::sample_service_log(
                45,
                99,
                Some(8),
                "done",
            )]),
        );
        assert_eq!(view_data.detail_stack.len(), 2);

        super::close_all_detail_snapshots(&mut view_data);
        assert!(view_data.detail_stack.is_empty());
        assert_eq!(view_data.active_tab_snapshot, root_snapshot);
        assert_eq!(view_data.table_state, root_table_state);
    }

    #[test]
    fn pop_detail_snapshot_returns_false_when_stack_is_empty() {
        let mut view_data = view_data_for_test();
        assert!(view_data.detail_stack.is_empty());

        let popped = super::pop_detail_snapshot(&mut view_data);
        assert!(!popped);
    }

    #[test]
    fn drill_title_for_uses_selected_label_when_present() {
        let title = super::drill_title_for(
            TabKind::Projects,
            "Kitchen Remodel".to_owned(),
            super::DrillRequest::QuotesForProject(micasa_app::ProjectId::new(7)),
        );
        assert_eq!(title, "quotes (Kitchen Remodel)");
    }

    #[test]
    fn drill_title_for_falls_back_to_plain_title_when_label_empty() {
        let title = super::drill_title_for(
            TabKind::Vendors,
            "   ".to_owned(),
            super::DrillRequest::ServiceLogForVendor(micasa_app::VendorId::new(7)),
        );
        assert_eq!(title, "jobs");
    }

    #[test]
    fn maintenance_projection_columns_include_log_and_not_manual() {
        let projection = super::projection_for_snapshot(
            &TabSnapshot::Maintenance(vec![TestRuntime::sample_maintenance(
                2,
                Some(4),
                "HVAC filter",
            )]),
            &super::TableUiState {
                tab: Some(TabKind::Maintenance),
                ..super::TableUiState::default()
            },
        );

        assert_eq!(projection.columns.last().copied(), Some("log"));
        assert!(!projection.columns.contains(&"manual"));
    }

    #[test]
    fn appliance_projection_columns_include_maint_and_docs() {
        let projection = super::projection_for_snapshot(
            &TabSnapshot::Appliances(vec![TestRuntime::sample_appliance(4, "Furnace")]),
            &super::TableUiState {
                tab: Some(TabKind::Appliances),
                ..super::TableUiState::default()
            },
        );

        assert_eq!(projection.columns.len(), 8);
        assert_eq!(projection.columns[6], "maint");
        assert_eq!(projection.columns[7], "docs");
    }

    #[test]
    fn project_projection_columns_include_quotes_and_docs() {
        let projection = super::projection_for_snapshot(
            &TabSnapshot::Projects(vec![TestRuntime::sample_project(1, "Alpha")]),
            &super::TableUiState {
                tab: Some(TabKind::Projects),
                ..super::TableUiState::default()
            },
        );

        assert_eq!(projection.columns.len(), 7);
        assert_eq!(projection.columns[5], "quotes");
        assert_eq!(projection.columns[6], "docs");
    }

    #[test]
    fn service_log_vendor_cell_link_target_depends_on_vendor_presence() {
        let with_vendor = super::projection_for_snapshot(
            &TabSnapshot::ServiceLog(vec![TestRuntime::sample_service_log(
                19,
                2,
                Some(7),
                "vendor visit",
            )]),
            &super::TableUiState {
                tab: Some(TabKind::ServiceLog),
                ..super::TableUiState::default()
            },
        );
        let without_vendor = super::projection_for_snapshot(
            &TabSnapshot::ServiceLog(vec![TestRuntime::sample_service_log(
                20,
                2,
                None,
                "self performed",
            )]),
            &super::TableUiState {
                tab: Some(TabKind::ServiceLog),
                ..super::TableUiState::default()
            },
        );

        let with_vendor_cell = with_vendor.rows[0].cells[3].clone();
        let without_vendor_cell = without_vendor.rows[0].cells[3].clone();
        assert!(super::cell_has_link_target(&with_vendor_cell));
        assert!(!super::cell_has_link_target(&without_vendor_cell));
    }

    #[test]
    fn project_drilldowns_filter_quotes_and_documents() {
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
        assert_eq!(view_data.table_state.selected_row, 1);

        for _ in 0..5 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 5);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Quotes));
        match view_data.active_tab_snapshot.as_ref() {
            Some(TabSnapshot::Quotes(rows)) => {
                assert_eq!(rows.len(), 1);
                assert!(rows.iter().all(|row| row.project_id.get() == 2));
            }
            _ => panic!("expected quote drill snapshot"),
        }

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Projects));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 6);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Documents));
        match view_data.active_tab_snapshot.as_ref() {
            Some(TabSnapshot::Documents(rows)) => {
                assert_eq!(rows.len(), 1);
                assert!(rows.iter().all(|row| {
                    row.entity_kind == micasa_app::DocumentEntityKind::Project && row.entity_id == 2
                }));
            }
            _ => panic!("expected document drill snapshot"),
        }
    }

    #[test]
    fn vendor_drilldowns_filter_quotes_and_jobs() {
        let mut state = AppState {
            active_tab: TabKind::Vendors,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..5 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 5);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Quotes));
        match view_data.active_tab_snapshot.as_ref() {
            Some(TabSnapshot::Quotes(rows)) => {
                assert_eq!(rows.len(), 2);
                assert!(rows.iter().all(|row| row.vendor_id.get() == 7));
            }
            _ => panic!("expected quote drill snapshot"),
        }

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Vendors));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.selected_col, 6);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::ServiceLog));
        match view_data.active_tab_snapshot.as_ref() {
            Some(TabSnapshot::ServiceLog(rows)) => {
                assert_eq!(rows.len(), 1);
                assert!(
                    rows.iter()
                        .all(|row| row.vendor_id.map(|id| id.get()) == Some(7))
                );
            }
            _ => panic!("expected service log drill snapshot"),
        }
    }

    #[test]
    fn incident_document_drilldown_filters_rows() {
        let mut state = AppState {
            active_tab: TabKind::Incidents,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..7 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 7);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Documents));
        match view_data.active_tab_snapshot.as_ref() {
            Some(TabSnapshot::Documents(rows)) => {
                assert_eq!(rows.len(), 1);
                assert!(rows.iter().all(|row| {
                    row.entity_kind == micasa_app::DocumentEntityKind::Incident
                        && row.entity_id == 6
                }));
            }
            _ => panic!("expected document drill snapshot"),
        }
    }

    #[test]
    fn maintenance_log_drilldown_filters_rows() {
        let mut state = AppState {
            active_tab: TabKind::Maintenance,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..7 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 7);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::ServiceLog));

        match view_data.active_tab_snapshot.as_ref() {
            Some(TabSnapshot::ServiceLog(rows)) => {
                assert_eq!(rows.len(), 1);
                assert!(rows.iter().all(|row| row.maintenance_item_id.get() == 2));
            }
            _ => panic!("expected service-log drill snapshot"),
        }
    }

    #[test]
    fn appliance_document_drilldown_filters_rows() {
        let mut state = AppState {
            active_tab: TabKind::Appliances,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..7 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 7);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(view_data.table_state.tab, Some(TabKind::Documents));

        match view_data.active_tab_snapshot.as_ref() {
            Some(TabSnapshot::Documents(rows)) => {
                assert_eq!(rows.len(), 1);
                assert!(rows.iter().all(|row| {
                    row.entity_kind == micasa_app::DocumentEntityKind::Appliance
                        && row.entity_id == 4
                }));
            }
            _ => panic!("expected document drill snapshot"),
        }
    }

    #[test]
    fn service_log_vendor_link_follows_to_vendor_tab() {
        let mut state = AppState {
            active_tab: TabKind::ServiceLog,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        for _ in 0..3 {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            );
        }
        assert_eq!(view_data.table_state.selected_col, 3);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(state.active_tab, TabKind::Vendors);
        assert_eq!(view_data.table_state.tab, Some(TabKind::Vendors));
        let selected = super::selected_row_metadata(&view_data).map(|(row_id, _)| row_id);
        assert_eq!(selected, Some(7));
    }

    #[test]
    fn service_log_self_row_has_no_vendor_link_target() {
        let mut state = AppState {
            active_tab: TabKind::ServiceLog,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();

        view_data.active_tab_snapshot = Some(TabSnapshot::ServiceLog(vec![
            TestRuntime::sample_service_log(90, 2, None, "self performed"),
        ]));
        view_data.table_state.tab = Some(TabKind::ServiceLog);
        view_data.table_state.selected_row = 0;
        view_data.table_state.selected_col = 3;
        super::clamp_table_cursor(&mut view_data);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(state.active_tab, TabKind::ServiceLog);
        assert_eq!(state.status_line.as_deref(), Some("nothing to follow"));
    }

    #[test]
    fn header_indicators_and_contextual_enter_hints_follow_column_semantics() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        let projection = super::active_projection(&view_data).expect("projection");
        let project_header = header_label_for_column(&projection, &view_data.table_state, 1);
        assert!(
            project_header.contains(super::LINK_ARROW),
            "linked quote project column should display link indicator"
        );
        view_data.table_state.sorts = vec![
            super::SortSpec {
                column: 1,
                direction: SortDirection::Asc,
            },
            super::SortSpec {
                column: 2,
                direction: SortDirection::Desc,
            },
        ];
        let sorted_primary = header_label_for_column(&projection, &view_data.table_state, 1);
        let sorted_secondary = header_label_for_column(&projection, &view_data.table_state, 2);
        assert!(sorted_primary.contains("▲1"));
        assert!(sorted_secondary.contains("▼2"));
        view_data.table_state.sorts.clear();
        assert_eq!(contextual_enter_hint(&view_data), "open");

        view_data.table_state.selected_col = 1;
        assert_eq!(contextual_enter_hint(&view_data), "follow");

        state.active_tab = TabKind::Maintenance;
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");
        view_data.table_state.selected_col = 7;
        assert_eq!(contextual_enter_hint(&view_data), "drill");
    }

    #[test]
    fn chat_overlay_supports_history_toggle_and_submit() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime {
            chat_history: vec!["old prompt".to_owned()],
            ..TestRuntime::default()
        };
        let mut view_data = view_data_for_test();
        let (tx, rx) = internal_channel();

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
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        );
        assert_eq!(view_data.chat.input, "old prompt");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
        );
        assert!(view_data.chat.show_sql);

        for key in ['n', 'e', 'w'] {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        pump_internal(&mut state, &mut view_data, &tx, &rx);
        assert!(
            runtime
                .chat_history
                .iter()
                .any(|entry| entry == "old promptnew")
        );
        assert!(view_data.chat.input.is_empty());
        assert_eq!(
            view_data.chat.transcript.last().map(|message| message.role),
            Some(super::ChatRole::Assistant)
        );
    }

    #[test]
    fn chat_pipeline_submission_captures_prior_conversation_history() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime {
            pipeline_result: Some(ChatPipelineResult {
                answer: "first answer".to_owned(),
                sql: Some("SELECT COUNT(*) FROM projects".to_owned()),
                used_fallback: false,
            }),
            ..TestRuntime::default()
        };
        let mut view_data = view_data_for_test();
        let (tx, rx) = internal_channel();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );

        for ch in "first question".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        pump_internal(&mut state, &mut view_data, &tx, &rx);

        runtime.pipeline_result = Some(ChatPipelineResult {
            answer: "second answer".to_owned(),
            sql: Some("SELECT title FROM projects".to_owned()),
            used_fallback: false,
        });
        for ch in "second question".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        pump_internal(&mut state, &mut view_data, &tx, &rx);

        assert_eq!(
            runtime.last_pipeline_question.as_deref(),
            Some("second question")
        );
        assert_eq!(runtime.last_pipeline_history.len(), 2);
        assert_eq!(runtime.last_pipeline_history[0].role, ChatHistoryRole::User);
        assert_eq!(
            runtime.last_pipeline_history[0].content,
            "first question".to_owned()
        );
        assert_eq!(
            runtime.last_pipeline_history[1],
            ChatHistoryMessage {
                role: ChatHistoryRole::Assistant,
                content: "first answer".to_owned(),
            }
        );
    }

    #[test]
    fn chat_pipeline_fallback_sets_status_message() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime {
            pipeline_result: Some(ChatPipelineResult {
                answer: "fallback reply".to_owned(),
                sql: None,
                used_fallback: true,
            }),
            ..TestRuntime::default()
        };
        let mut view_data = view_data_for_test();
        let (tx, rx) = internal_channel();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );
        for ch in "need a fallback".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        pump_internal(&mut state, &mut view_data, &tx, &rx);

        assert_eq!(
            state.status_line.as_deref(),
            Some("fallback mode: answered from data snapshot")
        );
        assert_eq!(
            view_data
                .chat
                .transcript
                .last()
                .map(|message| message.body.as_str()),
            Some("fallback reply")
        );
    }

    #[test]
    fn chat_pipeline_error_is_actionable_in_status_and_transcript() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime {
            pipeline_error: Some("cannot reach http://localhost:11434/v1".to_owned()),
            ..TestRuntime::default()
        };
        let mut view_data = view_data_for_test();
        let (tx, rx) = internal_channel();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );
        for ch in "broken query".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        pump_internal(&mut state, &mut view_data, &tx, &rx);

        assert!(
            state
                .status_line
                .as_deref()
                .is_some_and(|message| message.contains("chat query failed"))
        );
        assert!(
            view_data
                .chat
                .transcript
                .last()
                .map(|message| message.body.contains("verify [llm] config"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn ctrl_c_cancels_in_flight_chat_and_ignores_late_chunks() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let (tx, rx) = internal_channel();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );

        for ch in "cancel me".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        let in_flight = view_data.chat.in_flight.expect("in-flight request");
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(view_data.chat.in_flight.is_none());
        assert_eq!(state.status_line.as_deref(), Some("chat canceled"));

        tx.send(super::InternalEvent::ChatPipeline(
            super::ChatPipelineEvent::AnswerChunk {
                request_id: in_flight.request_id,
                chunk: "late".to_owned(),
            },
        ))
        .expect("send late chunk");
        pump_internal(&mut state, &mut view_data, &tx, &rx);
        assert!(
            !view_data
                .chat
                .transcript
                .iter()
                .any(|message| message.body.contains("late"))
        );
    }

    #[test]
    fn ctrl_c_marks_partial_chat_output_as_interrupted() {
        let mut state = AppState {
            chat: ChatVisibility::Visible,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        view_data.chat.transcript = vec![
            super::ChatMessage {
                role: super::ChatRole::User,
                body: "interrupted answer".to_owned(),
                sql: None,
            },
            super::ChatMessage {
                role: super::ChatRole::Assistant,
                body: "partial answer".to_owned(),
                sql: None,
            },
        ];
        view_data.chat.in_flight = Some(super::ChatInFlight {
            request_id: 42,
            assistant_index: 1,
            stage: super::ChatPipelineStage::Summary,
        });

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(view_data.chat.in_flight.is_none());
        assert_eq!(state.status_line.as_deref(), Some("chat canceled"));
        let last = view_data
            .chat
            .transcript
            .last()
            .expect("partial response should remain");
        assert_eq!(last.body, "partial answer\n(interrupted)");
    }

    #[test]
    fn ctrl_c_marks_partial_sql_output_as_interrupted() {
        let mut state = AppState {
            chat: ChatVisibility::Visible,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        view_data.chat.transcript = vec![
            super::ChatMessage {
                role: super::ChatRole::User,
                body: "interrupted sql".to_owned(),
                sql: None,
            },
            super::ChatMessage {
                role: super::ChatRole::Assistant,
                body: String::new(),
                sql: Some("SELECT".to_owned()),
            },
        ];
        view_data.chat.in_flight = Some(super::ChatInFlight {
            request_id: 43,
            assistant_index: 1,
            stage: super::ChatPipelineStage::Sql,
        });

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(view_data.chat.in_flight.is_none());
        let last = view_data
            .chat
            .transcript
            .last()
            .expect("partial response should remain");
        assert_eq!(last.body, "(interrupted)");
        assert_eq!(last.sql.as_deref(), Some("SELECT"));
    }

    #[test]
    fn late_sql_events_after_cancellation_are_ignored() {
        let mut state = AppState {
            chat: ChatVisibility::Visible,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let (tx, rx) = internal_channel();
        view_data.chat.transcript = vec![
            super::ChatMessage {
                role: super::ChatRole::User,
                body: "cancel sql".to_owned(),
                sql: None,
            },
            super::ChatMessage {
                role: super::ChatRole::Assistant,
                body: String::new(),
                sql: None,
            },
        ];
        view_data.chat.in_flight = Some(super::ChatInFlight {
            request_id: 44,
            assistant_index: 1,
            stage: super::ChatPipelineStage::Sql,
        });

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(view_data.chat.in_flight.is_none());
        assert_eq!(state.status_line.as_deref(), Some("chat canceled"));
        assert_eq!(view_data.chat.transcript.len(), 1);

        tx.send(super::InternalEvent::ChatPipeline(
            super::ChatPipelineEvent::SqlChunk {
                request_id: 44,
                chunk: "SELECT ".to_owned(),
            },
        ))
        .expect("send late sql chunk");
        tx.send(super::InternalEvent::ChatPipeline(
            super::ChatPipelineEvent::SqlReady {
                request_id: 44,
                sql: "SELECT 1".to_owned(),
            },
        ))
        .expect("send late sql ready");
        pump_internal(&mut state, &mut view_data, &tx, &rx);

        assert!(!view_data.chat.transcript.iter().any(|message| {
            message
                .sql
                .as_deref()
                .is_some_and(|sql| sql.contains("SELECT"))
        }));
    }

    #[test]
    fn chat_mag_mode_toggle_updates_existing_rendered_output() {
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
        view_data.chat.transcript.push(super::ChatMessage {
            role: super::ChatRole::User,
            body: "how much?".to_owned(),
            sql: None,
        });
        view_data.chat.transcript.push(super::ChatMessage {
            role: super::ChatRole::Assistant,
            body: "You spent $5,234.23 on kitchen upgrades.".to_owned(),
            sql: None,
        });

        let normal = super::render_chat_overlay_text(&view_data.chat, view_data.mag_mode);
        assert!(normal.contains("$5,234.23"));
        assert!(!normal.contains("↑4"));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        );
        assert!(view_data.mag_mode);
        let mag = super::render_chat_overlay_text(&view_data.chat, view_data.mag_mode);
        assert!(!mag.contains("$5,234.23"));
        assert!(mag.contains("↑4"));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        );
        assert!(!view_data.mag_mode);
        let normal_again = super::render_chat_overlay_text(&view_data.chat, view_data.mag_mode);
        assert!(normal_again.contains("$5,234.23"));
    }

    #[test]
    fn chat_model_commands_list_and_switch_model() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime {
            available_models: vec!["qwen3".to_owned(), "qwen3:32b".to_owned()],
            active_model: Some("qwen3".to_owned()),
            ..TestRuntime::default()
        };
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

        for ch in "/models".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        let list_reply = view_data
            .chat
            .transcript
            .last()
            .map(|message| message.body.clone())
            .unwrap_or_default();
        assert!(list_reply.contains("* qwen3"));
        assert!(list_reply.contains("- qwen3:32b"));

        for ch in "/model qwen3:32b".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(runtime.active_model.as_deref(), Some("qwen3:32b"));
        let switch_reply = view_data
            .chat
            .transcript
            .last()
            .map(|message| message.body.clone())
            .unwrap_or_default();
        assert!(switch_reply.contains("model set: qwen3:32b"));
    }

    #[test]
    fn chat_model_picker_esc_dismisses_without_closing_overlay() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime {
            available_models: vec!["qwen3".to_owned(), "qwen3:32b".to_owned()],
            active_model: Some("qwen3".to_owned()),
            ..TestRuntime::default()
        };
        let mut view_data = view_data_for_test();
        let tx = internal_tx();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );
        for ch in "/model ".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        assert!(view_data.chat.model_picker.visible);
        assert!(!view_data.chat.model_picker.matches.is_empty());

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(state.chat == ChatVisibility::Visible);
        assert!(!view_data.chat.model_picker.visible);
    }

    #[test]
    fn chat_model_picker_enter_selects_highlighted_model() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime {
            available_models: vec![
                "qwen3".to_owned(),
                "qwen3:32b".to_owned(),
                "llama3:8b".to_owned(),
            ],
            active_model: Some("qwen3".to_owned()),
            ..TestRuntime::default()
        };
        let mut view_data = view_data_for_test();
        let tx = internal_tx();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE),
        );
        for ch in "/model q".chars() {
            handle_key_event(
                &mut state,
                &mut runtime,
                &mut view_data,
                &tx,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        assert!(view_data.chat.model_picker.visible);
        assert_eq!(view_data.chat.model_picker.matches.len(), 2);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(runtime.active_model.as_deref(), Some("qwen3:32b"));
        assert!(!view_data.chat.model_picker.visible);
    }

    #[test]
    fn table_command_mapping_covers_sort_filter_and_column_keys() {
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
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT)),
            Some(TableCommand::ToggleFilterInversion)
        );
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)),
            Some(TableCommand::ToggleSettledProjects)
        );
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(TableCommand::HideCurrentColumn)
        );
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT)),
            Some(TableCommand::ShowAllColumns)
        );
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)),
            Some(TableCommand::OpenColumnFinder)
        );
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(TableCommand::MoveHalfPageDown)
        );
        assert_eq!(
            table_command_for_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
            Some(TableCommand::MoveFullPageDown)
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

        let invert = apply_table_command(&mut view_data, TableCommand::ToggleFilterInversion);
        assert_eq!(invert, TableEvent::Status(TableStatus::FilterInvertedOn));
    }

    #[test]
    fn status_text_hides_primary_hints_while_overlays_are_active() {
        let state = AppState::default();
        let mut view_data = view_data_for_test();

        view_data.dashboard.visible = true;
        let dashboard_status = status_text(&state, &view_data);
        assert!(!dashboard_status.contains("NAV"));
        assert!(!dashboard_status.contains("sort"));
        assert!(!dashboard_status.contains("chat"));

        view_data.dashboard.visible = false;
        view_data.help_visible = true;
        let help_status = status_text(&state, &view_data);
        assert!(!help_status.contains("NAV"));
        assert!(!help_status.contains("sort"));
        assert!(!help_status.contains("chat"));

        view_data.help_visible = false;
        view_data.note_preview.visible = true;
        view_data.note_preview.text = "test note".to_owned();
        let note_status = status_text(&state, &view_data);
        assert!(!note_status.contains("NAV"));
        assert!(!note_status.contains("sort"));
        assert!(!note_status.contains("chat"));

        view_data.note_preview.visible = false;
        view_data.column_finder.visible = true;
        let finder_status = status_text(&state, &view_data);
        assert!(!finder_status.contains("NAV"));
        assert!(!finder_status.contains("sort"));
        assert!(!finder_status.contains("chat"));

        view_data.column_finder.visible = false;
        view_data.date_picker.visible = true;
        let date_status = status_text(&state, &view_data);
        assert!(!date_status.contains("NAV"));
        assert!(!date_status.contains("sort"));
        assert!(!date_status.contains("chat"));
    }

    #[test]
    fn status_text_shows_primary_hints_when_no_overlays_are_active() {
        let state = AppState::default();
        let view_data = view_data_for_test();

        let status = status_text(&state, &view_data);
        assert!(status.contains("NAV"));
        assert!(status.contains("s/S/t"));
        assert!(status.contains("chat"));
    }

    #[test]
    fn visible_column_helpers_skip_hidden_columns() {
        let projection = projection_for_visibility_test();
        let hidden = BTreeSet::from([1_usize, 3_usize]);

        assert_eq!(visible_column_indices(&projection, &hidden), vec![0, 2]);
        assert_eq!(first_visible_column(&projection, &hidden), Some(0));
        assert_eq!(last_visible_column(&projection, &hidden), Some(2));
    }

    #[test]
    fn coerce_visible_column_skips_hidden_and_clamps_edges() {
        let projection = projection_for_visibility_test();
        let hidden = BTreeSet::from([1_usize]);

        assert_eq!(coerce_visible_column(&projection, &hidden, 0), Some(0));
        assert_eq!(coerce_visible_column(&projection, &hidden, 1), Some(2));
        assert_eq!(coerce_visible_column(&projection, &hidden, 9), Some(3));

        let all_hidden = BTreeSet::from([0_usize, 1_usize, 2_usize, 3_usize]);
        assert_eq!(coerce_visible_column(&projection, &all_hidden, 0), None);
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
    fn enter_on_plain_column_shows_press_i_to_edit_guidance() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        view_data.table_state.selected_col = 1;
        assert!(matches!(
            super::selected_cell(&view_data),
            Some((1, super::TableCell::Text(_)))
        ));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );

        assert_eq!(state.mode, AppMode::Nav);
        assert_eq!(state.status_line.as_deref(), Some("press i to edit"));
    }

    #[test]
    fn help_overlay_toggle_round_trip_and_absorbs_mode_keys() {
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
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
        );
        assert!(view_data.help_visible);
        assert_eq!(state.status_line.as_deref(), Some("help open"));

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
        );
        assert!(view_data.help_visible);
        assert_eq!(state.mode, AppMode::Nav);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
        );
        assert!(!view_data.help_visible);
        assert_eq!(state.status_line.as_deref(), Some("help hidden"));
    }

    #[test]
    fn esc_in_nav_mode_clears_status_when_no_detail_is_open() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            status_line: Some("temp status".to_owned()),
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
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert_eq!(state.status_line, None);
    }

    #[test]
    fn nav_mode_d_key_moves_rows_without_dispatching_delete() {
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
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );

        assert_eq!(runtime.lifecycle_count, 0);
        assert_eq!(state.mode, AppMode::Nav);
    }

    #[test]
    fn i_key_is_noop_while_already_in_edit_mode() {
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
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
        );
        assert_eq!(state.mode, AppMode::Edit);
    }

    #[test]
    fn dashboard_overlay_navigation_clamps_and_enter_on_header_is_noop() {
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
        assert_eq!(view_data.dashboard.cursor, 0);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.dashboard.cursor, 0, "k at top should clamp");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(
            view_data.dashboard.visible,
            "enter on section header should be a no-op"
        );
        assert_eq!(state.active_tab, TabKind::Projects);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT),
        );
        assert_eq!(view_data.dashboard.cursor, 1);

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.dashboard.cursor, 1, "j at bottom should clamp");

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
        );
        assert_eq!(view_data.dashboard.cursor, 0);
    }

    #[test]
    fn dashboard_overlay_blocks_table_and_mode_keys() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        let start_tab = state.active_tab;
        let start_col = view_data.table_state.selected_col;
        let start_sorts = view_data.table_state.sorts.len();

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
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );

        assert_eq!(view_data.table_state.selected_col, start_col);
        assert_eq!(view_data.table_state.sorts.len(), start_sorts);
        assert_eq!(state.mode, AppMode::Nav);
        assert_eq!(state.active_tab, start_tab);
        assert!(view_data.dashboard.visible);
    }

    #[test]
    fn dashboard_overlay_tab_switch_keys_close_overlay_and_change_tab() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        let start = state.active_tab;
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
        );
        assert!(!view_data.dashboard.visible);
        assert_ne!(state.active_tab, start);
        assert_eq!(runtime.show_dashboard_pref, Some(false));

        let before_prev = state.active_tab;
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
        );
        assert!(!view_data.dashboard.visible);
        assert_ne!(state.active_tab, before_prev);
        assert_eq!(runtime.show_dashboard_pref, Some(false));
    }

    #[test]
    fn dashboard_nav_entries_order_incidents_before_overdue_and_projects() {
        let snapshot = DashboardSnapshot {
            incidents: vec![DashboardIncident {
                incident_id: micasa_app::IncidentId::new(7),
                title: "Burst pipe".to_owned(),
                severity: IncidentSeverity::Urgent,
                days_open: 3,
            }],
            overdue: vec![DashboardMaintenance {
                maintenance_item_id: micasa_app::MaintenanceItemId::new(11),
                item_name: "HVAC filter".to_owned(),
                days_from_now: -5,
            }],
            active_projects: vec![DashboardProject {
                project_id: micasa_app::ProjectId::new(21),
                title: "Deck".to_owned(),
                status: ProjectStatus::Underway,
            }],
            ..DashboardSnapshot::default()
        };
        let entries = dashboard_nav_entries(&snapshot);
        let labels = entries
            .iter()
            .map(|(_, label)| label.as_str())
            .collect::<Vec<_>>();

        let incidents_idx = labels
            .iter()
            .position(|label| *label == "incidents (1)")
            .expect("incidents section");
        let overdue_idx = labels
            .iter()
            .position(|label| *label == "overdue (1)")
            .expect("overdue section");
        let projects_idx = labels
            .iter()
            .position(|label| *label == "active projects (1)")
            .expect("projects section");
        assert!(incidents_idx < overdue_idx);
        assert!(overdue_idx < projects_idx);
    }

    #[test]
    fn dashboard_nav_entries_format_maintenance_and_warranty_relative_durations() {
        let snapshot = DashboardSnapshot {
            overdue: vec![DashboardMaintenance {
                maintenance_item_id: micasa_app::MaintenanceItemId::new(11),
                item_name: "HVAC filter".to_owned(),
                days_from_now: -5,
            }],
            upcoming: vec![DashboardMaintenance {
                maintenance_item_id: micasa_app::MaintenanceItemId::new(12),
                item_name: "Check sump".to_owned(),
                days_from_now: 10,
            }],
            expiring_warranties: vec![
                DashboardWarranty {
                    appliance_id: micasa_app::ApplianceId::new(31),
                    appliance_name: "Fridge".to_owned(),
                    days_from_now: 7,
                },
                DashboardWarranty {
                    appliance_id: micasa_app::ApplianceId::new(32),
                    appliance_name: "Oven".to_owned(),
                    days_from_now: -3,
                },
            ],
            ..DashboardSnapshot::default()
        };

        let entries = dashboard_nav_entries(&snapshot);
        let labels = entries
            .iter()
            .map(|(_, label)| label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"HVAC filter | 5d overdue"));
        assert!(labels.contains(&"Check sump | due in 10d"));
        assert!(labels.contains(&"Fridge | 7d left"));
        assert!(labels.contains(&"Oven | 3d expired"));

        assert!(entries.iter().any(|(entry, label)| {
            matches!(entry, super::DashboardNavEntry::Overdue(_))
                && label == "HVAC filter | 5d overdue"
        }));
        assert!(entries.iter().any(|(entry, label)| {
            matches!(entry, super::DashboardNavEntry::Upcoming(_))
                && label == "Check sump | due in 10d"
        }));
        assert!(entries.iter().any(|(entry, label)| {
            matches!(entry, super::DashboardNavEntry::ExpiringWarranty(_))
                && label == "Oven | 3d expired"
        }));
    }

    #[test]
    fn dashboard_nav_entries_include_project_status_and_recent_activity_rows() {
        let snapshot = DashboardSnapshot {
            active_projects: vec![DashboardProject {
                project_id: micasa_app::ProjectId::new(21),
                title: "Deck".to_owned(),
                status: ProjectStatus::Underway,
            }],
            recent_activity: vec![DashboardServiceEntry {
                service_log_entry_id: micasa_app::ServiceLogEntryId::new(90),
                maintenance_item_id: micasa_app::MaintenanceItemId::new(11),
                serviced_at: Date::from_calendar_date(2026, Month::January, 9).expect("valid date"),
                cost_cents: Some(9_500),
            }],
            ..DashboardSnapshot::default()
        };

        let entries = dashboard_nav_entries(&snapshot);
        let labels = entries
            .iter()
            .map(|(_, label)| label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"Deck | wip"));
        assert!(labels.contains(&"2026-01-09 | item 11 | $95.00"));
    }

    #[test]
    fn dashboard_nav_entries_empty_snapshot_returns_no_rows() {
        let entries = dashboard_nav_entries(&DashboardSnapshot::default());
        assert!(entries.is_empty());
    }

    #[test]
    fn table_title_includes_sort_pin_filter_and_hidden_flags() {
        let state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        view_data.table_state.sorts = vec![super::SortSpec {
            column: 0,
            direction: SortDirection::Asc,
        }];
        view_data.table_state.pin = Some(super::PinnedCell {
            column: 1,
            value: super::TableCell::Text("abcdefghijklmnop".to_owned()),
        });
        view_data.table_state.filter_active = true;
        view_data.table_state.filter_inverted = true;
        view_data.table_state.hide_settled_projects = true;
        view_data.table_state.hidden_columns.insert(3);

        let projection = super::active_projection(&view_data).expect("projection");
        let title = table_title(&projection, &view_data.table_state);
        assert!(title.contains("projects"));
        assert!(title.contains("sort id:asc#1"));
        assert!(title.contains("pin title=abcdefghijkl…"));
        assert!(title.contains("filter on"));
        assert!(title.contains("invert on"));
        assert!(title.contains("settled hidden"));
        assert!(title.contains("hidden 1"));
    }

    #[test]
    fn table_title_includes_deleted_count_when_projection_has_deleted_rows() {
        let active = TestRuntime::sample_project(1, "Active");
        let mut deleted = TestRuntime::sample_project(2, "Deleted");
        deleted.deleted_at = Some(OffsetDateTime::UNIX_EPOCH);

        let snapshot = TabSnapshot::Projects(vec![active, deleted]);
        let table_state = super::TableUiState {
            tab: Some(TabKind::Projects),
            ..super::TableUiState::default()
        };
        let projection = super::projection_for_snapshot(&snapshot, &table_state);
        let title = table_title(&projection, &table_state);

        assert!(title.contains("projects r:2"));
        assert!(title.contains("del 1"));
    }

    #[test]
    fn active_tab_filter_marker_matches_preview_active_and_inverted_states() {
        let mut table_state = super::TableUiState::default();
        assert_eq!(super::active_tab_filter_marker(&table_state), None);

        table_state.pin = Some(super::PinnedCell {
            column: 1,
            value: super::TableCell::Text("plan".to_owned()),
        });
        assert_eq!(
            super::active_tab_filter_marker(&table_state),
            Some(super::FILTER_MARK_PREVIEW)
        );

        table_state.filter_active = true;
        assert_eq!(
            super::active_tab_filter_marker(&table_state),
            Some(super::FILTER_MARK_ACTIVE)
        );

        table_state.filter_inverted = true;
        assert_eq!(
            super::active_tab_filter_marker(&table_state),
            Some(super::FILTER_MARK_ACTIVE_INVERTED)
        );

        table_state.filter_active = false;
        assert_eq!(
            super::active_tab_filter_marker(&table_state),
            Some(super::FILTER_MARK_PREVIEW_INVERTED)
        );
    }

    #[test]
    fn tab_title_only_shows_filter_marker_on_active_tab() {
        let state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let table_state = super::TableUiState {
            pin: Some(super::PinnedCell {
                column: 1,
                value: super::TableCell::Text("plan".to_owned()),
            }),
            ..super::TableUiState::default()
        };

        let active = super::tab_title(TabKind::Projects, &state, &table_state);
        assert!(active.contains(super::FILTER_MARK_PREVIEW));

        let inactive = super::tab_title(TabKind::Quotes, &state, &table_state);
        assert!(!inactive.contains(super::FILTER_MARK_PREVIEW));
        assert!(inactive.contains(TabKind::Quotes.label()));
    }

    #[test]
    fn header_label_single_sort_uses_arrow_and_link_indicator() {
        let state = AppState {
            active_tab: TabKind::Quotes,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        view_data.table_state.sorts = vec![super::SortSpec {
            column: 1,
            direction: SortDirection::Asc,
        }];
        let projection = super::active_projection(&view_data).expect("projection");
        let asc = header_label_for_column(&projection, &view_data.table_state, 1);
        assert!(asc.contains(super::LINK_ARROW));
        assert!(asc.contains("↑"));

        view_data.table_state.sorts[0].direction = SortDirection::Desc;
        let desc = header_label_for_column(&projection, &view_data.table_state, 1);
        assert!(desc.contains("↓"));
    }

    #[test]
    fn header_label_multi_sort_keeps_short_title_visible() {
        let projection = super::TableProjection {
            title: "quotes",
            columns: vec!["id", "project", "vendor", "total"],
            rows: vec![super::TableRowProjection {
                cells: vec![
                    super::TableCell::Integer(1),
                    super::TableCell::Integer(2),
                    super::TableCell::Integer(7),
                    super::TableCell::Money(Some(11_000)),
                ],
                deleted: false,
                tag: None,
            }],
        };
        let table_state = super::TableUiState {
            tab: Some(TabKind::Quotes),
            sorts: vec![
                super::SortSpec {
                    column: 0,
                    direction: SortDirection::Asc,
                },
                super::SortSpec {
                    column: 1,
                    direction: SortDirection::Desc,
                },
            ],
            ..super::TableUiState::default()
        };

        let label = header_label_for_column(&projection, &table_state, 0);
        assert!(label.starts_with("id"));
        assert!(label.contains("▲1"));
    }

    #[test]
    fn header_label_single_sort_indicator_keeps_leading_space() {
        let projection = super::TableProjection {
            title: "quotes",
            columns: vec!["id", "project"],
            rows: vec![super::TableRowProjection {
                cells: vec![super::TableCell::Integer(1), super::TableCell::Integer(2)],
                deleted: false,
                tag: None,
            }],
        };
        let mut table_state = super::TableUiState {
            tab: Some(TabKind::Quotes),
            sorts: vec![super::SortSpec {
                column: 0,
                direction: SortDirection::Asc,
            }],
            ..super::TableUiState::default()
        };

        let asc = header_label_for_column(&projection, &table_state, 0);
        assert!(asc.ends_with(" ↑"));

        table_state.sorts[0].direction = SortDirection::Desc;
        let desc = header_label_for_column(&projection, &table_state, 0);
        assert!(desc.ends_with(" ↓"));
    }

    #[test]
    fn header_label_multi_sort_preserves_money_suffix() {
        let projection = super::TableProjection {
            title: "quotes",
            columns: vec!["id", "total"],
            rows: vec![super::TableRowProjection {
                cells: vec![
                    super::TableCell::Integer(1),
                    super::TableCell::Money(Some(523_423)),
                ],
                deleted: false,
                tag: None,
            }],
        };
        let table_state = super::TableUiState {
            tab: Some(TabKind::Quotes),
            sorts: vec![
                super::SortSpec {
                    column: 0,
                    direction: SortDirection::Asc,
                },
                super::SortSpec {
                    column: 1,
                    direction: SortDirection::Desc,
                },
            ],
            ..super::TableUiState::default()
        };

        let label = header_label_for_column(&projection, &table_state, 1);
        assert!(label.contains('$'));
        assert!(label.contains("▼2"));
    }

    #[test]
    fn header_label_multi_sort_preserves_drill_indicator() {
        let projection = super::TableProjection {
            title: "projects",
            columns: vec!["id", "title", "status", "budget", "start", "quotes"],
            rows: vec![super::TableRowProjection {
                cells: vec![
                    super::TableCell::Integer(1),
                    super::TableCell::Text("Kitchen".to_owned()),
                    super::TableCell::ProjectStatus(ProjectStatus::Underway),
                    super::TableCell::Money(Some(120_000)),
                    super::TableCell::Date(Some(
                        super::Date::from_calendar_date(2026, super::Month::January, 1)
                            .expect("valid date"),
                    )),
                    super::TableCell::Integer(2),
                ],
                deleted: false,
                tag: Some(super::RowTag::ProjectStatus(ProjectStatus::Underway)),
            }],
        };
        let table_state = super::TableUiState {
            tab: Some(TabKind::Projects),
            sorts: vec![
                super::SortSpec {
                    column: 1,
                    direction: SortDirection::Asc,
                },
                super::SortSpec {
                    column: 5,
                    direction: SortDirection::Desc,
                },
            ],
            ..super::TableUiState::default()
        };

        let label = header_label_for_column(&projection, &table_state, 5);
        assert!(label.contains(super::DRILL_ARROW));
        assert!(label.contains("▼2"));
    }

    #[test]
    fn header_label_link_indicator_requires_positive_link_target() {
        let table_state = super::TableUiState {
            tab: Some(TabKind::Quotes),
            ..super::TableUiState::default()
        };

        let no_links_projection = super::TableProjection {
            title: "quotes",
            columns: vec!["id", "project", "vendor"],
            rows: vec![
                super::TableRowProjection {
                    cells: vec![
                        super::TableCell::Integer(1),
                        super::TableCell::Integer(0),
                        super::TableCell::Integer(8),
                    ],
                    deleted: false,
                    tag: None,
                },
                super::TableRowProjection {
                    cells: vec![
                        super::TableCell::Integer(2),
                        super::TableCell::Integer(0),
                        super::TableCell::Integer(9),
                    ],
                    deleted: false,
                    tag: None,
                },
            ],
        };
        let no_links = header_label_for_column(&no_links_projection, &table_state, 1);
        assert!(!no_links.contains(super::LINK_ARROW));

        let with_link_projection = super::TableProjection {
            title: "quotes",
            columns: vec!["id", "project", "vendor"],
            rows: vec![
                super::TableRowProjection {
                    cells: vec![
                        super::TableCell::Integer(1),
                        super::TableCell::Integer(0),
                        super::TableCell::Integer(8),
                    ],
                    deleted: false,
                    tag: None,
                },
                super::TableRowProjection {
                    cells: vec![
                        super::TableCell::Integer(2),
                        super::TableCell::Integer(5),
                        super::TableCell::Integer(9),
                    ],
                    deleted: false,
                    tag: None,
                },
            ],
        };
        let with_link = header_label_for_column(&with_link_projection, &table_state, 1);
        assert!(with_link.contains(super::LINK_ARROW));

        let empty_projection = super::TableProjection {
            title: "quotes",
            columns: vec!["id", "project", "vendor"],
            rows: vec![],
        };
        let empty = header_label_for_column(&empty_projection, &table_state, 1);
        assert!(!empty.contains(super::LINK_ARROW));
    }

    #[test]
    fn link_target_id_only_accepts_positive_integer_variants() {
        assert_eq!(
            super::link_target_id(&super::TableCell::Integer(5)),
            Some(5)
        );
        assert_eq!(super::link_target_id(&super::TableCell::Integer(0)), None);
        assert_eq!(super::link_target_id(&super::TableCell::Integer(-1)), None);
        assert_eq!(
            super::link_target_id(&super::TableCell::OptionalInteger(Some(7))),
            Some(7)
        );
        assert_eq!(
            super::link_target_id(&super::TableCell::OptionalInteger(Some(0))),
            None
        );
        assert_eq!(
            super::link_target_id(&super::TableCell::OptionalInteger(None)),
            None
        );
        assert_eq!(
            super::link_target_id(&super::TableCell::Text("5".to_owned())),
            None
        );
    }

    #[test]
    fn status_text_width_stays_stable_when_filter_state_changes() {
        let mut state = AppState {
            active_tab: TabKind::Quotes,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let tx = internal_tx();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        let before = status_text(&state, &view_data);
        let before_len = before.len();

        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        );
        handle_key_event(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
        );
        state.status_line = None;
        let after = status_text(&state, &view_data);
        assert_eq!(before_len, after.len());
    }

    #[test]
    fn status_text_uses_nav_badge_not_legacy_normal_label() {
        let state = AppState::default();
        let view_data = view_data_for_test();

        let status = status_text(&state, &view_data);
        assert!(status.contains("NAV"));
        assert!(!status.contains("NORMAL"));
    }

    #[test]
    fn edit_mode_status_omits_undo_redo_and_profile_hints() {
        let state = AppState {
            mode: AppMode::Edit,
            ..AppState::default()
        };
        let view_data = view_data_for_test();

        let status = status_text(&state, &view_data);
        assert!(status.contains("EDIT"));
        assert!(!status.to_ascii_lowercase().contains("undo"));
        assert!(!status.to_ascii_lowercase().contains("redo"));
        assert!(!status.to_ascii_lowercase().contains("profile"));
    }

    #[test]
    fn pin_values_do_not_leak_into_status_hint_bar() {
        let state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut view_data = view_data_for_test();
        view_data.table_state.tab = Some(TabKind::Projects);
        view_data.table_state.pin = Some(super::PinnedCell {
            column: 1,
            value: super::TableCell::Text("scoped pin".to_owned()),
        });

        let status = status_text(&state, &view_data);
        assert!(!status.contains("scoped pin"));
    }

    #[test]
    fn help_overlay_text_excludes_legacy_date_picker_heading() {
        let help = help_overlay_text();
        assert!(!help.contains("Date Picker"));
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
    fn dashboard_and_overlay_text_snapshots_match_expected_content() {
        let state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        let dashboard = render_dashboard_text(&state, &view_data);
        assert_eq!(
            dashboard,
            "mode: nav\n\
             deleted: hidden\n\
             \n\
             projects due: 2\n\
             maintenance due: 1\n\
             incidents open: 3"
        );

        let overlay = render_dashboard_overlay_text(
            &view_data.dashboard.snapshot,
            view_data.dashboard.cursor,
            false,
        );
        assert!(overlay.contains("incidents (1)"));
        assert!(overlay.contains("Leak | urg | 2d"));
    }

    #[test]
    fn chat_overlay_text_snapshot_shows_sql_and_history() {
        let mut view_data = view_data_for_test();
        view_data.chat.show_sql = true;
        view_data.chat.history = vec!["/help".to_owned(), "show projects".to_owned()];
        view_data.chat.input = "/sql".to_owned();
        view_data.chat.transcript.push(super::ChatMessage {
            role: super::ChatRole::User,
            body: "show projects".to_owned(),
            sql: None,
        });
        view_data.chat.transcript.push(super::ChatMessage {
            role: super::ChatRole::Assistant,
            body: "2 active projects".to_owned(),
            sql: Some("SELECT title\nFROM projects".to_owned()),
        });

        let rendered = render_chat_overlay_text(&view_data.chat, false);
        assert!(rendered.contains("sql: on | history: 2"));
        assert!(rendered.contains("you: show projects"));
        assert!(rendered.contains("llm: 2 active projects"));
        assert!(rendered.contains("  sql: SELECT title"));
        assert!(rendered.contains("  sql: FROM projects"));
        assert!(rendered.contains("> /sql"));
    }

    #[test]
    fn help_overlay_text_includes_global_section_and_cancel_shortcut() {
        let help = help_overlay_text();
        assert!(help.contains("global:"));
        assert!(help.contains("ctrl+q quit"));
        assert!(help.contains("ctrl+c cancel llm"));
    }

    #[test]
    fn help_overlay_text_includes_settled_toggle_and_half_page_shortcuts() {
        let help = help_overlay_text();
        assert!(help.contains("s/S sort"));
        assert!(help.contains("t settled"));
        assert!(help.contains("! invert filter"));
        assert!(help.contains("ctrl+d/u"));
        assert!(help.contains("pgup/pgdn"));
    }

    #[test]
    fn help_overlay_text_includes_form_field_navigation_shortcuts() {
        let help = help_overlay_text();
        assert!(help.contains("form: tab/shift+tab field"));
        assert!(help.contains("ctrl+s or enter submit"));
    }

    #[test]
    fn keybinding_script_edit_and_dashboard_flow_matches_docs() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime {
            can_undo: true,
            can_redo: true,
            ..TestRuntime::default()
        };
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");

        let (tx, rx) = internal_channel();
        run_key_script(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            &rx,
            &[
                KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT),
                KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            ],
        );

        assert_eq!(state.mode, AppMode::Nav);
        assert!(state.show_deleted);
        assert_eq!(runtime.undo_count, 1);
        assert_eq!(runtime.redo_count, 1);
        assert_eq!(state.active_tab, TabKind::Incidents);
        assert_eq!(state.status_line.as_deref(), Some("dashboard -> incidents"));
    }

    #[test]
    fn keybinding_script_chat_overlay_flow_matches_docs() {
        let mut state = AppState::default();
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        let (tx, rx) = internal_channel();

        run_key_script(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            &rx,
            &[KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE)],
        );

        run_key_script(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            &rx,
            &[
                KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            ],
        );

        assert_eq!(state.chat, ChatVisibility::Hidden);
        assert!(!view_data.chat.show_sql);
        assert_eq!(state.status_line.as_deref(), Some("chat hidden"));
        assert!(
            view_data
                .chat
                .transcript
                .iter()
                .any(|message| message.role == super::ChatRole::User && message.body == "/sql")
        );
    }

    #[test]
    fn keybinding_script_filter_and_column_flow_matches_docs() {
        let mut state = AppState {
            active_tab: TabKind::Projects,
            ..AppState::default()
        };
        let mut runtime = TestRuntime::default();
        let mut view_data = view_data_for_test();
        refresh_view_data(&state, &mut runtime, &mut view_data).expect("refresh should work");
        let (tx, rx) = internal_channel();

        run_key_script(
            &mut state,
            &mut runtime,
            &mut view_data,
            &tx,
            &rx,
            &[
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT),
                KeyEvent::new(KeyCode::Char('!'), KeyModifiers::SHIFT),
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
                KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT),
            ],
        );

        assert!(view_data.table_state.pin.is_none());
        assert!(!view_data.table_state.filter_active);
        assert!(!view_data.table_state.filter_inverted);
        assert!(view_data.table_state.hidden_columns.is_empty());
        assert!(!view_data.column_finder.visible);
        assert_eq!(state.status_line.as_deref(), Some("all columns shown"));

        let projection = super::active_projection(&view_data).expect("active projection");
        assert_eq!(
            projection.columns[view_data.table_state.selected_col],
            "actual"
        );
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
