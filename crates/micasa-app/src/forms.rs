// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Result, bail};
use time::Date;

use crate::{
    ApplianceId, DocumentEntityKind, FormKind, IncidentSeverity, IncidentStatus,
    MaintenanceCategoryId, ProjectStatus, ProjectTypeId, VendorId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFormInput {
    pub title: String,
    pub project_type_id: ProjectTypeId,
    pub status: ProjectStatus,
    pub description: String,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,
    pub budget_cents: Option<i64>,
    pub actual_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorFormInput {
    pub name: String,
    pub contact_name: String,
    pub email: String,
    pub phone: String,
    pub website: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteFormInput {
    pub project_id: crate::ProjectId,
    pub vendor_id: VendorId,
    pub total_cents: i64,
    pub labor_cents: Option<i64>,
    pub materials_cents: Option<i64>,
    pub other_cents: Option<i64>,
    pub received_date: Option<Date>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplianceFormInput {
    pub name: String,
    pub brand: String,
    pub model_number: String,
    pub serial_number: String,
    pub purchase_date: Option<Date>,
    pub warranty_expiry: Option<Date>,
    pub location: String,
    pub cost_cents: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceItemFormInput {
    pub name: String,
    pub category_id: MaintenanceCategoryId,
    pub appliance_id: Option<ApplianceId>,
    pub last_serviced_at: Option<Date>,
    pub interval_months: i32,
    pub manual_url: String,
    pub manual_text: String,
    pub notes: String,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentFormInput {
    pub title: String,
    pub description: String,
    pub status: IncidentStatus,
    pub severity: IncidentSeverity,
    pub date_noticed: Date,
    pub date_resolved: Option<Date>,
    pub location: String,
    pub cost_cents: Option<i64>,
    pub appliance_id: Option<ApplianceId>,
    pub vendor_id: Option<VendorId>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentFormInput {
    pub title: String,
    pub file_name: String,
    pub entity_kind: DocumentEntityKind,
    pub entity_id: i64,
    pub mime_type: String,
    pub data: Vec<u8>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormPayload {
    Project(ProjectFormInput),
    Vendor(VendorFormInput),
    Quote(QuoteFormInput),
    Appliance(ApplianceFormInput),
    Maintenance(MaintenanceItemFormInput),
    Incident(IncidentFormInput),
    Document(DocumentFormInput),
}

impl FormPayload {
    pub fn kind(&self) -> FormKind {
        match self {
            Self::Project(_) => FormKind::Project,
            Self::Vendor(_) => FormKind::Vendor,
            Self::Quote(_) => FormKind::Quote,
            Self::Appliance(_) => FormKind::Appliance,
            Self::Maintenance(_) => FormKind::MaintenanceItem,
            Self::Incident(_) => FormKind::Incident,
            Self::Document(_) => FormKind::Document,
        }
    }

    pub fn blank_for(kind: FormKind) -> Option<Self> {
        match kind {
            FormKind::Project => Some(Self::Project(ProjectFormInput {
                title: String::new(),
                project_type_id: ProjectTypeId::new(0),
                status: ProjectStatus::Planned,
                description: String::new(),
                start_date: None,
                end_date: None,
                budget_cents: None,
                actual_cents: None,
            })),
            FormKind::Vendor => Some(Self::Vendor(VendorFormInput {
                name: String::new(),
                contact_name: String::new(),
                email: String::new(),
                phone: String::new(),
                website: String::new(),
                notes: String::new(),
            })),
            FormKind::Quote => Some(Self::Quote(QuoteFormInput {
                project_id: crate::ProjectId::new(0),
                vendor_id: VendorId::new(0),
                total_cents: 0,
                labor_cents: None,
                materials_cents: None,
                other_cents: None,
                received_date: None,
                notes: String::new(),
            })),
            FormKind::Appliance => Some(Self::Appliance(ApplianceFormInput {
                name: String::new(),
                brand: String::new(),
                model_number: String::new(),
                serial_number: String::new(),
                purchase_date: None,
                warranty_expiry: None,
                location: String::new(),
                cost_cents: None,
                notes: String::new(),
            })),
            FormKind::MaintenanceItem => Some(Self::Maintenance(MaintenanceItemFormInput {
                name: String::new(),
                category_id: MaintenanceCategoryId::new(0),
                appliance_id: None,
                last_serviced_at: None,
                interval_months: 1,
                manual_url: String::new(),
                manual_text: String::new(),
                notes: String::new(),
                cost_cents: None,
            })),
            FormKind::Incident => Some(Self::Incident(IncidentFormInput {
                title: String::new(),
                description: String::new(),
                status: IncidentStatus::Open,
                severity: IncidentSeverity::Soon,
                date_noticed: Date::from_calendar_date(1970, time::Month::January, 1)
                    .expect("valid baseline date"),
                date_resolved: None,
                location: String::new(),
                cost_cents: None,
                appliance_id: None,
                vendor_id: None,
                notes: String::new(),
            })),
            FormKind::Document => Some(Self::Document(DocumentFormInput {
                title: String::new(),
                file_name: String::new(),
                entity_kind: DocumentEntityKind::None,
                entity_id: 0,
                mime_type: String::new(),
                data: Vec::new(),
                notes: String::new(),
            })),
            FormKind::HouseProfile | FormKind::ServiceLogEntry => None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Project(project) => project.validate(),
            Self::Vendor(vendor) => vendor.validate(),
            Self::Quote(quote) => quote.validate(),
            Self::Appliance(appliance) => appliance.validate(),
            Self::Maintenance(maintenance) => maintenance.validate(),
            Self::Incident(incident) => incident.validate(),
            Self::Document(document) => document.validate(),
        }
    }
}

impl ProjectFormInput {
    pub fn validate(&self) -> Result<()> {
        if self.title.trim().is_empty() {
            bail!("project title is required -- enter a title and retry");
        }
        if self.project_type_id.get() <= 0 {
            bail!("project type is required -- choose a project type and retry");
        }
        if let (Some(start_date), Some(end_date)) = (self.start_date, self.end_date)
            && end_date < start_date
        {
            bail!("project end date must be on/after start date");
        }
        if let Some(budget) = self.budget_cents
            && budget < 0
        {
            bail!("project budget cannot be negative");
        }
        if let Some(actual) = self.actual_cents
            && actual < 0
        {
            bail!("project actual cannot be negative");
        }
        Ok(())
    }
}

impl VendorFormInput {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("vendor name is required -- enter a vendor name and retry");
        }
        Ok(())
    }
}

impl QuoteFormInput {
    pub fn validate(&self) -> Result<()> {
        if self.project_id.get() <= 0 {
            bail!("quote project is required -- choose a project and retry");
        }
        if self.vendor_id.get() <= 0 {
            bail!("quote vendor is required -- choose a vendor and retry");
        }
        if self.total_cents <= 0 {
            bail!("quote total must be positive");
        }
        for cents in [self.labor_cents, self.materials_cents, self.other_cents]
            .into_iter()
            .flatten()
        {
            if cents < 0 {
                bail!("quote line-item values cannot be negative");
            }
        }
        Ok(())
    }
}

impl ApplianceFormInput {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("appliance name is required -- enter a name and retry");
        }
        if let Some(cost) = self.cost_cents
            && cost < 0
        {
            bail!("appliance cost cannot be negative");
        }
        Ok(())
    }
}

impl MaintenanceItemFormInput {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("maintenance item name is required -- enter a name and retry");
        }
        if self.category_id.get() <= 0 {
            bail!("maintenance category is required -- choose a category and retry");
        }
        if self.interval_months <= 0 {
            bail!("maintenance interval must be at least 1 month");
        }
        if let Some(cost) = self.cost_cents
            && cost < 0
        {
            bail!("maintenance cost cannot be negative");
        }
        Ok(())
    }
}

impl IncidentFormInput {
    pub fn validate(&self) -> Result<()> {
        if self.title.trim().is_empty() {
            bail!("incident title is required -- enter a title and retry");
        }
        if let Some(cost) = self.cost_cents
            && cost < 0
        {
            bail!("incident cost cannot be negative");
        }
        if let Some(date_resolved) = self.date_resolved
            && date_resolved < self.date_noticed
        {
            bail!("incident resolved date must be on/after date noticed");
        }
        Ok(())
    }
}

impl DocumentFormInput {
    pub fn validate(&self) -> Result<()> {
        if self.title.trim().is_empty() {
            bail!("document title is required -- enter a title and retry");
        }
        if self.file_name.trim().is_empty() {
            bail!("document file name is required -- choose a file and retry");
        }
        if self.mime_type.trim().is_empty() {
            bail!("document MIME type is required");
        }
        if self.entity_kind != DocumentEntityKind::None && self.entity_id <= 0 {
            bail!("document entity id must be positive for linked documents");
        }
        if self.data.is_empty() {
            bail!("document content is empty -- choose a file with content and retry");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplianceFormInput, FormPayload, IncidentFormInput, MaintenanceItemFormInput,
        ProjectFormInput, QuoteFormInput,
    };
    use crate::{
        DocumentEntityKind, FormKind, IncidentSeverity, IncidentStatus, MaintenanceCategoryId,
        ProjectId, ProjectStatus, ProjectTypeId, VendorId,
    };
    use time::{Date, Month};

    #[test]
    fn blank_payload_is_available_for_supported_forms() {
        assert!(FormPayload::blank_for(FormKind::Project).is_some());
        assert!(FormPayload::blank_for(FormKind::Vendor).is_some());
        assert!(FormPayload::blank_for(FormKind::HouseProfile).is_none());
    }

    #[test]
    fn project_validation_rejects_empty_title() {
        let payload = FormPayload::Project(ProjectFormInput {
            title: String::new(),
            project_type_id: ProjectTypeId::new(1),
            status: ProjectStatus::Planned,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: None,
            actual_cents: None,
        });
        assert!(payload.validate().is_err());
    }

    #[test]
    fn quote_validation_rejects_non_positive_total() {
        let payload = FormPayload::Quote(QuoteFormInput {
            project_id: ProjectId::new(1),
            vendor_id: VendorId::new(1),
            total_cents: 0,
            labor_cents: None,
            materials_cents: None,
            other_cents: None,
            received_date: None,
            notes: String::new(),
        });
        assert!(payload.validate().is_err());
    }

    #[test]
    fn maintenance_validation_rejects_non_positive_interval() {
        let payload = FormPayload::Maintenance(MaintenanceItemFormInput {
            name: "Filter".to_owned(),
            category_id: MaintenanceCategoryId::new(1),
            appliance_id: None,
            last_serviced_at: None,
            interval_months: 0,
            manual_url: String::new(),
            manual_text: String::new(),
            notes: String::new(),
            cost_cents: None,
        });
        assert!(payload.validate().is_err());
    }

    #[test]
    fn incident_validation_rejects_bad_date_range() {
        let payload = FormPayload::Incident(IncidentFormInput {
            title: "Leak".to_owned(),
            description: String::new(),
            status: IncidentStatus::Open,
            severity: IncidentSeverity::Soon,
            date_noticed: Date::from_calendar_date(2026, Month::January, 10)
                .expect("valid noticed date"),
            date_resolved: Some(
                Date::from_calendar_date(2026, Month::January, 9).expect("valid resolved date"),
            ),
            location: String::new(),
            cost_cents: None,
            appliance_id: None,
            vendor_id: None,
            notes: String::new(),
        });
        assert!(payload.validate().is_err());
    }

    #[test]
    fn appliance_validation_accepts_valid_payload() {
        let payload = FormPayload::Appliance(ApplianceFormInput {
            name: "Dryer".to_owned(),
            brand: "GE".to_owned(),
            model_number: String::new(),
            serial_number: String::new(),
            purchase_date: None,
            warranty_expiry: None,
            location: "Laundry".to_owned(),
            cost_cents: Some(120_000),
            notes: String::new(),
        });
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn document_validation_requires_data() {
        let payload = FormPayload::Document(super::DocumentFormInput {
            title: "Invoice".to_owned(),
            file_name: "invoice.pdf".to_owned(),
            entity_kind: DocumentEntityKind::Project,
            entity_id: 1,
            mime_type: "application/pdf".to_owned(),
            data: Vec::new(),
            notes: String::new(),
        });
        assert!(payload.validate().is_err());
    }
}
