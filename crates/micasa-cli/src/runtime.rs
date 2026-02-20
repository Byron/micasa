// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::Result;
use micasa_app::{FormPayload, TabKind};
use micasa_db::{
    NewAppliance, NewDocument, NewIncident, NewMaintenanceItem, NewProject, NewQuote, NewVendor,
    Store,
};

pub struct DbRuntime<'a> {
    store: &'a Store,
}

impl<'a> DbRuntime<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }
}

impl micasa_tui::AppRuntime for DbRuntime<'_> {
    fn load_dashboard_counts(&mut self) -> Result<micasa_app::DashboardCounts> {
        self.store.dashboard_counts()
    }

    fn load_tab_row_count(&mut self, tab: TabKind, include_deleted: bool) -> Result<Option<usize>> {
        let count = match tab {
            TabKind::Dashboard => None,
            TabKind::House => None,
            TabKind::Projects => Some(self.store.list_projects(include_deleted)?.len()),
            TabKind::Quotes => Some(self.store.list_quotes(include_deleted)?.len()),
            TabKind::Maintenance => Some(self.store.list_maintenance_items(include_deleted)?.len()),
            TabKind::ServiceLog => None,
            TabKind::Incidents => Some(self.store.list_incidents(include_deleted)?.len()),
            TabKind::Appliances => Some(self.store.list_appliances(include_deleted)?.len()),
            TabKind::Vendors => Some(self.store.list_vendors(include_deleted)?.len()),
            TabKind::Documents => Some(self.store.list_documents(include_deleted)?.len()),
        };
        Ok(count)
    }

    fn submit_form(&mut self, payload: &FormPayload) -> Result<()> {
        payload.validate()?;

        match payload {
            FormPayload::Project(form) => {
                self.store.create_project(&NewProject {
                    title: form.title.clone(),
                    project_type_id: form.project_type_id,
                    status: form.status,
                    description: form.description.clone(),
                    start_date: form.start_date,
                    end_date: form.end_date,
                    budget_cents: form.budget_cents,
                    actual_cents: form.actual_cents,
                })?;
            }
            FormPayload::Vendor(form) => {
                self.store.create_vendor(&NewVendor {
                    name: form.name.clone(),
                    contact_name: form.contact_name.clone(),
                    email: form.email.clone(),
                    phone: form.phone.clone(),
                    website: form.website.clone(),
                    notes: form.notes.clone(),
                })?;
            }
            FormPayload::Quote(form) => {
                self.store.create_quote(&NewQuote {
                    project_id: form.project_id,
                    vendor_id: form.vendor_id,
                    total_cents: form.total_cents,
                    labor_cents: form.labor_cents,
                    materials_cents: form.materials_cents,
                    other_cents: form.other_cents,
                    received_date: form.received_date,
                    notes: form.notes.clone(),
                })?;
            }
            FormPayload::Appliance(form) => {
                self.store.create_appliance(&NewAppliance {
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
            }
            FormPayload::Maintenance(form) => {
                self.store.create_maintenance_item(&NewMaintenanceItem {
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
            }
            FormPayload::Incident(form) => {
                self.store.create_incident(&NewIncident {
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
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DbRuntime;
    use anyhow::Result;
    use micasa_app::{FormPayload, ProjectFormInput, ProjectStatus, ProjectTypeId, TabKind};
    use micasa_db::{NewProject, Store};
    use micasa_tui::AppRuntime;

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
    fn row_count_respects_deleted_filter() -> Result<()> {
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
        assert_eq!(
            runtime.load_tab_row_count(TabKind::Projects, false)?,
            Some(0)
        );
        assert_eq!(
            runtime.load_tab_row_count(TabKind::Projects, true)?,
            Some(1)
        );
        Ok(())
    }

    #[test]
    fn unsupported_tabs_report_no_row_count() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::new(&store);
        assert_eq!(runtime.load_tab_row_count(TabKind::House, false)?, None);
        assert_eq!(
            runtime.load_tab_row_count(TabKind::ServiceLog, false)?,
            None
        );
        Ok(())
    }
}
