// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result};
use micasa_app::{IncidentSeverity, IncidentStatus, ProjectStatus};
use std::path::PathBuf;
use time::{Date, Duration, Month, OffsetDateTime, Time};

const PROJECT_TYPES: [&str; 12] = [
    "Appliance",
    "Electrical",
    "Exterior",
    "Flooring",
    "HVAC",
    "Landscaping",
    "Painting",
    "Plumbing",
    "Remodel",
    "Roof",
    "Structural",
    "Windows",
];

const MAINTENANCE_CATEGORIES: [&str; 9] = [
    "Appliance",
    "Electrical",
    "Exterior",
    "HVAC",
    "Interior",
    "Landscaping",
    "Plumbing",
    "Safety",
    "Structural",
];

const VENDOR_TRADES: [&str; 12] = [
    "Plumbing",
    "Electric",
    "Landscaping",
    "Roofing",
    "HVAC",
    "Painting",
    "Handyman",
    "Flooring",
    "Fencing",
    "Pest Control",
    "Window",
    "Concrete",
];

const VENDOR_SUFFIXES: [&str; 6] = ["Services", "Solutions", "Co", "Pros", "Works", "Group"];
const VENDOR_ADJECTIVES: [&str; 12] = [
    "Premier",
    "Central",
    "Reliable",
    "Bright",
    "Quality",
    "Summit",
    "Eagle",
    "Heritage",
    "Greenleaf",
    "Sparks",
    "Hartley",
    "Apex",
];

const FIRST_NAMES: [&str; 16] = [
    "Avery", "Jordan", "Taylor", "Riley", "Morgan", "Casey", "Alex", "Quinn", "Parker", "Drew",
    "Kai", "Elliot", "Robin", "Cameron", "Hayden", "Rowan",
];
const LAST_NAMES: [&str; 18] = [
    "Walker", "Martin", "Hill", "Evans", "Lopez", "Gray", "Ward", "Young", "Diaz", "Reed",
    "Campbell", "Turner", "Flores", "Bennett", "Price", "Morris", "Foster", "Brooks",
];

const CITIES: [&str; 14] = [
    "Austin",
    "Seattle",
    "Denver",
    "Madison",
    "Raleigh",
    "Pittsburgh",
    "Portland",
    "Boise",
    "Phoenix",
    "Nashville",
    "Columbus",
    "Minneapolis",
    "Omaha",
    "Tucson",
];
const STATES: [&str; 14] = [
    "TX", "WA", "CO", "WI", "NC", "PA", "OR", "ID", "AZ", "TN", "OH", "MN", "NE", "UT",
];
const STREET_NAMES: [&str; 18] = [
    "Cedar",
    "Maple",
    "Oak",
    "Pine",
    "Willow",
    "Elm",
    "Birch",
    "Juniper",
    "Sunset",
    "Ridge",
    "Valley",
    "Lakeview",
    "Northview",
    "Hillcrest",
    "Brookside",
    "Meadow",
    "Aspen",
    "Canyon",
];

