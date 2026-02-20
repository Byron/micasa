// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use serde::{Deserialize, Serialize};

macro_rules! entity_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(i64);

        impl $name {
            pub const fn new(value: i64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> i64 {
                self.0
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self(value)
            }
        }
    };
}

entity_id!(HouseProfileId);
entity_id!(ProjectTypeId);
entity_id!(VendorId);
entity_id!(ProjectId);
entity_id!(QuoteId);
entity_id!(MaintenanceCategoryId);
entity_id!(ApplianceId);
entity_id!(MaintenanceItemId);
entity_id!(ServiceLogEntryId);
entity_id!(IncidentId);
entity_id!(DocumentId);
entity_id!(DeletionRecordId);
entity_id!(SettingId);
entity_id!(ChatInputId);
