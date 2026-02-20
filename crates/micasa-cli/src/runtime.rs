// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result, bail};
use micasa_app::{FormPayload, TabKind};
use micasa_db::{
    HouseProfileInput, LifecycleEntityRef, NewAppliance, NewDocument, NewIncident,
    NewMaintenanceItem, NewProject, NewQuote, NewServiceLogEntry, NewVendor, Store,
};
use micasa_llm::{
    Client as LlmClient, ColumnInfo, Message as LlmMessage, Role as LlmRole, TableInfo,
    build_fallback_prompt, build_sql_prompt, build_summary_prompt, extract_sql,
    format_results_table,
};
use micasa_tui::{
    ChatHistoryMessage, ChatHistoryRole, ChatPipelineEvent, ChatPipelineResult, DashboardIncident,
    DashboardMaintenance, DashboardProject, DashboardServiceEntry, DashboardSnapshot,
    DashboardWarranty, InternalEvent, LifecycleAction, TabSnapshot,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use time::{Date, Duration, Month, OffsetDateTime};

const MAX_UNDO_STACK: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MutationRecord {
    Created(LifecycleEntityRef),
    SoftDeleted(LifecycleEntityRef),
    Restored(LifecycleEntityRef),
}

impl MutationRecord {
    const fn inverse(self) -> Self {
        match self {
            Self::Created(target) => Self::SoftDeleted(target),
            Self::SoftDeleted(target) => Self::Restored(target),
            Self::Restored(target) => Self::SoftDeleted(target),
        }
    }
}

pub struct DbRuntime<'a> {
    store: &'a Store,
    undo_stack: Vec<MutationRecord>,
    redo_stack: Vec<MutationRecord>,
    llm_client: Option<LlmClient>,
    llm_extra_context: String,
    db_path: Option<PathBuf>,
    chat_cancellations: HashMap<u64, Arc<AtomicBool>>,
}

impl<'a> DbRuntime<'a> {
    pub fn with_llm_client_context_and_db_path(
        store: &'a Store,
        llm_client: Option<LlmClient>,
        llm_extra_context: impl Into<String>,
        db_path: Option<PathBuf>,
    ) -> Self {
        Self {
            store,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            llm_client,
            llm_extra_context: llm_extra_context.into(),
            db_path,
            chat_cancellations: HashMap::new(),
        }
    }