const FOUNDATION_TYPES: [&str; 6] = [
    "Poured Concrete",
    "Block",
    "Crawlspace",
    "Slab",
    "Pier and Beam",
    "Stone",
];
const WIRING_TYPES: [&str; 4] = ["Copper", "Aluminum", "Knob and Tube", "Romex NM-B"];
const ROOF_TYPES: [&str; 6] = [
    "Asphalt Shingle",
    "Metal Standing Seam",
    "Clay Tile",
    "Slate",
    "Wood Shake",
    "TPO Flat",
];
const EXTERIOR_TYPES: [&str; 6] = [
    "Vinyl Siding",
    "Brick",
    "Stucco",
    "Wood Clapboard",
    "Fiber Cement",
    "Stone Veneer",
];
const HEATING_TYPES: [&str; 5] = [
    "Forced Air Gas",
    "Heat Pump",
    "Radiant Floor",
    "Boiler/Radiator",
    "Electric Baseboard",
];
const COOLING_TYPES: [&str; 5] = [
    "Central AC",
    "Mini-Split",
    "Window Units",
    "Evaporative Cooler",
    "Heat Pump",
];
const WATER_SOURCES: [&str; 4] = ["Municipal", "Private Well", "Community Well", "Cistern"];
const SEWER_TYPES: [&str; 4] = ["Municipal", "Septic", "Aerobic Septic", "Holding Tank"];
const PARKING_TYPES: [&str; 6] = [
    "Attached Garage",
    "Detached Garage",
    "Driveway",
    "Carport",
    "Street",
    "None",
];
const BASEMENT_TYPES: [&str; 5] = ["Finished", "Unfinished", "Partial", "Walkout", "None"];
const INSURANCE_CARRIERS: [&str; 10] = [
    "Northern Mutual",
    "Summit Home",
    "Lakeside Insurance",
    "Homestead Mutual",
    "Pioneer Assurance",
    "Frontier Home",
    "Harbor Coverage",
    "Canyon Insurance",
    "Evergreen Assurance",
    "Metro Mutual",
];

const APPLIANCE_NAMES: [&str; 20] = [
    "Refrigerator",
    "Washer",
    "Dryer",
    "Dishwasher",
    "Water Heater",
    "Tankless Water Heater",
    "Furnace",
    "Central AC",
    "Mini-Split AC",
    "Oven / Range",
    "Microwave",
    "Garage Door Opener",
    "Sump Pump",
    "Water Softener",
    "Garbage Disposal",
    "Dehumidifier",
    "Whole-House Fan",
    "Smoke / CO Detector",
    "Thermostat",
    "Ceiling Fan",
];
const APPLIANCE_BRANDS: [&str; 12] = [
    "Frostline",
    "CleanWave",
    "AquaMax",
    "AirComfort",
    "LiftRight",
    "BrightHome",
    "CoolBreeze",
    "SteadyHeat",
    "QuietFlow",
    "PureAir",
    "IronGuard",
    "ClearView",
];
const APPLIANCE_LOCATIONS: [&str; 12] = [
    "Kitchen",
    "Laundry Room",
    "Basement",
    "Garage",
    "Utility Closet",
    "Bathroom",
    "Master Bedroom",
    "Living Room",
    "Attic",
    "Hallway",
    "Sunroom",
    "Crawlspace",
];

const SERVICE_LOG_NOTES: [&str; 9] = [
    "cleaned and tested",
    "replaced worn part",
    "preventive service completed",
    "inspected and tightened fittings",
    "verified operation under load",
    "flushed and reset controls",
    "lubricated moving parts",
    "deep cleaned and reassembled",
    "professional service visit",
];

const INCIDENT_TITLES: [&str; 11] = [
    "Water leak in utility room",
    "Garage door stuck halfway",
    "HVAC not cooling",
    "Ceiling stain after storm",
    "Dishwasher drain backup",
    "Breaker trips on microwave",
    "Fence panel blown down",
    "Toilet running continuously",
    "Window seal condensation",
    "Sump pump alarm",
    "Dryer vibration noise",
];
const INCIDENT_LOCATIONS: [&str; 10] = [
    "Kitchen",
    "Basement",
    "Garage",
    "Laundry Room",
    "Primary Bath",
    "Guest Bath",
    "Backyard",
    "Attic",
    "Living Room",
    "Hallway",
];

const PROJECT_STATUSES: [ProjectStatus; 7] = [
    ProjectStatus::Ideating,
    ProjectStatus::Planned,
    ProjectStatus::Quoted,
    ProjectStatus::Underway,
    ProjectStatus::Delayed,
    ProjectStatus::Completed,
    ProjectStatus::Abandoned,
];
const INCIDENT_STATUSES: [IncidentStatus; 2] = [IncidentStatus::Open, IncidentStatus::InProgress];
const INCIDENT_SEVERITIES: [IncidentSeverity; 3] = [
    IncidentSeverity::Urgent,
    IncidentSeverity::Soon,
    IncidentSeverity::Whenever,
];

