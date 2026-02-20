// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::ids::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectStatus {
    Ideating,
    Planned,
    Quoted,
    Underway,
    Delayed,
    Completed,
    Abandoned,
}

impl ProjectStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ideating => "ideating",
            Self::Planned => "planned",
            Self::Quoted => "quoted",
            Self::Underway => "underway",
            Self::Delayed => "delayed",
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ideating" => Some(Self::Ideating),
            "planned" => Some(Self::Planned),
            "quoted" => Some(Self::Quoted),
            "underway" => Some(Self::Underway),
            "delayed" => Some(Self::Delayed),
            "completed" => Some(Self::Completed),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentStatus {
    Open,
    InProgress,
    Resolved,
}

impl IncidentStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Resolved => "resolved",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "in_progress" => Some(Self::InProgress),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentSeverity {
    Urgent,
    Soon,
    Whenever,
}

impl IncidentSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Urgent => "urgent",
            Self::Soon => "soon",
            Self::Whenever => "whenever",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "urgent" => Some(Self::Urgent),
            "soon" => Some(Self::Soon),
            "whenever" => Some(Self::Whenever),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeletionEntity {
    Project,
    Quote,
    Maintenance,
    Appliance,
    ServiceLog,
    Vendor,
    Document,
    Incident,
}

impl DeletionEntity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Quote => "quote",
            Self::Maintenance => "maintenance",
            Self::Appliance => "appliance",
            Self::ServiceLog => "service_log",
            Self::Vendor => "vendor",
            Self::Document => "document",
            Self::Incident => "incident",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "project" => Some(Self::Project),
            "quote" => Some(Self::Quote),
            "maintenance" => Some(Self::Maintenance),
            "appliance" => Some(Self::Appliance),
            "service_log" => Some(Self::ServiceLog),
            "vendor" => Some(Self::Vendor),
            "document" => Some(Self::Document),
            "incident" => Some(Self::Incident),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentEntityKind {
    None,
    Project,
    Quote,
    Maintenance,
    Appliance,
    ServiceLog,
    Vendor,
    Incident,
}

impl DocumentEntityKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Project => "project",
            Self::Quote => "quote",
            Self::Maintenance => "maintenance",
            Self::Appliance => "appliance",
            Self::ServiceLog => "service_log",
            Self::Vendor => "vendor",
            Self::Incident => "incident",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "" => Some(Self::None),
            "project" => Some(Self::Project),
            "quote" => Some(Self::Quote),
            "maintenance" => Some(Self::Maintenance),
            "appliance" => Some(Self::Appliance),
            "service_log" => Some(Self::ServiceLog),
            "vendor" => Some(Self::Vendor),
            "incident" => Some(Self::Incident),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabKind {
    Dashboard,
    House,
    Projects,
    Quotes,
    Maintenance,
    ServiceLog,
    Incidents,
    Appliances,
    Vendors,
    Documents,
    Settings,
}

