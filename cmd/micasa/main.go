package main

import (
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/micasa/micasa/internal/app"
	"github.com/micasa/micasa/internal/data"
)

func main() {
	dbPath, err := data.DefaultDBPath()
	if err != nil {
		fail("resolve db path", err)
	}
	store, err := data.Open(dbPath)
	if err != nil {
		fail("open database", err)
	}
	if err := store.AutoMigrate(); err != nil {
		fail("migrate database", err)
	}
	if err := store.SeedDefaults(); err != nil {
		fail("seed defaults", err)
	}
	model, err := app.NewModel(store)
	if err != nil {
		fail("initialize app", err)
	}
	if _, err := tea.NewProgram(model, tea.WithAltScreen()).Run(); err != nil {
		fail("run app", err)
	}
}

func fail(context string, err error) {
	fmt.Fprintf(os.Stderr, "micasa: %s: %v\n", context, err)
	os.Exit(1)
}
