// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Result, bail};
use micasa_app::{FormPayload, TabKind};
use micasa_db::{
    HouseProfileInput, LifecycleEntityRef, NewAppliance, NewDocument, NewIncident,
    NewMaintenanceItem, NewProject, NewQuote, NewServiceLogEntry, NewVendor, Store,
};
use micasa_tui::{
    DashboardIncident, DashboardMaintenance, DashboardProject, DashboardServiceEntry,
    DashboardSnapshot, DashboardWarranty, LifecycleAction, TabSnapshot,
};
use time::{Date, Duration, Month};

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
}

impl<'a> DbRuntime<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self {
            store,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
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
            TabKind::House | TabKind::Documents | TabKind::Dashboard => {
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
        };
        Ok(snapshot)
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
        ProjectTypeId, ServiceLogEntryFormInput, TabKind,
    };
    use micasa_db::{NewMaintenanceItem, NewProject, Store};
    use micasa_tui::{AppRuntime, LifecycleAction};
    use time::{Date, Month};

    #[test]
    fn submit_form_creates_project_row() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::new(&store);
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

        let mut runtime = DbRuntime::new(&store);
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

        let mut runtime = DbRuntime::new(&store);
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

        let mut runtime = DbRuntime::new(&store);
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

        let mut runtime = DbRuntime::new(&store);
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

        let mut runtime = DbRuntime::new(&store);
        let snapshot = runtime.load_dashboard_snapshot()?;
        assert!(!snapshot.incidents.is_empty());
        assert!(!snapshot.recent_activity.is_empty());
        Ok(())
    }
}