const REFERENCE_YEAR: i32 = 2026;

#[derive(Debug, Clone, PartialEq)]
pub struct HouseProfile {
    pub nickname: String,
    pub address_line_1: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub year_built: i32,
    pub square_feet: i32,
    pub lot_square_feet: i32,
    pub bedrooms: i32,
    pub bathrooms: f64,
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
    pub insurance_renewal: Option<OffsetDateTime>,
    pub property_tax_cents: Option<i64>,
    pub hoa_name: String,
    pub hoa_fee_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vendor {
    pub name: String,
    pub contact_name: String,
    pub phone: String,
    pub email: String,
    pub website: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub title: String,
    pub type_name: String,
    pub status: ProjectStatus,
    pub description: String,
    pub start_date: Option<OffsetDateTime>,
    pub end_date: Option<OffsetDateTime>,
    pub budget_cents: Option<i64>,
    pub actual_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appliance {
    pub name: String,
    pub brand: String,
    pub model_number: String,
    pub serial_number: String,
    pub location: String,
    pub purchase_date: Option<OffsetDateTime>,
    pub warranty_expiry: Option<OffsetDateTime>,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceItem {
    pub name: String,
    pub category_name: String,
    pub interval_months: i32,
    pub notes: String,
    pub last_serviced_at: Option<OffsetDateTime>,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLogEntry {
    pub serviced_at: OffsetDateTime,
    pub cost_cents: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incident {
    pub title: String,
    pub description: String,
    pub status: IncidentStatus,
    pub severity: IncidentSeverity,
    pub date_noticed: OffsetDateTime,
    pub location: String,
    pub cost_cents: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub total_cents: i64,
    pub labor_cents: Option<i64>,
    pub materials_cents: Option<i64>,
    pub received_date: Option<OffsetDateTime>,
    pub notes: String,
}

#[derive(Debug, Clone)]
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
        if state == 0 {
            state = 0xA409_3822_299F_31D0;
        }
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);

        let mut x = self.state;
        x ^= x >> 13;
        x ^= x << 7;
        x ^= x >> 17;
        x
    }

    fn int_n(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        (self.next_u64() % (n as u64)) as usize
    }

    fn bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }
}

#[derive(Debug, Clone)]
pub struct HomeFaker {
    rng: DeterministicRng,
    seed: u64,
}

impl HomeFaker {
    pub fn new(seed: u64) -> Self {
        let normalized = if seed == 0 { 1 } else { seed };
        Self {
            rng: DeterministicRng::new(normalized),
            seed: normalized,
        }
    }

    pub fn int_n(&mut self, n: usize) -> usize {
        self.rng.int_n(n)
    }

    pub fn house_profile(&mut self) -> HouseProfile {
        let street = self.pick(&STREET_NAMES);
        let city = self.pick(&CITIES);
        let state = self.pick(&STATES);
        let year_built = self.int_range_i32(1920, 2024);
        let square_feet = self.int_range_i32(800, 4500);
        let lot_square_feet = self.int_range_i32(square_feet, square_feet.saturating_mul(4));

        let renewal =
            self.random_datetime_between(reference_now(), reference_now() + Duration::days(730));
        let property_tax = self.int_range_i64(100_000, 1_200_000);
        let hoa_fee = self.int_range_i64(5_000, 50_000);

        HouseProfile {
            nickname: format!("{street} house"),
            address_line_1: format!("{} {} St", self.int_range_i32(100, 9999), street),
            city: city.to_owned(),
            state: state.to_owned(),
            postal_code: format!("{:05}", self.int_range_i32(10_000, 99_999)),
            year_built,
            square_feet,
            lot_square_feet,
            bedrooms: self.int_range_i32(1, 6),
            bathrooms: (self.int_range_i32(2, 9) as f64) / 2.0,
            foundation_type: self.pick(&FOUNDATION_TYPES).to_owned(),
            wiring_type: self.pick(&WIRING_TYPES).to_owned(),
            roof_type: self.pick(&ROOF_TYPES).to_owned(),
            exterior_type: self.pick(&EXTERIOR_TYPES).to_owned(),
            heating_type: self.pick(&HEATING_TYPES).to_owned(),
            cooling_type: self.pick(&COOLING_TYPES).to_owned(),
            water_source: self.pick(&WATER_SOURCES).to_owned(),
            sewer_type: self.pick(&SEWER_TYPES).to_owned(),
            parking_type: self.pick(&PARKING_TYPES).to_owned(),
            basement_type: self.pick(&BASEMENT_TYPES).to_owned(),
            insurance_carrier: self.pick(&INSURANCE_CARRIERS).to_owned(),
            insurance_policy: format!(
                "HO-{:02}-{:07}",
                self.int_range_i32(1, 99),
                self.int_range_i32(0, 9_999_999),
            ),
            insurance_renewal: Some(renewal),
            property_tax_cents: Some(property_tax),
            hoa_name: format!("{street} HOA"),
            hoa_fee_cents: Some(hoa_fee),
        }
    }