    fn llm_extra_context(&self) -> Option<&str> {
        let trimmed = self.llm_extra_context.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    fn build_table_info(&self) -> Vec<TableInfo> {
        Self::build_table_info_from_store(self.store)
    }

    fn build_table_info_from_store(store: &Store) -> Vec<TableInfo> {
        let table_names = match store.table_names() {
            Ok(names) => names,
            Err(_) => return Vec::new(),
        };

        let mut tables = Vec::new();
        for name in table_names {
            let columns = match store.table_columns(&name) {
                Ok(columns) => columns,
                Err(_) => continue,
            };

            let mut llm_columns = Vec::with_capacity(columns.len());
            for column in columns {
                llm_columns.push(ColumnInfo {
                    name: column.name,
                    column_type: column.column_type,
                    not_null: column.not_null,
                    primary_key: column.primary_key > 0,
                });
            }

            tables.push(TableInfo {
                name,
                columns: llm_columns,
            });
        }
        tables
    }

    fn build_history_messages(history: &[ChatHistoryMessage]) -> Vec<LlmMessage> {
        history
            .iter()
            .map(|message| LlmMessage {
                role: match message.role {
                    ChatHistoryRole::User => LlmRole::User,
                    ChatHistoryRole::Assistant => LlmRole::Assistant,
                },
                content: message.content.clone(),
            })
            .collect()
    }

    fn stream_chat_complete(client: &LlmClient, messages: &[LlmMessage]) -> Result<String> {
        let mut response = String::new();
        let stream = client.chat_stream(messages).context("start LLM stream")?;
        for chunk in stream {
            let chunk = chunk.context("read LLM stream chunk")?;
            response.push_str(&chunk.content);
            if chunk.done {
                break;
            }
        }
        Ok(response)
    }

    fn stream_chat_with_events<F>(
        client: &LlmClient,
        messages: &[LlmMessage],
        cancel: &AtomicBool,
        mut on_chunk: F,
    ) -> Result<String>
    where
        F: FnMut(String) -> bool,
    {
        let mut response = String::new();
        let stream = client.chat_stream(messages).context("start LLM stream")?;
        for chunk in stream {
            if cancel.load(Ordering::Acquire) {
                break;
            }
            let chunk = chunk.context("read LLM stream chunk")?;
            if !chunk.content.is_empty() {
                response.push_str(&chunk.content);
                if !on_chunk(chunk.content) {
                    break;
                }
            }
            if chunk.done {
                break;
            }
        }
        Ok(response)
    }

    fn run_fallback_pipeline(
        &self,
        client: &LlmClient,
        question: &str,
        history: &[ChatHistoryMessage],
        tables: &[TableInfo],
        now: OffsetDateTime,
    ) -> Result<ChatPipelineResult> {
        let data_dump = self.store.data_dump();
        let fallback_prompt = build_fallback_prompt(
            tables,
            if data_dump.is_empty() {
                "(no rows)\n"
            } else {
                &data_dump
            },
            now,
            self.llm_extra_context(),
        );

        let mut messages = Vec::with_capacity(history.len() + 2);
        messages.push(LlmMessage {
            role: LlmRole::System,
            content: fallback_prompt,
        });
        messages.extend(Self::build_history_messages(history));
        messages.push(LlmMessage {
            role: LlmRole::User,
            content: question.to_owned(),
        });

        let answer = Self::stream_chat_complete(client, &messages).context(
            "fallback response failed; verify the LLM server is reachable and selected model exists",
        )?;
        Ok(ChatPipelineResult {
            answer,
            sql: None,
            used_fallback: true,
        })
    }

    fn record_mutation(&mut self, record: MutationRecord) {
        self.undo_stack.push(record);
        if self.undo_stack.len() > MAX_UNDO_STACK {
            let overflow = self.undo_stack.len() - MAX_UNDO_STACK;
            self.undo_stack.drain(0..overflow);
        }
        self.redo_stack.clear();
    }

    fn apply_record(&self, record: MutationRecord) -> Result<()> {
        match record {
            MutationRecord::Created(target) | MutationRecord::Restored(target) => {
                self.store.restore(target)
            }
            MutationRecord::SoftDeleted(target) => self.store.soft_delete(target),
        }
    }

    fn lifecycle_target(tab: TabKind, row_id: i64) -> Result<LifecycleEntityRef> {
        if row_id <= 0 {
            bail!("row id must be positive, got {row_id}");
        }

        let target = match tab {
            TabKind::Projects => LifecycleEntityRef::Project(micasa_app::ProjectId::new(row_id)),
            TabKind::Quotes => LifecycleEntityRef::Quote(micasa_app::QuoteId::new(row_id)),
            TabKind::Maintenance => {
                LifecycleEntityRef::MaintenanceItem(micasa_app::MaintenanceItemId::new(row_id))
            }
            TabKind::ServiceLog => {
                LifecycleEntityRef::ServiceLogEntry(micasa_app::ServiceLogEntryId::new(row_id))
            }
            TabKind::Incidents => LifecycleEntityRef::Incident(micasa_app::IncidentId::new(row_id)),
            TabKind::Appliances => {
                LifecycleEntityRef::Appliance(micasa_app::ApplianceId::new(row_id))
            }
            TabKind::Vendors => LifecycleEntityRef::Vendor(micasa_app::VendorId::new(row_id)),
            TabKind::House | TabKind::Documents | TabKind::Dashboard | TabKind::Settings => {
                bail!(
                    "tab {} does not support delete/restore actions",
                    tab.label()
                );
            }
        };
        Ok(target)
    }

    fn today_utc() -> Result<Date> {
        Ok(time::OffsetDateTime::now_utc().date())
    }

    fn compute_next_due(last_serviced_at: Option<Date>, interval_months: i32) -> Option<Date> {
        let start = last_serviced_at?;
        if interval_months <= 0 {
            return None;
        }
        add_months_clamped(start, interval_months)
    }
}

impl micasa_tui::AppRuntime for DbRuntime<'_> {
    fn load_dashboard_counts(&mut self) -> Result<micasa_app::DashboardCounts> {
        self.store.dashboard_counts()
    }

    fn load_dashboard_snapshot(&mut self) -> Result<DashboardSnapshot> {
        let today = Self::today_utc()?;

        let incidents = self
            .store
            .list_open_incidents()?
            .into_iter()
            .map(|incident| DashboardIncident {
                incident_id: incident.id,
                title: incident.title,
                severity: incident.severity,
                days_open: days_from_to(incident.date_noticed, today).max(0),
            })
            .collect::<Vec<_>>();

        let mut overdue = Vec::new();
        let mut upcoming = Vec::new();
        for item in self.store.list_maintenance_with_schedule()? {
            let Some(next_due) =
                Self::compute_next_due(item.last_serviced_at, item.interval_months)
            else {
                continue;
            };
            let days_from_now = days_from_to(today, next_due);
            let entry = DashboardMaintenance {
                maintenance_item_id: item.id,
                item_name: item.name,
                days_from_now,
            };
            if days_from_now < 0 {
                overdue.push(entry);
            } else if days_from_now <= 30 {
                upcoming.push(entry);
            }
        }
        overdue.sort_by_key(|entry| entry.days_from_now);
        upcoming.sort_by_key(|entry| entry.days_from_now);

        let active_projects = self
            .store
            .list_active_projects()?
            .into_iter()
            .map(|project| DashboardProject {
                project_id: project.id,
                title: project.title,
                status: project.status,
            })
            .collect::<Vec<_>>();

        let expiring_warranties = self
            .store
            .list_expiring_warranties(today, 30, 90)?
            .into_iter()
            .filter_map(|appliance| {
                let warranty_expiry = appliance.warranty_expiry?;
                Some(DashboardWarranty {
                    appliance_id: appliance.id,
                    appliance_name: appliance.name,
                    days_from_now: days_from_to(today, warranty_expiry),
                })
            })
            .collect::<Vec<_>>();

        let recent_activity = self
            .store
            .list_recent_service_logs(5)?
            .into_iter()
            .map(|entry| DashboardServiceEntry {
                service_log_entry_id: entry.id,
                maintenance_item_id: entry.maintenance_item_id,
                serviced_at: entry.serviced_at,
                cost_cents: entry.cost_cents,
            })
            .collect::<Vec<_>>();

        Ok(DashboardSnapshot {
            incidents,
            overdue,
            upcoming,
            active_projects,
            expiring_warranties,
            recent_activity,
        })
    }

