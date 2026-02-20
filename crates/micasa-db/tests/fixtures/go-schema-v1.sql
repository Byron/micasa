CREATE TABLE house_profiles (
  id INTEGER PRIMARY KEY,
  nickname TEXT NOT NULL DEFAULT '',
  address_line_1 TEXT NOT NULL DEFAULT '',
  address_line_2 TEXT NOT NULL DEFAULT '',
  city TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL DEFAULT '',
  postal_code TEXT NOT NULL DEFAULT '',
  year_built INTEGER,
  square_feet INTEGER,
  lot_square_feet INTEGER,
  bedrooms INTEGER,
  bathrooms REAL,
  foundation_type TEXT NOT NULL DEFAULT '',
  wiring_type TEXT NOT NULL DEFAULT '',
  roof_type TEXT NOT NULL DEFAULT '',
  exterior_type TEXT NOT NULL DEFAULT '',
  heating_type TEXT NOT NULL DEFAULT '',
  cooling_type TEXT NOT NULL DEFAULT '',
  water_source TEXT NOT NULL DEFAULT '',
  sewer_type TEXT NOT NULL DEFAULT '',
  parking_type TEXT NOT NULL DEFAULT '',
  basement_type TEXT NOT NULL DEFAULT '',
  insurance_carrier TEXT NOT NULL DEFAULT '',
  insurance_policy TEXT NOT NULL DEFAULT '',
  insurance_renewal TEXT,
  property_tax_cents INTEGER,
  hoa_name TEXT NOT NULL DEFAULT '',
  hoa_fee_cents INTEGER,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE project_types (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX idx_project_types_name ON project_types (name);
CREATE TABLE vendors (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  contact_name TEXT NOT NULL DEFAULT '',
  email TEXT NOT NULL DEFAULT '',
  phone TEXT NOT NULL DEFAULT '',
  website TEXT NOT NULL DEFAULT '',
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  deleted_at TEXT
);
CREATE UNIQUE INDEX idx_vendors_name ON vendors (name);
CREATE INDEX idx_vendors_deleted_at ON vendors (deleted_at);
CREATE TABLE projects (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  project_type_id INTEGER NOT NULL,
  status TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  start_date TEXT,
  end_date TEXT,
  budget_cents INTEGER,
  actual_cents INTEGER,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  deleted_at TEXT,
  FOREIGN KEY (project_type_id) REFERENCES project_types(id) ON DELETE RESTRICT
);
CREATE INDEX idx_projects_project_type_id ON projects (project_type_id);
CREATE INDEX idx_projects_deleted_at ON projects (deleted_at);
CREATE TABLE quotes (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL,
  vendor_id INTEGER NOT NULL,
  total_cents INTEGER NOT NULL,
  labor_cents INTEGER,
  materials_cents INTEGER,
  other_cents INTEGER,
  received_date TEXT,
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  deleted_at TEXT,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE RESTRICT,
  FOREIGN KEY (vendor_id) REFERENCES vendors(id) ON DELETE RESTRICT
);
CREATE INDEX idx_quotes_project_id ON quotes (project_id);
CREATE INDEX idx_quotes_vendor_id ON quotes (vendor_id);
CREATE INDEX idx_quotes_deleted_at ON quotes (deleted_at);
CREATE TABLE maintenance_categories (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX idx_maintenance_categories_name ON maintenance_categories (name);
CREATE TABLE appliances (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  brand TEXT NOT NULL DEFAULT '',
  model_number TEXT NOT NULL DEFAULT '',
  serial_number TEXT NOT NULL DEFAULT '',
  purchase_date TEXT,
  warranty_expiry TEXT,
  location TEXT NOT NULL DEFAULT '',
  cost_cents INTEGER,
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  deleted_at TEXT
);
CREATE INDEX idx_appliances_warranty_expiry ON appliances (warranty_expiry);
CREATE INDEX idx_appliances_deleted_at ON appliances (deleted_at);
CREATE TABLE maintenance_items (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  category_id INTEGER NOT NULL,
  appliance_id INTEGER,
  last_serviced_at TEXT,
  interval_months INTEGER NOT NULL,
  manual_url TEXT NOT NULL DEFAULT '',
  manual_text TEXT NOT NULL DEFAULT '',
  notes TEXT NOT NULL DEFAULT '',
  cost_cents INTEGER,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  deleted_at TEXT,
  FOREIGN KEY (category_id) REFERENCES maintenance_categories(id) ON DELETE RESTRICT,
  FOREIGN KEY (appliance_id) REFERENCES appliances(id) ON DELETE SET NULL
);
CREATE INDEX idx_maintenance_items_category_id ON maintenance_items (category_id);
CREATE INDEX idx_maintenance_items_appliance_id ON maintenance_items (appliance_id);
CREATE INDEX idx_maintenance_items_deleted_at ON maintenance_items (deleted_at);
CREATE TABLE incidents (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL,
  severity TEXT NOT NULL,
  date_noticed TEXT NOT NULL,
  date_resolved TEXT,
  location TEXT NOT NULL DEFAULT '',
  cost_cents INTEGER,
  appliance_id INTEGER,
  vendor_id INTEGER,
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  deleted_at TEXT,
  FOREIGN KEY (appliance_id) REFERENCES appliances(id) ON DELETE SET NULL,
  FOREIGN KEY (vendor_id) REFERENCES vendors(id) ON DELETE SET NULL
);
CREATE INDEX idx_incidents_appliance_id ON incidents (appliance_id);
CREATE INDEX idx_incidents_vendor_id ON incidents (vendor_id);
CREATE INDEX idx_incidents_deleted_at ON incidents (deleted_at);
CREATE TABLE service_log_entries (
  id INTEGER PRIMARY KEY,
  maintenance_item_id INTEGER NOT NULL,
  serviced_at TEXT NOT NULL,
  vendor_id INTEGER,
  cost_cents INTEGER,
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  deleted_at TEXT,
  FOREIGN KEY (maintenance_item_id) REFERENCES maintenance_items(id) ON DELETE CASCADE,
  FOREIGN KEY (vendor_id) REFERENCES vendors(id) ON DELETE SET NULL
);
CREATE INDEX idx_service_log_entries_maintenance_item_id ON service_log_entries (maintenance_item_id);
CREATE INDEX idx_service_log_entries_vendor_id ON service_log_entries (vendor_id);
CREATE INDEX idx_service_log_entries_deleted_at ON service_log_entries (deleted_at);
CREATE TABLE documents (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  file_name TEXT NOT NULL,
  entity_kind TEXT NOT NULL DEFAULT '',
  entity_id INTEGER NOT NULL DEFAULT 0,
  mime_type TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  data BLOB NOT NULL,
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  deleted_at TEXT
);
CREATE INDEX idx_doc_entity ON documents (entity_kind, entity_id);
CREATE INDEX idx_documents_deleted_at ON documents (deleted_at);
CREATE TABLE deletion_records (
  id INTEGER PRIMARY KEY,
  entity TEXT NOT NULL,
  target_id INTEGER NOT NULL,
  deleted_at TEXT NOT NULL,
  restored_at TEXT
);
CREATE INDEX idx_deletion_records_entity ON deletion_records (entity);
CREATE INDEX idx_deletion_records_target_id ON deletion_records (target_id);
CREATE INDEX idx_deletion_records_deleted_at ON deletion_records (deleted_at);
CREATE INDEX idx_entity_restored ON deletion_records (entity, restored_at);
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE chat_inputs (
  id INTEGER PRIMARY KEY,
  input TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