    pub fn vendor(&mut self) -> Vendor {
        let trade = self.pick(&VENDOR_TRADES);
        self.vendor_for_trade(trade)
    }

    pub fn vendor_for_trade(&mut self, trade: &str) -> Vendor {
        let first = self.pick(&FIRST_NAMES);
        let last = self.pick(&LAST_NAMES);
        let domain = self.pick(&[
            "example-home.com",
            "repairs.local",
            "service-group.net",
            "hometeam.io",
            "craftpros.org",
        ]);
        Vendor {
            name: self.vendor_name_for_trade(trade),
            contact_name: format!("{first} {last}"),
            phone: format!(
                "({:03}) {:03}-{:04}",
                self.int_range_i32(200, 999),
                self.int_range_i32(200, 999),
                self.int_range_i32(0, 9_999),
            ),
            email: format!(
                "{}.{}@{domain}",
                first.to_ascii_lowercase(),
                last.to_ascii_lowercase()
            ),
            website: format!("https://{}", domain.replace('@', "")),
        }
    }

    pub fn project(&mut self, type_name: &str) -> Project {
        let mut fallback = format!("Fix {} issue", type_name.to_ascii_lowercase());
        if fallback.trim().is_empty() {
            fallback = "Fix home issue".to_owned();
        }

        let titles = project_titles(type_name);
        let title = if titles.is_empty() {
            fallback
        } else {
            self.pick(titles).to_owned()
        };

        let status_index =
            (self.seed as usize + self.rng.int_n(PROJECT_STATUSES.len())) % PROJECT_STATUSES.len();
        let status = PROJECT_STATUSES[status_index];
        let mut project = Project {
            title,
            type_name: type_name.to_owned(),
            status,
            description: self.sentence(8, 20),
            start_date: None,
            end_date: None,
            budget_cents: None,
            actual_cents: None,
        };

        if status != ProjectStatus::Ideating && status != ProjectStatus::Abandoned {
            let now = reference_now();
            let start = self.random_datetime_between(now - Duration::days(730), now);
            let budget = self.int_range_i64(5_000, 1_500_000);
            project.start_date = Some(start);
            project.budget_cents = Some(budget);
        }

        if status == ProjectStatus::Completed
            && let Some(start) = project.start_date
        {
            let end = self.random_datetime_between(start, reference_now());
            let budget = project.budget_cents.unwrap_or(100_000);
            let variance_percent = self.int_range_i64(-20, 20);
            let actual = (budget + (budget * variance_percent / 100)).max(0);
            project.end_date = Some(end);
            project.actual_cents = Some(actual);
        }

        project
    }