    fn load_tab_snapshot(
        &mut self,
        tab: TabKind,
        include_deleted: bool,
    ) -> Result<Option<TabSnapshot>> {
        let snapshot = match tab {
            TabKind::Dashboard => None,
            TabKind::House => Some(TabSnapshot::House(Box::new(
                self.store.get_house_profile()?,
            ))),
            TabKind::Projects => Some(TabSnapshot::Projects(
                self.store.list_projects(include_deleted)?,
            )),
            TabKind::Quotes => Some(TabSnapshot::Quotes(
                self.store.list_quotes(include_deleted)?,
            )),
            TabKind::Maintenance => Some(TabSnapshot::Maintenance(
                self.store.list_maintenance_items(include_deleted)?,
            )),
            TabKind::ServiceLog => Some(TabSnapshot::ServiceLog(
                self.store.list_service_log_entries(include_deleted)?,
            )),
            TabKind::Incidents => Some(TabSnapshot::Incidents(
                self.store.list_incidents(include_deleted)?,
            )),
            TabKind::Appliances => Some(TabSnapshot::Appliances(
                self.store.list_appliances(include_deleted)?,
            )),
            TabKind::Vendors => Some(TabSnapshot::Vendors(
                self.store.list_vendors(include_deleted)?,
            )),
            TabKind::Documents => Some(TabSnapshot::Documents(
                self.store.list_documents(include_deleted)?,
            )),
            TabKind::Settings => Some(TabSnapshot::Settings(self.store.list_settings()?)),
        };
        Ok(snapshot)
    }

    fn load_chat_history(&mut self) -> Result<Vec<String>> {
        Ok(self
            .store
            .load_chat_history()?
            .into_iter()
            .map(|entry| entry.input)
            .collect())
    }

