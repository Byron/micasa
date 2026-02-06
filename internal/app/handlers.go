package app

import (
	"fmt"

	"github.com/charmbracelet/bubbles/table"
	"github.com/micasa/micasa/internal/data"
)

// TabHandler encapsulates entity-specific operations for a tab, eliminating
// TabKind/FormKind switch dispatch scattered across the codebase. Each entity
// type (projects, quotes, maintenance, appliances) implements this interface.
type TabHandler interface {
	// FormKind returns the FormKind that identifies this entity in forms and
	// undo entries.
	FormKind() FormKind

	// Load fetches entities and converts them to table rows.
	Load(store *data.Store, showDeleted bool) ([]table.Row, []rowMeta, [][]cell, error)

	// Delete soft-deletes the entity with the given ID.
	Delete(store *data.Store, id uint) error

	// Restore reverses a soft-delete.
	Restore(store *data.Store, id uint) error

	// StartAddForm opens a "new entity" form on the model.
	StartAddForm(m *Model) error

	// StartEditForm opens an "edit entity" form for the given ID.
	StartEditForm(m *Model, id uint) error

	// InlineEdit opens a single-field editor for the given column.
	InlineEdit(m *Model, id uint, col int) error

	// SubmitForm persists the current form data (create or update).
	SubmitForm(m *Model) error

	// Snapshot captures the current DB state of an entity for undo/redo.
	Snapshot(store *data.Store, id uint) (undoEntry, bool)

	// SyncFixedValues updates column specs with values from dynamic lookup
	// tables so column widths stay stable.
	SyncFixedValues(m *Model, specs []columnSpec)
}

// handlerForFormKind finds the tab handler that owns the given FormKind.
// Returns nil for formHouse (no tab) or unknown kinds.
func (m *Model) handlerForFormKind(kind FormKind) TabHandler {
	for i := range m.tabs {
		if m.tabs[i].Handler != nil && m.tabs[i].Handler.FormKind() == kind {
			return m.tabs[i].Handler
		}
	}
	return nil
}

// ---------------------------------------------------------------------------
// projectHandler
// ---------------------------------------------------------------------------

type projectHandler struct{}

func (projectHandler) FormKind() FormKind { return formProject }

func (projectHandler) Load(
	store *data.Store,
	showDeleted bool,
) ([]table.Row, []rowMeta, [][]cell, error) {
	projects, err := store.ListProjects(showDeleted)
	if err != nil {
		return nil, nil, nil, err
	}
	rows, meta, cellRows := projectRows(projects)
	return rows, meta, cellRows, nil
}

func (projectHandler) Delete(store *data.Store, id uint) error {
	return store.DeleteProject(id)
}

func (projectHandler) Restore(store *data.Store, id uint) error {
	return store.RestoreProject(id)
}

func (projectHandler) StartAddForm(m *Model) error {
	m.startProjectForm()
	return nil
}

func (projectHandler) StartEditForm(m *Model, id uint) error {
	return m.startEditProjectForm(id)
}

func (projectHandler) InlineEdit(m *Model, id uint, col int) error {
	return m.inlineEditProject(id, col)
}

func (projectHandler) SubmitForm(m *Model) error {
	if m.editID != nil {
		return m.submitEditProjectForm(*m.editID)
	}
	return m.submitProjectForm()
}

func (projectHandler) Snapshot(store *data.Store, id uint) (undoEntry, bool) {
	project, err := store.GetProject(id)
	if err != nil {
		return undoEntry{}, false
	}
	return undoEntry{
		Description: fmt.Sprintf("project %q", project.Title),
		FormKind:    formProject,
		EntityID:    id,
		Restore: func() error {
			return store.UpdateProject(project)
		},
	}, true
}

func (projectHandler) SyncFixedValues(m *Model, specs []columnSpec) {
	typeNames := make([]string, len(m.projectTypes))
	for i, pt := range m.projectTypes {
		typeNames[i] = pt.Name
	}
	setFixedValues(specs, "Type", typeNames)
}

// ---------------------------------------------------------------------------
// quoteHandler
// ---------------------------------------------------------------------------

type quoteHandler struct{}

func (quoteHandler) FormKind() FormKind { return formQuote }

func (quoteHandler) Load(
	store *data.Store,
	showDeleted bool,
) ([]table.Row, []rowMeta, [][]cell, error) {
	quotes, err := store.ListQuotes(showDeleted)
	if err != nil {
		return nil, nil, nil, err
	}
	rows, meta, cellRows := quoteRows(quotes)
	return rows, meta, cellRows, nil
}

func (quoteHandler) Delete(store *data.Store, id uint) error {
	return store.DeleteQuote(id)
}

func (quoteHandler) Restore(store *data.Store, id uint) error {
	return store.RestoreQuote(id)
}

func (quoteHandler) StartAddForm(m *Model) error {
	return m.startQuoteForm()
}

func (quoteHandler) StartEditForm(m *Model, id uint) error {
	return m.startEditQuoteForm(id)
}