    pub fn appliance(&mut self) -> Appliance {
        let brand = self.pick(&APPLIANCE_BRANDS).to_owned();
        let prefix = brand_prefix(&brand);
        let purchase_date = self.random_datetime_between(
            reference_now() - Duration::days(3650),
            reference_now() - Duration::days(365),
        );
        let cost_cents = self.int_range_i64(15_000, 800_000);
        let mut appliance = Appliance {
            name: self.pick(&APPLIANCE_NAMES).to_owned(),
            brand: brand.clone(),
            model_number: format!("{prefix}-{:04}", self.int_range_i32(100, 9_999)),
            serial_number: format!(
                "{prefix}-{:02}-{:06}",
                self.int_range_i32(0, 99),
                self.int_range_i32(0, 999_999),
            ),
            location: self.pick(&APPLIANCE_LOCATIONS).to_owned(),
            purchase_date: Some(purchase_date),
            warranty_expiry: None,
            cost_cents: Some(cost_cents),
        };

        if self.int_range_i32(1, 10) <= 6 {
            let years = self.int_range_i32(1, 10);
            appliance.warranty_expiry =
                Some(purchase_date + Duration::days(i64::from(years) * 365));
        }
        appliance
    }

    pub fn maintenance_item(&mut self, category_name: &str) -> MaintenanceItem {
        let options = maintenance_options(category_name);
        if options.is_empty() {
            return MaintenanceItem {
                name: format!("Check {}", category_name.to_ascii_lowercase()),
                category_name: category_name.to_owned(),
                interval_months: 12,
                notes: String::new(),
                last_serviced_at: None,
                cost_cents: None,
            };
        }

        let (name, interval, notes) = options[self.rng.int_n(options.len())];
        let mut item = MaintenanceItem {
            name: name.to_owned(),
            category_name: category_name.to_owned(),
            interval_months: interval,
            notes: notes.to_owned(),
            last_serviced_at: None,
            cost_cents: None,
        };

        if self.int_range_i32(1, 10) <= 7 {
            let window_days = i64::from(interval).max(1) * 60;
            item.last_serviced_at = Some(self.random_datetime_between(
                reference_now() - Duration::days(window_days),
                reference_now(),
            ));
        }

        if self.int_range_i32(1, 10) <= 4 {
            item.cost_cents = Some(self.int_range_i64(500, 50_000));
        }

        item
    }

    pub fn service_log_entry(&mut self) -> ServiceLogEntry {
        let serviced_at =
            self.random_datetime_between(reference_now() - Duration::days(730), reference_now());
        self.service_log_entry_at(serviced_at)
    }

    pub fn service_log_entry_at(&mut self, serviced_at: OffsetDateTime) -> ServiceLogEntry {
        ServiceLogEntry {
            serviced_at,
            cost_cents: Some(self.int_range_i64(1_000, 60_000)),
            notes: self.pick(&SERVICE_LOG_NOTES).to_owned(),
        }
    }

    pub fn quote(&mut self) -> Quote {
        let total = self.int_range_i64(10_000, 2_000_000);
        let labor_percent = self.int_range_i64(40, 70);
        let labor = total * labor_percent / 100;
        let materials = total - labor;
        Quote {
            total_cents: total,
            labor_cents: Some(labor),
            materials_cents: Some(materials),
            received_date: Some(
                self.random_datetime_between(
                    reference_now() - Duration::days(365),
                    reference_now(),
                ),
            ),
            notes: self.sentence(5, 15),
        }
    }

    pub fn incident(&mut self) -> Incident {
        let severity = INCIDENT_SEVERITIES[self.rng.int_n(INCIDENT_SEVERITIES.len())];
        let status = INCIDENT_STATUSES[self.rng.int_n(INCIDENT_STATUSES.len())];
        let mut incident = Incident {
            title: self.pick(&INCIDENT_TITLES).to_owned(),
            description: self.sentence(8, 20),
            status,
            severity,
            date_noticed: self
                .random_datetime_between(reference_now() - Duration::days(365), reference_now()),
            location: self.pick(&INCIDENT_LOCATIONS).to_owned(),
            cost_cents: None,
        };
        if self.int_range_i32(1, 10) <= 5 {
            incident.cost_cents = Some(self.int_range_i64(2_000, 300_000));
        }
        incident
    }