    fn append_chat_input(&mut self, input: &str) -> Result<()> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        self.store.append_chat_input(trimmed)
    }

    fn submit_form(&mut self, payload: &FormPayload) -> Result<()> {
        payload.validate()?;

        let mutation = match payload {
            FormPayload::HouseProfile(form) => {
                self.store.upsert_house_profile(&HouseProfileInput {
                    nickname: form.nickname.clone(),
                    address_line_1: form.address_line_1.clone(),
                    address_line_2: form.address_line_2.clone(),
                    city: form.city.clone(),
                    state: form.state.clone(),
                    postal_code: form.postal_code.clone(),
                    year_built: form.year_built,
                    square_feet: form.square_feet,
                    lot_square_feet: form.lot_square_feet,
                    bedrooms: form.bedrooms,
                    bathrooms: form.bathrooms,
                    foundation_type: form.foundation_type.clone(),
                    wiring_type: form.wiring_type.clone(),
                    roof_type: form.roof_type.clone(),
                    exterior_type: form.exterior_type.clone(),
                    heating_type: form.heating_type.clone(),
                    cooling_type: form.cooling_type.clone(),
                    water_source: form.water_source.clone(),
                    sewer_type: form.sewer_type.clone(),
                    parking_type: form.parking_type.clone(),
                    basement_type: form.basement_type.clone(),
                    insurance_carrier: form.insurance_carrier.clone(),
                    insurance_policy: form.insurance_policy.clone(),
                    insurance_renewal: form.insurance_renewal,
                    property_tax_cents: form.property_tax_cents,
                    hoa_name: form.hoa_name.clone(),
                    hoa_fee_cents: form.hoa_fee_cents,
                })?;
                None
            }
            FormPayload::Project(form) => {
                let id = self.store.create_project(&NewProject {
                    title: form.title.clone(),
                    project_type_id: form.project_type_id,
                    status: form.status,
                    description: form.description.clone(),
                    start_date: form.start_date,
                    end_date: form.end_date,
                    budget_cents: form.budget_cents,
                    actual_cents: form.actual_cents,
                })?;
                Some(MutationRecord::Created(LifecycleEntityRef::Project(id)))
            }
            FormPayload::Vendor(form) => {
                let id = self.store.create_vendor(&NewVendor {
                    name: form.name.clone(),
                    contact_name: form.contact_name.clone(),
                    email: form.email.clone(),
                    phone: form.phone.clone(),
                    website: form.website.clone(),
                    notes: form.notes.clone(),
                })?;
                Some(MutationRecord::Created(LifecycleEntityRef::Vendor(id)))
            }
            FormPayload::Quote(form) => {
                let id = self.store.create_quote(&NewQuote {
                    project_id: form.project_id,
                    vendor_id: form.vendor_id,
                    total_cents: form.total_cents,
                    labor_cents: form.labor_cents,
                    materials_cents: form.materials_cents,
                    other_cents: form.other_cents,
                    received_date: form.received_date,
                    notes: form.notes.clone(),
                })?;
                Some(MutationRecord::Created(LifecycleEntityRef::Quote(id)))
            }
            FormPayload::Appliance(form) => {
                let id = self.store.create_appliance(&NewAppliance {
                    name: form.name.clone(),
                    brand: form.brand.clone(),
                    model_number: form.model_number.clone(),
                    serial_number: form.serial_number.clone(),
                    purchase_date: form.purchase_date,
                    warranty_expiry: form.warranty_expiry,
                    location: form.location.clone(),
                    cost_cents: form.cost_cents,
                    notes: form.notes.clone(),
                })?;
                Some(MutationRecord::Created(LifecycleEntityRef::Appliance(id)))
            }
            FormPayload::Maintenance(form) => {
                let id = self.store.create_maintenance_item(&NewMaintenanceItem {
                    name: form.name.clone(),
                    category_id: form.category_id,
                    appliance_id: form.appliance_id,
                    last_serviced_at: form.last_serviced_at,
                    interval_months: form.interval_months,
                    manual_url: form.manual_url.clone(),
                    manual_text: form.manual_text.clone(),
                    notes: form.notes.clone(),
                    cost_cents: form.cost_cents,
                })?;
                Some(MutationRecord::Created(
                    LifecycleEntityRef::MaintenanceItem(id),
                ))
            }
            FormPayload::ServiceLogEntry(form) => {
                let id = self.store.create_service_log_entry(&NewServiceLogEntry {
                    maintenance_item_id: form.maintenance_item_id,
                    serviced_at: form.serviced_at,
                    vendor_id: form.vendor_id,
                    cost_cents: form.cost_cents,
                    notes: form.notes.clone(),
                })?;
                Some(MutationRecord::Created(
                    LifecycleEntityRef::ServiceLogEntry(id),
                ))
            }
            FormPayload::Incident(form) => {
                let id = self.store.create_incident(&NewIncident {
                    title: form.title.clone(),
                    description: form.description.clone(),
                    status: form.status,
                    severity: form.severity,
                    date_noticed: form.date_noticed,
                    date_resolved: form.date_resolved,
                    location: form.location.clone(),
                    cost_cents: form.cost_cents,
                    appliance_id: form.appliance_id,
                    vendor_id: form.vendor_id,
                    notes: form.notes.clone(),
                })?;
                Some(MutationRecord::Created(LifecycleEntityRef::Incident(id)))
            }
            FormPayload::Document(form) => {
                self.store.insert_document(&NewDocument {
                    title: form.title.clone(),
                    file_name: form.file_name.clone(),
                    entity_kind: form.entity_kind,
                    entity_id: form.entity_id,
                    mime_type: form.mime_type.clone(),
                    data: form.data.clone(),
                    notes: form.notes.clone(),
                })?;
                None
            }
        };

        if let Some(mutation) = mutation {
            self.record_mutation(mutation);
        }

        Ok(())
    }

    fn apply_lifecycle(
        &mut self,
        tab: TabKind,
        row_id: i64,
        action: LifecycleAction,
    ) -> Result<()> {
        let target = Self::lifecycle_target(tab, row_id)?;
        let record = match action {
            LifecycleAction::Delete => {
                self.store.soft_delete(target)?;
                MutationRecord::SoftDeleted(target)
            }
            LifecycleAction::Restore => {
                self.store.restore(target)?;
                MutationRecord::Restored(target)
            }
        };
        self.record_mutation(record);
        Ok(())
    }

    fn undo_last_edit(&mut self) -> Result<bool> {
        let Some(record) = self.undo_stack.pop() else {
            return Ok(false);
        };

        let inverse = record.inverse();
        self.apply_record(inverse)?;
        self.redo_stack.push(record);
        if self.redo_stack.len() > MAX_UNDO_STACK {
            let overflow = self.redo_stack.len() - MAX_UNDO_STACK;
            self.redo_stack.drain(0..overflow);
        }
        Ok(true)
    }

    fn redo_last_edit(&mut self) -> Result<bool> {
        let Some(record) = self.redo_stack.pop() else {
            return Ok(false);
        };

        self.apply_record(record)?;
        self.undo_stack.push(record);
        if self.undo_stack.len() > MAX_UNDO_STACK {
            let overflow = self.undo_stack.len() - MAX_UNDO_STACK;
            self.undo_stack.drain(0..overflow);
        }
        Ok(true)
    }

    fn set_show_dashboard_preference(&mut self, show: bool) -> Result<()> {
        self.store.put_show_dashboard(show)
    }

    fn list_chat_models(&mut self) -> Result<Vec<String>> {
        let Some(client) = self.llm_client.as_ref() else {
            bail!("LLM disabled -- set [llm].enabled = true and restart");
        };
        client
            .list_models()
            .context("list models; ensure Ollama (or another OpenAI-compatible server) is running")
    }

    fn active_chat_model(&mut self) -> Result<Option<String>> {
        let Some(client) = self.llm_client.as_mut() else {
            return Ok(None);
        };

        if let Some(saved) = self.store.get_last_model()? {
            if client.model() != saved {
                client.set_model(&saved);
            }
            return Ok(Some(saved));
        }

        Ok(Some(client.model().to_owned()))
    }

    fn select_chat_model(&mut self, model: &str) -> Result<()> {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            bail!("usage: /model <name>");
        }

        let Some(client) = self.llm_client.as_mut() else {
            bail!("LLM disabled -- set [llm].enabled = true and restart");
        };

        let available = client.list_models().with_context(|| {
            format!(
                "list models from {}; verify LLM server URL and availability",
                client.base_url()
            )
        })?;
        let exists = available
            .iter()
            .any(|entry| entry == trimmed || entry.starts_with(&format!("{trimmed}:")));

        if !exists {
            if client.base_url().contains("11434") {
                let mut scanner = client.pull_model(trimmed).with_context(|| {
                    format!(
                        "model `{trimmed}` is missing and auto-pull failed to start; run `ollama pull {trimmed}`"
                    )
                })?;
                while let Some(chunk) = scanner.next_chunk()? {
                    if let Some(error) = chunk.error
                        && !error.is_empty()
                    {
                        bail!(
                            "model pull failed for `{trimmed}`: {error}; run `ollama pull {trimmed}` and retry"
                        );
                    }
                }
            } else {
                bail!(
                    "model `{trimmed}` not found on server -- run `/models` and choose one that exists"
                );
            }
        }

        client.set_model(trimmed);
        self.store.put_last_model(trimmed)?;
        Ok(())
    }

    fn spawn_chat_pipeline(
        &mut self,
        request_id: u64,
        question: &str,
        history: &[ChatHistoryMessage],
        tx: Sender<InternalEvent>,
    ) -> Result<()> {
        self.chat_cancellations
            .retain(|_, flag| !flag.load(Ordering::Acquire));

        let Some(client) = self.llm_client.clone() else {
            bail!("LLM disabled -- set [llm].enabled = true and restart");
        };
        let Some(db_path) = self.db_path.clone() else {
            bail!(
                "chat worker needs a file-backed database path; restart micasa without in-memory DB"
            );
        };

        let cancel = Arc::new(AtomicBool::new(false));
        self.chat_cancellations.insert(request_id, cancel.clone());
        let worker = ChatWorker {
            request_id,
            client,
            llm_extra_context: self.llm_extra_context.clone(),
            question: question.to_owned(),
            history: history.to_vec(),
            cancel,
            tx,
        };

        thread::spawn(move || worker.run(db_path));
        Ok(())
    }

    fn cancel_chat_pipeline(&mut self, request_id: u64) -> Result<()> {
        if let Some(cancel) = self.chat_cancellations.remove(&request_id) {
            cancel.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn run_chat_pipeline(
        &mut self,
        question: &str,
        history: &[ChatHistoryMessage],
    ) -> Result<ChatPipelineResult> {
        let trimmed_question = question.trim();
        if trimmed_question.is_empty() {
            bail!("question is empty; enter a prompt and retry");
        }

        let Some(client) = self.llm_client.as_ref() else {
            bail!("LLM disabled -- set [llm].enabled = true and restart");
        };

        let now = OffsetDateTime::now_utc();
        let tables = self.build_table_info();
        let column_hints = self.store.column_hints();
        let sql_prompt = build_sql_prompt(
            &tables,
            now,
            if column_hints.is_empty() {
                None
            } else {
                Some(column_hints.as_str())
            },
            self.llm_extra_context(),
        );

        let mut sql_messages = Vec::with_capacity(history.len() + 2);
        sql_messages.push(LlmMessage {
            role: LlmRole::System,
            content: sql_prompt,
        });
        sql_messages.extend(Self::build_history_messages(history));
        sql_messages.push(LlmMessage {
            role: LlmRole::User,
            content: trimmed_question.to_owned(),
        });

        let raw_sql = Self::stream_chat_complete(client, &sql_messages).context(
            "SQL generation failed; verify the selected model is available and LLM server is reachable",
        )?;
        let sql = extract_sql(&raw_sql);
        if sql.is_empty() {
            return self
                .run_fallback_pipeline(client, trimmed_question, history, &tables, now)
                .context("LLM returned empty SQL and fallback query failed");
        }

        let (columns, rows) = match self.store.read_only_query(&sql) {
            Ok(output) => output,
            Err(_) => {
                return self
                    .run_fallback_pipeline(client, trimmed_question, history, &tables, now)
                    .context("generated SQL could not be executed and fallback query failed");
            }
        };

        let results_table = format_results_table(&columns, &rows);
        let summary_prompt = build_summary_prompt(
            trimmed_question,
            &sql,
            &results_table,
            now,
            self.llm_extra_context(),
        );

        let summary_messages = vec![
            LlmMessage {
                role: LlmRole::System,
                content: summary_prompt,
            },
            LlmMessage {
                role: LlmRole::User,
                content: "Summarize these results.".to_owned(),
            },
        ];
        let answer = Self::stream_chat_complete(client, &summary_messages).context(
            "result summarization failed; retry with a smaller question or switch to another model",
        )?;

        Ok(ChatPipelineResult {
            answer,
            sql: Some(sql),
            used_fallback: false,
        })
    }
}

struct ChatWorker {
    request_id: u64,
    client: LlmClient,
    llm_extra_context: String,
    question: String,
    history: Vec<ChatHistoryMessage>,
    cancel: Arc<AtomicBool>,
    tx: Sender<InternalEvent>,
}

impl ChatWorker {
    fn send(&self, event: ChatPipelineEvent) -> bool {
        self.tx.send(InternalEvent::ChatPipeline(event)).is_ok()
    }

    fn extra_context(&self) -> Option<&str> {
        let trimmed = self.llm_extra_context.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    fn is_canceled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    fn run_fallback(&self, store: &Store, tables: &[TableInfo], now: OffsetDateTime) -> Result<()> {
        if self.is_canceled() {
            return Ok(());
        }
        if !self.send(ChatPipelineEvent::FallbackStarted {
            request_id: self.request_id,
        }) {
            return Ok(());
        }

        let data_dump = store.data_dump();
        let fallback_prompt = build_fallback_prompt(
            tables,
            if data_dump.is_empty() {
                "(no rows)\n"
            } else {
                &data_dump
            },
            now,
            self.extra_context(),
        );
        let mut fallback_messages = Vec::with_capacity(self.history.len() + 2);
        fallback_messages.push(LlmMessage {
            role: LlmRole::System,
            content: fallback_prompt,
        });
        fallback_messages.extend(DbRuntime::build_history_messages(&self.history));
        fallback_messages.push(LlmMessage {
            role: LlmRole::User,
            content: self.question.trim().to_owned(),
        });

        let answer = DbRuntime::stream_chat_with_events(
            &self.client,
            &fallback_messages,
            self.cancel.as_ref(),
            |chunk| {
                self.send(ChatPipelineEvent::AnswerChunk {
                    request_id: self.request_id,
                    chunk,
                })
            },
        )
        .context(
            "fallback response failed; verify the LLM server is reachable and selected model exists",
        )?;

        if self.is_canceled() {
            return Ok(());
        }
        let _ = self.send(ChatPipelineEvent::Completed {
            request_id: self.request_id,
            result: ChatPipelineResult {
                answer,
                sql: None,
                used_fallback: true,
            },
        });
        Ok(())
    }

    fn run(self, db_path: PathBuf) {
        let result = (|| -> Result<()> {
            let trimmed_question = self.question.trim();
            if trimmed_question.is_empty() {
                bail!("question is empty; enter a prompt and retry");
            }

            let store = Store::open(&db_path)
                .with_context(|| format!("open database {} for chat worker", db_path.display()))?;

            let now = OffsetDateTime::now_utc();
            let tables = DbRuntime::build_table_info_from_store(&store);
            let column_hints = store.column_hints();
            let sql_prompt = build_sql_prompt(
                &tables,
                now,
                if column_hints.is_empty() {
                    None
                } else {
                    Some(column_hints.as_str())
                },
                self.extra_context(),
            );

            let mut sql_messages = Vec::with_capacity(self.history.len() + 2);
            sql_messages.push(LlmMessage {
                role: LlmRole::System,
                content: sql_prompt,
            });
            sql_messages.extend(DbRuntime::build_history_messages(&self.history));
            sql_messages.push(LlmMessage {
                role: LlmRole::User,
                content: trimmed_question.to_owned(),
            });

            let raw_sql = DbRuntime::stream_chat_with_events(
                &self.client,
                &sql_messages,
                self.cancel.as_ref(),
                |chunk| {
                    self.send(ChatPipelineEvent::SqlChunk {
                        request_id: self.request_id,
                        chunk,
                    })
                },
            )
            .context(
                "SQL generation failed; verify the selected model is available and LLM server is reachable",
            )?;

            if self.is_canceled() {
                return Ok(());
            }
            let sql = extract_sql(&raw_sql);
            if sql.is_empty() {
                return self
                    .run_fallback(&store, &tables, now)
                    .context("LLM returned empty SQL and fallback query failed");
            }

            if !self.send(ChatPipelineEvent::SqlReady {
                request_id: self.request_id,
                sql: sql.clone(),
            }) {
                return Ok(());
            }

            let (columns, rows) = match store.read_only_query(&sql) {
                Ok(output) => output,
                Err(_) => {
                    return self
                        .run_fallback(&store, &tables, now)
                        .context("generated SQL could not be executed and fallback query failed");
                }
            };

            let results_table = format_results_table(&columns, &rows);
            let summary_prompt = build_summary_prompt(
                trimmed_question,
                &sql,
                &results_table,
                now,
                self.extra_context(),
            );
            let summary_messages = vec![
                LlmMessage {
                    role: LlmRole::System,
                    content: summary_prompt,
                },
                LlmMessage {
                    role: LlmRole::User,
                    content: "Summarize these results.".to_owned(),
                },
            ];

            let answer = DbRuntime::stream_chat_with_events(
                &self.client,
                &summary_messages,
                self.cancel.as_ref(),
                |chunk| {
                    self.send(ChatPipelineEvent::AnswerChunk {
                        request_id: self.request_id,
                        chunk,
                    })
                },
            )
            .context(
                "result summarization failed; retry with a smaller question or switch to another model",
            )?;

            if self.is_canceled() {
                return Ok(());
            }
            let _ = self.send(ChatPipelineEvent::Completed {
                request_id: self.request_id,
                result: ChatPipelineResult {
                    answer,
                    sql: Some(sql),
                    used_fallback: false,
                },
            });
            Ok(())
        })();

        let was_canceled = self.is_canceled();
        if let Err(error) = result
            && !was_canceled
        {
            let _ = self.send(ChatPipelineEvent::Failed {
                request_id: self.request_id,
                error: error.to_string(),
            });
        }
        self.cancel.store(true, Ordering::Release);
    }
}

fn add_months_clamped(date: Date, months: i32) -> Option<Date> {
    if months <= 0 {
        return None;
    }

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
    let last = first_next_month - Duration::days(1);
    Some(last.day())
}

fn days_from_to(from: Date, to: Date) -> i64 {
    i64::from(to.to_julian_day() - from.to_julian_day())
}

#[cfg(test)]
mod tests {
    use super::DbRuntime;
    use anyhow::Result;
    use micasa_app::{
        FormPayload, HouseProfileFormInput, IncidentSeverity, ProjectFormInput, ProjectStatus,
        ProjectTypeId, ServiceLogEntryFormInput, SettingKey, SettingValue, TabKind,
    };
    use micasa_db::{NewMaintenanceItem, NewProject, Store};
    use micasa_llm::Role as LlmRole;
    use micasa_tui::{
        AppRuntime, ChatHistoryMessage, ChatHistoryRole, LifecycleAction, TabSnapshot,
    };
    use time::{Date, Month};

    #[test]
    fn submit_form_creates_project_row() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        runtime.submit_form(&FormPayload::Project(ProjectFormInput {
            title: "Deck repair".to_owned(),
            project_type_id: ProjectTypeId::new(1),
            status: ProjectStatus::Planned,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: Some(9_500),
            actual_cents: None,
        }))?;

        let projects = store.list_projects(false)?;
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].title, "Deck repair");
        Ok(())
    }

    #[test]
    fn snapshot_respects_deleted_filter() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let project_type_id = store.list_project_types()?[0].id;
        let project_id = store.create_project(&NewProject {
            title: "Window replacement".to_owned(),
            project_type_id,
            status: ProjectStatus::Planned,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: None,
            actual_cents: None,
        })?;
        store.soft_delete_project(project_id)?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        let visible = runtime
            .load_tab_snapshot(TabKind::Projects, false)?
            .expect("projects snapshot");
        let with_deleted = runtime
            .load_tab_snapshot(TabKind::Projects, true)?
            .expect("projects snapshot");
        assert_eq!(visible.row_count(), 0);
        assert_eq!(with_deleted.row_count(), 1);
        Ok(())
    }

    #[test]
    fn house_snapshot_tracks_profile_presence() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        let before = runtime
            .load_tab_snapshot(TabKind::House, false)?
            .expect("house snapshot");
        assert_eq!(before.row_count(), 0);

        runtime.submit_form(&FormPayload::HouseProfile(Box::new(
            HouseProfileFormInput {
                nickname: "Elm Street".to_owned(),
                address_line_1: "123 Elm".to_owned(),
                address_line_2: String::new(),
                city: "Springfield".to_owned(),
                state: "IL".to_owned(),
                postal_code: "62701".to_owned(),
                year_built: Some(1987),
                square_feet: Some(2400),
                lot_square_feet: None,
                bedrooms: Some(4),
                bathrooms: Some(2.5),
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
        )))?;

        let after = runtime
            .load_tab_snapshot(TabKind::House, false)?
            .expect("house snapshot");
        assert_eq!(after.row_count(), 1);
        Ok(())
    }

    #[test]
    fn service_log_snapshot_respects_deleted_filter() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let category_id = store.list_maintenance_categories()?[0].id;
        let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
            name: "HVAC filter".to_owned(),
            category_id,
            appliance_id: None,
            last_serviced_at: None,
            interval_months: 6,
            manual_url: String::new(),
            manual_text: String::new(),
            notes: String::new(),
            cost_cents: None,
        })?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        runtime.submit_form(&FormPayload::ServiceLogEntry(ServiceLogEntryFormInput {
            maintenance_item_id: maintenance_id,
            serviced_at: Date::from_calendar_date(2026, Month::January, 9)?,
            vendor_id: None,
            cost_cents: Some(12_500),
            notes: "Winter check".to_owned(),
        }))?;

        let entry_id = store.list_service_log_entries(false)?[0].id;
        store.soft_delete_service_log_entry(entry_id)?;

        let visible = runtime
            .load_tab_snapshot(TabKind::ServiceLog, false)?
            .expect("service log snapshot");
        let with_deleted = runtime
            .load_tab_snapshot(TabKind::ServiceLog, true)?
            .expect("service log snapshot");
        assert_eq!(visible.row_count(), 0);
        assert_eq!(with_deleted.row_count(), 1);
        Ok(())
    }

    #[test]
    fn lifecycle_and_undo_redo_round_trip() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        runtime.submit_form(&FormPayload::Project(ProjectFormInput {
            title: "Undo demo".to_owned(),
            project_type_id: ProjectTypeId::new(1),
            status: ProjectStatus::Underway,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: Some(5_000),
            actual_cents: None,
        }))?;

        let created_id = store.list_projects(false)?[0].id;
        assert!(runtime.undo_last_edit()?);
        assert!(store.list_projects(false)?.is_empty());

        assert!(runtime.redo_last_edit()?);
        assert_eq!(store.list_projects(false)?.len(), 1);

        runtime.apply_lifecycle(TabKind::Projects, created_id.get(), LifecycleAction::Delete)?;
        assert!(store.list_projects(false)?.is_empty());
        runtime.undo_last_edit()?;
        assert_eq!(store.list_projects(false)?.len(), 1);

        Ok(())
    }

    #[test]
    fn chat_history_round_trip_persists_and_dedupes_adjacent_inputs() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        runtime.append_chat_input("When is the next HVAC service due?")?;
        runtime.append_chat_input("When is the next HVAC service due?")?;
        runtime.append_chat_input("How many active projects do I have?")?;

        let history = runtime.load_chat_history()?;
        assert_eq!(
            history,
            vec![
                "When is the next HVAC service due?".to_owned(),
                "How many active projects do I have?".to_owned(),
            ]
        );
        Ok(())
    }

    #[test]
    fn dashboard_snapshot_includes_open_incident_and_recent_service() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let category_id = store.list_maintenance_categories()?[0].id;
        let maintenance_id = store.create_maintenance_item(&NewMaintenanceItem {
            name: "Water heater flush".to_owned(),
            category_id,
            appliance_id: None,
            last_serviced_at: Some(Date::from_calendar_date(2025, Month::January, 1)?),
            interval_months: 12,
            manual_url: String::new(),
            manual_text: String::new(),
            notes: String::new(),
            cost_cents: None,
        })?;

        store.create_service_log_entry(&micasa_db::NewServiceLogEntry {
            maintenance_item_id: maintenance_id,
            serviced_at: Date::from_calendar_date(2026, Month::January, 5)?,
            vendor_id: None,
            cost_cents: Some(9500),
            notes: String::new(),
        })?;

        store.create_incident(&micasa_db::NewIncident {
            title: "Basement leak".to_owned(),
            description: String::new(),
            status: micasa_app::IncidentStatus::Open,
            severity: IncidentSeverity::Urgent,
            date_noticed: Date::from_calendar_date(2026, Month::January, 10)?,
            date_resolved: None,
            location: String::new(),
            cost_cents: None,
            appliance_id: None,
            vendor_id: None,
            notes: String::new(),
        })?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        let snapshot = runtime.load_dashboard_snapshot()?;
        assert!(!snapshot.incidents.is_empty());
        assert!(!snapshot.recent_activity.is_empty());
        Ok(())
    }

    #[test]
    fn chat_model_commands_fail_actionably_when_llm_disabled() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        let list_error = runtime
            .list_chat_models()
            .expect_err("list models should fail without LLM client");
        assert!(list_error.to_string().contains("LLM disabled"));

        let model_error = runtime
            .select_chat_model("qwen3")
            .expect_err("select model should fail without LLM client");
        assert!(model_error.to_string().contains("LLM disabled"));
        Ok(())
    }

    #[test]
    fn chat_pipeline_fails_actionably_when_llm_disabled() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        let error = runtime
            .run_chat_pipeline("How many projects are underway?", &[])
            .expect_err("pipeline should fail without an llm client");
        assert!(error.to_string().contains("LLM disabled"));
        Ok(())
    }

    #[test]
    fn chat_history_mapping_uses_typed_roles() {
        let mapped = DbRuntime::build_history_messages(&[
            ChatHistoryMessage {
                role: ChatHistoryRole::User,
                content: "Question".to_owned(),
            },
            ChatHistoryMessage {
                role: ChatHistoryRole::Assistant,
                content: "Answer".to_owned(),
            },
        ]);

        assert_eq!(mapped.len(), 2);
        assert_eq!(mapped[0].role, LlmRole::User);
        assert_eq!(mapped[1].role, LlmRole::Assistant);
    }

    #[test]
    fn dashboard_preference_round_trip_uses_settings_table() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        runtime.set_show_dashboard_preference(false)?;
        assert!(!store.get_show_dashboard()?);

        runtime.set_show_dashboard_preference(true)?;
        assert!(store.get_show_dashboard()?);
        Ok(())
    }

    #[test]
    fn settings_snapshot_returns_typed_setting_rows() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::with_llm_client_context_and_db_path(&store, None, "", None);
        runtime.set_show_dashboard_preference(false)?;
        store.put_last_model("qwen3:32b")?;

        let snapshot = runtime
            .load_tab_snapshot(TabKind::Settings, false)?
            .expect("settings snapshot");
        match snapshot {
            TabSnapshot::Settings(rows) => {
                assert!(rows.iter().any(|setting| {
                    setting.key == SettingKey::UiShowDashboard
                        && setting.value == SettingValue::Bool(false)
                }));
                assert!(rows.iter().any(|setting| {
                    setting.key == SettingKey::LlmModel
                        && setting.value == SettingValue::Text("qwen3:32b".to_owned())
                }));
            }
            _ => panic!("expected settings snapshot"),
        }

        Ok(())
    }
}
