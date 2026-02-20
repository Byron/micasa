# Fixture Notes

`go-schema-v1.db` is a compatibility fixture used by Rust integration tests.

- It contains the full table/index layout expected from the legacy Go runtime.
- It intentionally includes only seeded lookup rows in `project_types` and
  `maintenance_categories`.
- User tables (`projects`, `vendors`, `documents`, etc.) are empty so no
  personal data is stored in-repo.

`go-schema-v1.sql` is the schema dump used to create the fixture DB.