    pub fn date_in_year(&mut self, year: i32) -> OffsetDateTime {
        let start = midnight_utc(year, Month::January, 1);
        let end =
            midnight_utc(year, Month::December, 31) + Duration::days(1) - Duration::seconds(1);
        self.random_datetime_between(start, end)
    }

    fn pick<'a>(&mut self, items: &'a [&'a str]) -> &'a str {
        items[self.rng.int_n(items.len())]
    }

    fn int_range_i32(&mut self, min: i32, max: i32) -> i32 {
        if max <= min {
            return min;
        }
        let span = i64::from(max) - i64::from(min) + 1;
        let offset = (self.rng.next_u64() % (span as u64)) as i64;
        (i64::from(min) + offset) as i32
    }

    fn int_range_i64(&mut self, min: i64, max: i64) -> i64 {
        if max <= min {
            return min;
        }
        let span = max - min + 1;
        min + (self.rng.next_u64() % (span as u64)) as i64
    }

    fn random_datetime_between(
        &mut self,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> OffsetDateTime {
        let start_ts = start.unix_timestamp();
        let end_ts = end.unix_timestamp();
        if end_ts <= start_ts {
            return start;
        }
        let span = (end_ts - start_ts) as u64;
        let offset = self.rng.next_u64() % (span + 1);
        OffsetDateTime::from_unix_timestamp(start_ts + offset as i64).expect("valid unix timestamp")
    }

    fn vendor_name_for_trade(&mut self, trade: &str) -> String {
        if self.rng.bool() {
            format!("{} {}", self.pick(&LAST_NAMES), trade)
        } else {
            format!(
                "{} {} {}",
                self.pick(&VENDOR_ADJECTIVES),
                trade,
                self.pick(&VENDOR_SUFFIXES),
            )
        }
    }

    fn sentence(&mut self, min_words: usize, max_words: usize) -> String {
        const WORDS: [&str; 30] = [
            "inspect",
            "repair",
            "replace",
            "service",
            "preventive",
            "maintenance",
            "schedule",
            "estimate",
            "budget",
            "timeline",
            "kitchen",
            "bathroom",
            "garage",
            "basement",
            "attic",
            "roof",
            "window",
            "door",
            "furnace",
            "plumbing",
            "electrical",
            "exterior",
            "interior",
            "clean",
            "test",
            "verify",
            "safety",
            "upgrade",
            "replaceable",
            "component",
        ];

        let count = self.int_range_i32(min_words as i32, max_words as i32) as usize;
        let mut parts = Vec::with_capacity(count);
        for _ in 0..count {
            parts.push(self.pick(&WORDS).to_owned());
        }
        let mut sentence = parts.join(" ");
        if let Some(first) = sentence.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        sentence.push('.');
        sentence
    }
}

pub fn temp_db_path() -> Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir().context("create temp dir")?;
    let db_path = dir.path().join("micasa.db");
    Ok((dir, db_path))
}

pub fn fixture_datetime() -> &'static str {
    "2026-02-19T12:34:56Z"
}

pub fn project_types() -> &'static [&'static str] {
    &PROJECT_TYPES
}

pub fn maintenance_categories() -> &'static [&'static str] {
    &MAINTENANCE_CATEGORIES
}

pub fn vendor_trades() -> &'static [&'static str] {
    &VENDOR_TRADES
}

pub fn brand_prefix(brand: &str) -> String {
    brand.chars().take(2).collect::<String>().to_uppercase()
}

fn reference_now() -> OffsetDateTime {
    midnight_utc(REFERENCE_YEAR, Month::January, 1)
}

fn midnight_utc(year: i32, month: Month, day: u8) -> OffsetDateTime {
    let date = Date::from_calendar_date(year, month, day).expect("valid calendar date");
    let midnight = Time::from_hms(0, 0, 0).expect("valid midnight");
    date.with_time(midnight).assume_utc()
}

