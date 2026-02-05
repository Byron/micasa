# Plans

Tracks in-flight feature plans and ad-hoc requests so context survives agent crashes/handoffs.

## Appliances Tab (remaining_work.md item 1) -- DONE

The first work item is a multi-part feature. Prior agent did most of the data + UI work but left the build broken. This session wired the remaining pieces.

**What was already done** (by prior agent, not logged):
- Data model: `Appliance` struct, store CRUD (Create/Get/Update/Delete/Restore/List)
- Table: `applianceColumnSpecs`, `applianceRows`, `NewTabs` includes Appliances
- Forms: `applianceFormData`, `startApplianceForm`, `startEditApplianceForm`, `openApplianceForm`, `submitApplianceForm`, `submitEditApplianceForm`
- Types: `formAppliance`, `tabAppliances`, `columnLink`, `cell.LinkID`
- Demo seed data: 7 appliances, 3 maintenance-appliance links
- Maintenance form: ApplianceID field, appliance select dropdown

**What this session added** (to fix build + complete wiring):
- `applianceOptions()` helper for huh select dropdowns
- `inlineEditAppliance()` for per-cell editing
- Switch cases in: `handleFormSubmit`, `startAddForm`, `startEditForm`, `deleteSelected`, `restoreByTab`, `deletionEntityForTab`, `reloadTab`, `tabLabel`, `tabIndex`, `buildSearchEntries`

**Cross-tab navigation (enter on linked cell)** -- NOT YET DONE:
- The `columnLink` struct and `cell.LinkID` are in place but no key handler navigates on enter yet.
- This is part of the same work item; needs: detect link on enter, switch tab, find row by ID.

## Remaining Work Items (from remaining_work.md)

1. **Appliance tab + cross-tab FK navigation** -- tab done, navigation TBD
2. **Column sorting** -- toggle asc/desc/none with keystroke, default PK sort
3. **Maintenance ghost text** -- compute next_due from last_serviced + interval as default