func (quoteHandler) InlineEdit(m *Model, id uint, col int) error {
	return m.inlineEditQuote(id, col)
}

func (quoteHandler) SubmitForm(m *Model) error {
	if m.editID != nil {
		return m.submitEditQuoteForm(*m.editID)
	}
	return m.submitQuoteForm()
}

func (quoteHandler) Snapshot(store *data.Store, id uint) (undoEntry, bool) {
	quote, err := store.GetQuote(id)
	if err != nil {
		return undoEntry{}, false
	}
	vendor := quote.Vendor
	return undoEntry{
		Description: fmt.Sprintf("quote from %s", vendor.Name),
		FormKind:    formQuote,
		EntityID:    id,
		Restore: func() error {
			return store.UpdateQuote(quote, vendor)
		},
	}, true
}

func (quoteHandler) SyncFixedValues(_ *Model, _ []columnSpec) {}

// ---------------------------------------------------------------------------
// maintenanceHandler
// ---------------------------------------------------------------------------

type maintenanceHandler struct{}

func (maintenanceHandler) FormKind() FormKind { return formMaintenance }

func (maintenanceHandler) Load(
	store *data.Store,
	showDeleted bool,
) ([]table.Row, []rowMeta, [][]cell, error) {
	items, err := store.ListMaintenance(showDeleted)
	if err != nil {
		return nil, nil, nil, err
	}
	rows, meta, cellRows := maintenanceRows(items)
	return rows, meta, cellRows, nil
}

func (maintenanceHandler) Delete(store *data.Store, id uint) error {
	return store.DeleteMaintenance(id)
}

func (maintenanceHandler) Restore(store *data.Store, id uint) error {
	return store.RestoreMaintenance(id)
}

func (maintenanceHandler) StartAddForm(m *Model) error {
	m.startMaintenanceForm()
	return nil
}

func (maintenanceHandler) StartEditForm(m *Model, id uint) error {
	return m.startEditMaintenanceForm(id)
}

func (maintenanceHandler) InlineEdit(m *Model, id uint, col int) error {
	return m.inlineEditMaintenance(id, col)
}

func (maintenanceHandler) SubmitForm(m *Model) error {
	if m.editID != nil {
		return m.submitEditMaintenanceForm(*m.editID)
	}
	return m.submitMaintenanceForm()
}

func (maintenanceHandler) Snapshot(store *data.Store, id uint) (undoEntry, bool) {
	item, err := store.GetMaintenance(id)
	if err != nil {
		return undoEntry{}, false
	}
	return undoEntry{
		Description: fmt.Sprintf("maintenance %q", item.Name),
		FormKind:    formMaintenance,
		EntityID:    id,
		Restore: func() error {
			return store.UpdateMaintenance(item)
		},
	}, true
}

func (maintenanceHandler) SyncFixedValues(m *Model, specs []columnSpec) {
	catNames := make([]string, len(m.maintenanceCategories))
	for i, c := range m.maintenanceCategories {
		catNames[i] = c.Name
	}
	setFixedValues(specs, "Category", catNames)
}

// ---------------------------------------------------------------------------
// applianceHandler
// ---------------------------------------------------------------------------

type applianceHandler struct{}

func (applianceHandler) FormKind() FormKind { return formAppliance }

func (applianceHandler) Load(
	store *data.Store,
	showDeleted bool,
) ([]table.Row, []rowMeta, [][]cell, error) {
	items, err := store.ListAppliances(showDeleted)
	if err != nil {
		return nil, nil, nil, err
	}
	rows, meta, cellRows := applianceRows(items)
	return rows, meta, cellRows, nil
}

func (applianceHandler) Delete(store *data.Store, id uint) error {
	return store.DeleteAppliance(id)
}

func (applianceHandler) Restore(store *data.Store, id uint) error {
	return store.RestoreAppliance(id)
}

func (applianceHandler) StartAddForm(m *Model) error {
	m.startApplianceForm()
	return nil
}

func (applianceHandler) StartEditForm(m *Model, id uint) error {
	return m.startEditApplianceForm(id)
}

func (applianceHandler) InlineEdit(m *Model, id uint, col int) error {
	return m.inlineEditAppliance(id, col)
}

func (applianceHandler) SubmitForm(m *Model) error {
	if m.editID != nil {
		return m.submitEditApplianceForm(*m.editID)
	}
	return m.submitApplianceForm()
}

func (applianceHandler) Snapshot(store *data.Store, id uint) (undoEntry, bool) {
	item, err := store.GetAppliance(id)
	if err != nil {
		return undoEntry{}, false
	}
	return undoEntry{
		Description: fmt.Sprintf("appliance %q", item.Name),
		FormKind:    formAppliance,
		EntityID:    id,
		Restore: func() error {
			return store.UpdateAppliance(item)
		},
	}, true
}

func (applianceHandler) SyncFixedValues(_ *Model, _ []columnSpec) {}
