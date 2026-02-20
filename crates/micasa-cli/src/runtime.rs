// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::Result;
use micasa_app::{FormPayload, TabKind};
use micasa_db::{
    HouseProfileInput, NewAppliance, NewDocument, NewIncident, NewMaintenanceItem, NewProject,
    NewQuote, NewServiceLogEntry, NewVendor, Store,
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
            TabKind::House => Some(usize::from(self.store.get_house_profile()?.is_some())),
            TabKind::Projects => Some(self.store.list_projects(include_deleted)?.len()),
            TabKind::Quotes => Some(self.store.list_quotes(include_deleted)?.len()),
            TabKind::Maintenance => Some(self.store.list_maintenance_items(include_deleted)?.len()),
            TabKind::ServiceLog => {
                Some(self.store.list_service_log_entries(include_deleted)?.len())
            }
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
            }
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
            FormPayload::ServiceLogEntry(form) => {
                self.store.create_service_log_entry(&NewServiceLogEntry {
                    maintenance_item_id: form.maintenance_item_id,
                    serviced_at: form.serviced_at,
                    vendor_id: form.vendor_id,
                    cost_cents: form.cost_cents,
                    notes: form.notes.clone(),
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
    use micasa_app::{
        FormPayload, HouseProfileFormInput, ProjectFormInput, ProjectStatus, ProjectTypeId,
        ServiceLogEntryFormInput, TabKind,
    };
    use micasa_db::{NewMaintenanceItem, NewProject, Store};
    use micasa_tui::AppRuntime;
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
    fn house_row_count_tracks_profile_presence() -> Result<()> {
        let store = Store::open_memory()?;
        store.bootstrap()?;

        let mut runtime = DbRuntime::new(&store);
        assert_eq!(runtime.load_tab_row_count(TabKind::House, false)?, Some(0));

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

        assert_eq!(runtime.load_tab_row_count(TabKind::House, false)?, Some(1));
        Ok(())
    }

    #[test]
    fn service_log_row_count_respects_deleted_filter() -> Result<()> {
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

        assert_eq!(
            runtime.load_tab_row_count(TabKind::ServiceLog, false)?,
            Some(0)
        );
        assert_eq!(
            runtime.load_tab_row_count(TabKind::ServiceLog, true)?,
            Some(1)
        );
        Ok(())
    }
}
