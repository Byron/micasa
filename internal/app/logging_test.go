package app

import (
	"testing"

	"github.com/charmbracelet/lipgloss"
)

func TestMatchLineNoFilter(t *testing.T) {
	ls := newLogState(1)
	if !ls.matchLine("anything") {
		t.Fatal("expected match when no filter is set")
	}
}

func TestMatchLineWithPattern(t *testing.T) {
	ls := newLogState(1)
	ls.setFilter("error")

	if !ls.matchLine("an error occurred") {
		t.Fatal("expected match for line containing 'error'")
	}
	if ls.matchLine("all good") {
		t.Fatal("expected no match for line without 'error'")
	}
}

func TestMatchLineInvalidRegex(t *testing.T) {
	ls := newLogState(1)
	ls.setFilter("[invalid")

	if !ls.matchLine("anything") {
		t.Fatal("expected match to pass through on invalid regex")
	}
}

func TestFindHighlightsNoFilter(t *testing.T) {
	ls := newLogState(1)
	highlights := ls.findHighlights("anything")
	if len(highlights) != 0 {
		t.Fatalf("expected no highlights, got %d", len(highlights))
	}
}

func TestFindHighlightsSingleMatch(t *testing.T) {
	ls := newLogState(1)
	ls.setFilter("err")

	highlights := ls.findHighlights("an error occurred")
	if len(highlights) != 1 {
		t.Fatalf("expected 1 highlight, got %d", len(highlights))
	}
	if highlights[0].Start != 3 || highlights[0].End != 6 {
		t.Fatalf("expected span [3,6), got [%d,%d)", highlights[0].Start, highlights[0].End)
	}
}

func TestFindHighlightsMultipleMatches(t *testing.T) {
	ls := newLogState(1)
	ls.setFilter("o")

	highlights := ls.findHighlights("foo boo")
	if len(highlights) < 2 {
		t.Fatalf("expected at least 2 highlights, got %d", len(highlights))
	}
}

func TestFindHighlightsInvalidRegex(t *testing.T) {
	ls := newLogState(1)
	ls.setFilter("[bad")

	highlights := ls.findHighlights("anything")
	if len(highlights) != 0 {
		t.Fatalf("expected no highlights on invalid regex, got %d", len(highlights))
	}
}

func TestApplyHighlightsEmpty(t *testing.T) {
	result := applyHighlights("hello world", nil, lipgloss.NewStyle())
	if result != "hello world" {
		t.Fatalf("expected unchanged text, got %q", result)
	}
}

func TestApplyHighlightsSingle(t *testing.T) {
	// Use a no-op style so we can verify the text content is preserved.
	style := lipgloss.NewStyle()
	spans := []logMatch{{Start: 0, End: 5}}
	result := applyHighlights("hello world", spans, style)
	if result != "hello world" {
		t.Fatalf("expected 'hello world' with no-op style, got %q", result)
	}
}

func TestApplyHighlightsOutOfBounds(t *testing.T) {
	style := lipgloss.NewStyle()
	spans := []logMatch{{Start: 50, End: 60}}
	result := applyHighlights("short", spans, style)
	if result != "short" {
		t.Fatalf("expected unchanged text for out-of-bounds span, got %q", result)
	}
}

func TestApplyHighlightsOverlapping(t *testing.T) {
	style := lipgloss.NewStyle()
	spans := []logMatch{{Start: 0, End: 5}, {Start: 3, End: 8}}
	result := applyHighlights("hello world", spans, style)
	// Overlapping spans: first covers [0,5), second adjusted to [5,8)
	if result != "hello world" {
		t.Fatalf("expected 'hello world' with no-op style, got %q", result)
	}
}

func TestLogStateAppend(t *testing.T) {
	ls := newLogState(1)
	ls.append(logInfo, "test message")
	if len(ls.entries) != 1 {
		t.Fatalf("expected 1 entry, got %d", len(ls.entries))
	}
	if ls.entries[0].Message != "test message" {
		t.Fatalf("expected 'test message', got %q", ls.entries[0].Message)
	}
}

func TestLogStateAppendDisabled(t *testing.T) {
	ls := newLogState(0)
	ls.append(logInfo, "should not be stored")
	if len(ls.entries) != 0 {
		t.Fatalf("expected 0 entries when disabled, got %d", len(ls.entries))
	}
}

func TestLogStateAppendEmpty(t *testing.T) {
	ls := newLogState(1)
	ls.append(logInfo, "   ")
	if len(ls.entries) != 0 {
		t.Fatalf("expected 0 entries for blank message, got %d", len(ls.entries))
	}
}

func TestLogStateAppendRespectsLevel(t *testing.T) {
	ls := newLogState(1) // maxLevel = logInfo
	ls.append(logDebug, "debug msg")
	if len(ls.entries) != 0 {
		t.Fatalf("expected 0 entries for debug at verbosity 1, got %d", len(ls.entries))
	}
}

func TestLogStateMaxEntries(t *testing.T) {
	ls := newLogState(1)
	ls.maxEntries = 3
	for i := 0; i < 5; i++ {
		ls.append(logInfo, "msg")
	}
	if len(ls.entries) != 3 {
		t.Fatalf("expected 3 entries after overflow, got %d", len(ls.entries))
	}
}

func TestSetFilterClear(t *testing.T) {
	ls := newLogState(1)
	ls.setFilter("test")
	if ls.filter == nil {
		t.Fatal("expected filter to be set")
	}
	ls.setFilter("")
	if ls.filter != nil {
		t.Fatal("expected filter to be cleared")
	}
}

func TestValidityLabel(t *testing.T) {
	ls := newLogState(1)
	if ls.validityLabel() != "no filter" {
		t.Fatalf("expected 'no filter', got %q", ls.validityLabel())
	}
	// validityLabel checks input.Value(), so set it via the input model.
	ls.input.SetValue("test")
	ls.setFilter("test")
	if ls.validityLabel() != "valid" {
		t.Fatalf("expected 'valid', got %q", ls.validityLabel())
	}
	ls.input.SetValue("[bad")
	ls.setFilter("[bad")
	if got := ls.validityLabel(); got == "valid" || got == "no filter" {
		t.Fatalf("expected invalid label, got %q", got)
	}
}