impl TabKind {
    pub const ALL: [Self; 11] = [
        Self::Dashboard,
        Self::House,
        Self::Projects,
        Self::Quotes,
        Self::Maintenance,
        Self::ServiceLog,
        Self::Incidents,
        Self::Appliances,
        Self::Vendors,
        Self::Documents,
        Self::Settings,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::House => "house",
            Self::Projects => "projects",
            Self::Quotes => "quotes",
            Self::Maintenance => "maint",
            Self::ServiceLog => "service",
            Self::Incidents => "incidents",
            Self::Appliances => "appliances",
            Self::Vendors => "vendors",
            Self::Documents => "docs",
            Self::Settings => "settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingKey {
    UiShowDashboard,
    LlmModel,
}

impl SettingKey {
    pub const ALL: [Self; 2] = [Self::UiShowDashboard, Self::LlmModel];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UiShowDashboard => "ui.show_dashboard",
            Self::LlmModel => "llm.model",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ui.show_dashboard" => Some(Self::UiShowDashboard),
            "llm.model" => Some(Self::LlmModel),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::UiShowDashboard => "dashboard startup",
            Self::LlmModel => "llm model",
        }
    }

    pub const fn expected_value_kind(self) -> SettingValueKind {
        match self {
            Self::UiShowDashboard => SettingValueKind::Bool,
            Self::LlmModel => SettingValueKind::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingValueKind {
    Bool,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingValue {
    Bool(bool),
    Text(String),
}

impl SettingValue {
    pub fn parse_for_key(key: SettingKey, raw: &str) -> Option<Self> {
        match key.expected_value_kind() {
            SettingValueKind::Bool => match raw.trim().to_ascii_lowercase().as_str() {
                "1" | "true" | "on" | "yes" => Some(Self::Bool(true)),
                "0" | "false" | "off" | "no" => Some(Self::Bool(false)),
                _ => None,
            },
            SettingValueKind::Text => Some(Self::Text(raw.to_owned())),
        }
    }

    pub fn to_storage(&self, key: SettingKey) -> Option<String> {
        match (key.expected_value_kind(), self) {
            (SettingValueKind::Bool, Self::Bool(value)) => {
                Some(if *value { "true" } else { "false" }.to_owned())
            }
            (SettingValueKind::Text, Self::Text(value)) => Some(value.clone()),
            _ => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Bool(true) => "on".to_owned(),
            Self::Bool(false) => "off".to_owned(),
            Self::Text(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSetting {
    pub key: SettingKey,
    pub value: SettingValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormKind {
    HouseProfile,
    Project,
    Quote,
    MaintenanceItem,
    ServiceLogEntry,
    Incident,
    Appliance,
    Vendor,
    Document,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMode {
    Nav,
    Edit,
    Form(FormKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectSortKey {
    UpdatedAt,
    CreatedAt,
    Title,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterScope {
    CurrentTab,
    Global,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HouseProfile {
    pub id: HouseProfileId,
    pub nickname: String,
    pub address_line_1: String,
    pub address_line_2: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub year_built: Option<i32>,
    pub square_feet: Option<i32>,
    pub lot_square_feet: Option<i32>,
    pub bedrooms: Option<i32>,
    pub bathrooms: Option<f64>,
    pub foundation_type: String,
    pub wiring_type: String,
    pub roof_type: String,
    pub exterior_type: String,
    pub heating_type: String,
    pub cooling_type: String,
    pub water_source: String,
    pub sewer_type: String,
    pub parking_type: String,
    pub basement_type: String,
    pub insurance_carrier: String,
    pub insurance_policy: String,
    pub insurance_renewal: Option<Date>,
    pub property_tax_cents: Option<i64>,
    pub hoa_name: String,
    pub hoa_fee_cents: Option<i64>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectType {
    pub id: ProjectTypeId,
    pub name: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vendor {
    pub id: VendorId,
    pub name: String,
    pub contact_name: String,
    pub email: String,
    pub phone: String,
    pub website: String,
    pub notes: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub title: String,
    pub project_type_id: ProjectTypeId,
    pub status: ProjectStatus,
    pub description: String,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,
    pub budget_cents: Option<i64>,
    pub actual_cents: Option<i64>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub id: QuoteId,
    pub project_id: ProjectId,
    pub vendor_id: VendorId,
    pub total_cents: i64,
    pub labor_cents: Option<i64>,
    pub materials_cents: Option<i64>,
    pub other_cents: Option<i64>,
    pub received_date: Option<Date>,
    pub notes: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceCategory {
    pub id: MaintenanceCategoryId,
    pub name: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appliance {
    pub id: ApplianceId,
    pub name: String,
    pub brand: String,
    pub model_number: String,
    pub serial_number: String,
    pub purchase_date: Option<Date>,
    pub warranty_expiry: Option<Date>,
    pub location: String,
    pub cost_cents: Option<i64>,
    pub notes: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceItem {
    pub id: MaintenanceItemId,
    pub name: String,
    pub category_id: MaintenanceCategoryId,
    pub appliance_id: Option<ApplianceId>,
    pub last_serviced_at: Option<Date>,
    pub interval_months: i32,
    pub manual_url: String,
    pub manual_text: String,
    pub notes: String,
    pub cost_cents: Option<i64>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub id: IncidentId,
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
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLogEntry {
    pub id: ServiceLogEntryId,
    pub maintenance_item_id: MaintenanceItemId,
    pub serviced_at: Date,
    pub vendor_id: Option<VendorId>,
    pub cost_cents: Option<i64>,
    pub notes: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    pub title: String,
    pub file_name: String,
    pub entity_kind: DocumentEntityKind,
    pub entity_id: i64,
    pub mime_type: String,
    pub size_bytes: i64,
    pub checksum_sha256: String,
    pub data: Vec<u8>,
    pub notes: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionRecord {
    pub id: DeletionRecordId,
    pub entity: DeletionEntity,
    pub target_id: i64,
    pub deleted_at: OffsetDateTime,
    pub restored_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DashboardCounts {
    pub projects_due: usize,
    pub maintenance_due: usize,
    pub incidents_open: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatInput {
    pub id: ChatInputId,
    pub input: String,
    pub created_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::{SettingKey, SettingValue};

    #[test]
    fn bool_setting_parse_and_storage_round_trip() {
        let parsed = SettingValue::parse_for_key(SettingKey::UiShowDashboard, "true")
            .expect("parse true bool setting");
        assert_eq!(parsed, SettingValue::Bool(true));
        assert_eq!(
            parsed.to_storage(SettingKey::UiShowDashboard),
            Some("true".to_owned())
        );
    }

    #[test]
    fn text_setting_parse_and_storage_round_trip() {
        let parsed = SettingValue::parse_for_key(SettingKey::LlmModel, "qwen3:32b")
            .expect("parse text setting");
        assert_eq!(parsed, SettingValue::Text("qwen3:32b".to_owned()));
        assert_eq!(
            parsed.to_storage(SettingKey::LlmModel),
            Some("qwen3:32b".to_owned())
        );
    }

    #[test]
    fn mismatched_setting_value_type_rejected() {
        let text = SettingValue::Text("qwen3".to_owned());
        assert!(text.to_storage(SettingKey::UiShowDashboard).is_none());
    }
}
