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

**Cross-tab navigation (enter on linked cell)** -- DONE:
- `navigateToLink()` switches tab and selects target row by ID
- `selectedCell()` helper reads cell at current cursor position
- Header shows relation type (e.g. "m:1") in muted rose via `LinkIndicator` style
- Status bar shows "follow m:1" hint when cursor is on a linked cell with a target
- Works for Quotes.Project (m:1 -> Projects) and Maintenance.Appliance (m:1 -> Appliances)
- For empty links (e.g. maintenance with no appliance), falls through to normal edit

## House Profile UX Redesign (RW-HOUSE-UX)

**Problem**: Collapsed and expanded house profile views feel like a "wall of text tags." Every key-value pair is wrapped in a `RoundedBorder` chip box, creating dense visual noise.

**Collapsed (before)**: Title row + row of 6 bordered chip boxes (House, Loc, Yr, Sq Ft, Beds, Baths)
**Expanded (before)**: Title row + 2 chip rows + 3 section rows each packed with bordered chips

**Design**:

Collapsed -- single clean middot-separated line, no borders:
```
House Profile ▸  h toggle
Elm Street · Springfield, IL · 4bd / 2.5ba · 2,400 sqft · 1987
```
Nickname pops in orange (HeaderValue), stats in subdued gray (HeaderHint).

Expanded -- section headers with inline middot-separated values, no chip borders:
```
House Profile ▾  h toggle
Elm Street · 742 Elm Street, Springfield, IL 62704

 Structure  1987 · 2,400 sqft · 8,500 lot · 4bd / 2.5ba
            fnd Poured Concrete · wir Copper · roof Asphalt Shingle
            ext Vinyl Siding · bsmt Finished
 Utilities  heat Forced Air Gas · cool Central AC · water Municipal
            sewer Municipal · park Attached 2-Car
 Financial  ins Acme Insurance · policy HO-00-0000000 · renew 2026-08-15
            tax $4,850.00 · hoa Elm Street HOA ($150.00/mo)
```
Section headers use existing HeaderSection style. Values use dim label + bright value (`hlv` helper). Continuation lines indent to align with values.

**Implementation**:
1. Add helpers: `styledPart`, `bedBathLabel`, `sqftLabel`, `lotLabel`, `hlv`, `houseSection`
2. Rewrite `houseCollapsed` and `houseExpanded`
3. Remove now-unused `chip`, `sectionLine`, `renderHouseValue`, `HeaderChip` style

## Remaining Work Items (from remaining_work.md)

1. **Appliance tab + cross-tab FK navigation** -- tab done, navigation TBD
2. **Column sorting** -- toggle asc/desc/none with keystroke, default PK sort
3. **Maintenance ghost text** -- compute next_due from last_serviced + interval as default
