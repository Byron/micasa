package app

import (
	"fmt"
	"time"

	"github.com/charmbracelet/bubbles/table"
	"github.com/micasa/micasa/internal/data"
)

func NewTabs(styles Styles) []Tab {
	return []Tab{
		{
			Kind:  tabProjects,
			Name:  "Projects",
			Table: newTable(projectColumns(), styles),
		},
		{
			Kind:  tabQuotes,
			Name:  "Quotes",
			Table: newTable(quoteColumns(), styles),
		},
		{
			Kind:  tabMaintenance,
			Name:  "Maintenance",
			Table: newTable(maintenanceColumns(), styles),
		},
	}
}

func projectColumns() []table.Column {
	return []table.Column{
		{Title: "ID", Width: 4},
		{Title: "Type", Width: 12},
		{Title: "Title", Width: 24},
		{Title: "Status", Width: 10},
		{Title: "Budget", Width: 12},
		{Title: "Actual", Width: 12},
		{Title: "Start", Width: 10},
		{Title: "End", Width: 10},
	}
}

func quoteColumns() []table.Column {
	return []table.Column{
		{Title: "ID", Width: 4},
		{Title: "Project", Width: 18},
		{Title: "Vendor", Width: 16},
		{Title: "Total", Width: 12},
		{Title: "Labor", Width: 12},
		{Title: "Mat", Width: 12},
		{Title: "Other", Width: 12},
		{Title: "Recv", Width: 10},
	}
}

func maintenanceColumns() []table.Column {
	return []table.Column{
		{Title: "ID", Width: 4},
		{Title: "Item", Width: 20},
		{Title: "Category", Width: 12},
		{Title: "Last", Width: 10},
		{Title: "Next", Width: 10},
		{Title: "Every", Width: 7},
		{Title: "Manual", Width: 12},
	}
}

func newTable(columns []table.Column, styles Styles) table.Model {
	tbl := table.New(
		table.WithColumns(columns),
		table.WithFocused(true),
	)
	tbl.SetStyles(table.Styles{
		Header:   styles.TableHeader,
		Selected: styles.TableSelected,
	})
	return tbl
}

func projectRows(
	projects []data.Project,
	styles Styles,
) ([]table.Row, []rowMeta) {
	rows := make([]table.Row, 0, len(projects))
	meta := make([]rowMeta, 0, len(projects))
	for _, project := range projects {
		deleted := project.DeletedAt.Valid
		values := []string{
			styles.Readonly.Render(fmt.Sprintf("%d", project.ID)),
			emptyOr(project.ProjectType.Name, styles),
			emptyOr(project.Title, styles),
			emptyOr(project.Status, styles),
			moneyOrEmpty(project.BudgetCents, styles),
			moneyOrEmpty(project.ActualCents, styles),
			dateOrEmpty(project.StartDate, styles),
			dateOrEmpty(project.EndDate, styles),
		}
		rows = append(rows, styledRow(values, deleted, styles))
		meta = append(meta, rowMeta{
			ID:      project.ID,
			Deleted: deleted,
		})
	}
	return rows, meta
}

func quoteRows(
	quotes []data.Quote,
	styles Styles,
) ([]table.Row, []rowMeta) {
	rows := make([]table.Row, 0, len(quotes))
	meta := make([]rowMeta, 0, len(quotes))
	for _, quote := range quotes {
		deleted := quote.DeletedAt.Valid
		projectName := quote.Project.Title
		if projectName == "" {
			projectName = fmt.Sprintf("Project %d", quote.ProjectID)
		}
		values := []string{
			styles.Readonly.Render(fmt.Sprintf("%d", quote.ID)),
			emptyOr(projectName, styles),
			emptyOr(quote.Vendor.Name, styles),
			styles.Money.Render(data.FormatCents(quote.TotalCents)),
			moneyOrEmpty(quote.LaborCents, styles),
			moneyOrEmpty(quote.MaterialsCents, styles),
			moneyOrEmpty(quote.OtherCents, styles),
			dateOrEmpty(quote.ReceivedDate, styles),
		}
		rows = append(rows, styledRow(values, deleted, styles))
		meta = append(meta, rowMeta{
			ID:      quote.ID,
			Deleted: deleted,
		})
	}
	return rows, meta
}

func maintenanceRows(
	items []data.MaintenanceItem,
	styles Styles,
) ([]table.Row, []rowMeta) {
	rows := make([]table.Row, 0, len(items))
	meta := make([]rowMeta, 0, len(items))
	for _, item := range items {
		deleted := item.DeletedAt.Valid
		manual := manualSummary(item)
		interval := ""
		if item.IntervalMonths > 0 {
			interval = fmt.Sprintf("%d mo", item.IntervalMonths)
		}
		values := []string{
			styles.Readonly.Render(fmt.Sprintf("%d", item.ID)),
			emptyOr(item.Name, styles),
			emptyOr(item.Category.Name, styles),
			dateOrEmpty(item.LastServicedAt, styles),
			dateOrEmpty(item.NextDueAt, styles),
			emptyOr(interval, styles),
			emptyOr(manual, styles),
		}
		rows = append(rows, styledRow(values, deleted, styles))
		meta = append(meta, rowMeta{
			ID:      item.ID,
			Deleted: deleted,
		})
	}
	return rows, meta
}

func styledRow(values []string, deleted bool, styles Styles) table.Row {
	if !deleted {
		return table.Row(values)
	}
	for i, value := range values {
		values[i] = styles.Deleted.Render(value)
	}
	return table.Row(values)
}

func emptyOr(value string, styles Styles) string {
	if value == "" {
		return styles.Empty.Render("n/a")
	}
	return value
}

func moneyOrEmpty(cents *int64, styles Styles) string {
	if cents == nil {
		return styles.Empty.Render("n/a")
	}
	return styles.Money.Render(data.FormatCents(*cents))
}

func dateOrEmpty(value *time.Time, styles Styles) string {
	if value == nil {
		return styles.Empty.Render("n/a")
	}
	return value.Format(data.DateLayout)
}

func manualSummary(item data.MaintenanceItem) string {
	if item.ManualText != "" {
		return "stored"
	}
	if item.ManualURL != "" {
		return "link"
	}
	return ""
}