fn project_titles(type_name: &str) -> &'static [&'static str] {
    match type_name {
        "Appliance" => &["Replace garbage disposal", "Install range hood"],
        "Electrical" => &["Upgrade electrical panel", "Install recessed lighting"],
        "Exterior" => &["Paint exterior trim", "Repair fence gate"],
        "Flooring" => &["Refinish hardwood floors", "Replace bathroom tile"],
        "HVAC" => &["Install programmable thermostat", "Seal ductwork joints"],
        "Landscaping" => &["Plant privacy hedge", "Build retaining wall"],
        "Painting" => &["Paint front door", "Paint kitchen cabinets"],
        "Plumbing" => &["Replace water heater", "Fix leaky kitchen faucet"],
        "Remodel" => &["Kitchen countertop upgrade", "Finish basement"],
        "Roof" => &["Install gutter guards", "Replace missing shingles"],
        "Structural" => &["Repair cracked foundation wall", "Fix sagging beam"],
        "Windows" => &["Replace front windows", "Install storm windows"],
        _ => &[],
    }
}

fn maintenance_options(category_name: &str) -> &'static [(&'static str, i32, &'static str)] {
    match category_name {
        "Appliance" => &[
            ("Refrigerator coil cleaning", 6, "vacuum coils"),
            ("Dishwasher filter cleaning", 1, "rinse filter"),
        ],
        "Electrical" => &[
            ("Test GFCI outlets", 6, "test and reset"),
            ("Inspect panel for corrosion", 12, "visual inspection"),
        ],
        "Exterior" => &[
            ("Gutter cleaning", 6, "clear debris"),
            ("Inspect caulking around windows", 12, "re-caulk cracks"),
        ],
        "HVAC" => &[
            ("HVAC filter replacement", 3, "replace filter"),
            ("Furnace annual inspection", 12, "schedule service"),
        ],
        "Interior" => &[
            ("Deep clean carpets", 12, "steam clean"),
            ("Clean bathroom exhaust fans", 6, "vacuum motor"),
        ],
        "Landscaping" => &[
            ("Aerate lawn", 12, "fall schedule"),
            ("Prune trees and shrubs", 12, "late winter"),
        ],
        "Plumbing" => &[
            ("Flush water heater", 12, "drain sediment"),
            ("Sump pump test", 6, "pour test bucket"),
        ],
        "Safety" => &[
            ("Smoke detector batteries", 12, "replace all batteries"),
            ("CO detector test", 6, "press test button"),
        ],
        "Structural" => &[
            ("Inspect foundation cracks", 12, "mark any growth"),
            ("Check attic for leaks", 6, "inspect after storms"),
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{HomeFaker, brand_prefix, maintenance_categories, project_types, vendor_trades};
    use micasa_app::ProjectStatus;
    use std::collections::BTreeSet;

    #[test]
    fn new_deterministic_seed() {
        let mut left = HomeFaker::new(42);
        let mut right = HomeFaker::new(42);

        let left_vendor = left.vendor();
        let right_vendor = right.vendor();
        assert_eq!(left_vendor.name, right_vendor.name);
    }

    #[test]
    fn house_profile() {
        let mut faker = HomeFaker::new(1);
        let house = faker.house_profile();

        assert!(!house.nickname.is_empty());
        assert!(!house.city.is_empty());
        assert!((1920..=2024).contains(&house.year_built));
        assert!((800..=4500).contains(&house.square_feet));
        assert!((1..=6).contains(&house.bedrooms));
        assert!(house.insurance_renewal.is_some());
    }

    #[test]
    fn vendor() {
        let mut faker = HomeFaker::new(2);
        let vendor = faker.vendor();

        assert!(!vendor.name.is_empty());
        assert!(!vendor.contact_name.is_empty());
        assert!(!vendor.phone.is_empty());
        assert!(!vendor.email.is_empty());
    }

    #[test]
    fn vendor_for_trade() {
        let mut faker = HomeFaker::new(3);
        let vendor = faker.vendor_for_trade("Plumbing");
        assert!(!vendor.name.is_empty());
    }

    #[test]
    fn project() {
        let mut faker = HomeFaker::new(4);
        for type_name in project_types() {
            let project = faker.project(type_name);
            assert!(!project.title.is_empty(), "type {type_name}");
            assert_eq!(project.type_name, *type_name);
            assert!(!project.description.is_empty(), "type {type_name}");
        }
    }

    #[test]
    fn project_completed_has_end_date_and_actual() {
        let mut found_completed = false;
        for seed in 0_u64..100_u64 {
            let mut faker = HomeFaker::new(seed);
            let project = faker.project("Plumbing");
            if project.status == ProjectStatus::Completed {
                assert!(project.end_date.is_some());
                assert!(project.actual_cents.is_some());
                found_completed = true;
                break;
            }
        }
        assert!(found_completed);
    }

    #[test]
    fn project_unknown_type() {
        let mut faker = HomeFaker::new(5);
        let project = faker.project("Unknown");
        assert!(!project.title.is_empty());
    }

    #[test]
    fn appliance() {
        let mut faker = HomeFaker::new(6);
        let appliance = faker.appliance();

        assert!(!appliance.name.is_empty());
        assert!(!appliance.brand.is_empty());
        assert!(!appliance.model_number.is_empty());
        assert!(!appliance.serial_number.is_empty());
        assert!(!appliance.location.is_empty());
        assert!(appliance.purchase_date.is_some());
        assert!(appliance.cost_cents.is_some());
    }

    #[test]
    fn brand_prefix_handles_ascii_and_unicode_inputs() {
        let cases = [
            ("Frostline", "FR"),
            ("\u{6771}\u{829d}", "\u{6771}\u{829d}"),
            ("Electrolux\u{00AE}", "EL"),
            ("AquaMax", "AQ"),
        ];
        for (brand, expected) in cases {
            assert_eq!(brand_prefix(brand), expected, "brand {brand}");
        }
    }

    #[test]
    fn maintenance_item() {
        let mut faker = HomeFaker::new(7);
        for category in maintenance_categories() {
            let item = faker.maintenance_item(category);
            assert!(!item.name.is_empty(), "category {category}");
            assert!(item.interval_months > 0, "category {category}");
        }
    }

    #[test]
    fn maintenance_item_unknown_category() {
        let mut faker = HomeFaker::new(8);
        let item = faker.maintenance_item("Unknown");
        assert!(!item.name.is_empty());
        assert_eq!(item.interval_months, 12);
    }

    #[test]
    fn service_log_entry() {
        let mut faker = HomeFaker::new(9);
        let entry = faker.service_log_entry();

        assert_ne!(entry.serviced_at.unix_timestamp(), 0);
        assert!(entry.cost_cents.is_some());
        assert!(!entry.notes.is_empty());
    }

    #[test]
    fn quote() {
        let mut faker = HomeFaker::new(10);
        let quote = faker.quote();

        assert!(quote.total_cents > 0);
        let labor = quote.labor_cents.expect("labor should be generated");
        let materials = quote
            .materials_cents
            .expect("materials should be generated");
        assert_eq!(quote.total_cents, labor + materials);
        assert!(quote.received_date.is_some());
    }

    #[test]
    fn variety_across_seeds() {
        let mut names = BTreeSet::new();
        for seed in 0_u64..20_u64 {
            let mut faker = HomeFaker::new(seed);
            names.insert(faker.vendor().name);
        }
        assert!(names.len() >= 10, "got {}", names.len());
    }

    #[test]
    fn vendor_trades_list_is_non_empty() {
        assert!(!vendor_trades().is_empty());
    }

    #[test]
    fn int_n() {
        let mut faker = HomeFaker::new(42);
        for _ in 0..100 {
            let value = faker.int_n(5);
            assert!(value < 5);
        }
    }
}
