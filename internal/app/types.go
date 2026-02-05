package app

import "github.com/charmbracelet/bubbles/table"

type Mode int

const (
	modeTable Mode = iota
	modeForm
)

type FormKind int

const (
	formNone FormKind = iota
	formHouse
	formProject
	formQuote
	formMaintenance
)

type TabKind int

const (
	tabProjects TabKind = iota
	tabQuotes
	tabMaintenance
)

type rowMeta struct {
	ID      uint
	Deleted bool
}

type Tab struct {
	Kind        TabKind
	Name        string
	Table       table.Model
	Rows        []rowMeta
	ShowDeleted bool
}

type statusKind int

const (
	statusInfo statusKind = iota
	statusError
)

type statusMsg struct {
	Text string
	Kind statusKind
}
