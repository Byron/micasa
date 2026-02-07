// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

package app

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// gapSeparators computes a per-gap separator for the header/data and divider.
// Gaps between visible columns that have hidden columns in between use ⋯ to
// signal a collapsed region. Returns one separator per gap (len(visToFull)-1).
func gapSeparators(
	visToFull []int,
	totalCols int,
	normalSep string,
	styles Styles,
) (plainSeps, collapsedSeps []string) {
	n := len(visToFull)
	if n <= 1 {
		return nil, nil
	}
	collapsedSep := styles.TableSeparator.Render(" ") +
		lipgloss.NewStyle().Foreground(secondary).Render("⋯") +
		styles.TableSeparator.Render(" ")

	plainSeps = make([]string, n-1)
	collapsedSeps = make([]string, n-1)
	for i := 0; i < n-1; i++ {
		plainSeps[i] = normalSep
		if visToFull[i+1] > visToFull[i]+1 {
			collapsedSeps[i] = collapsedSep
		} else {
			collapsedSeps[i] = normalSep
		}
	}
	return
}

// hiddenColumnNames returns the titles of all hidden columns.
func hiddenColumnNames(specs []columnSpec) []string {
	var names []string
	for _, s := range specs {
		if s.HideOrder > 0 {
			names = append(names, s.Title)
		}
	}
	return names
}

// renderHiddenBadges renders a single line showing hidden column names,
// split by their position relative to the cursor. Columns to the left of
// the cursor are left-aligned in one color; columns to the right are
// right-aligned in another. This gives spatial awareness of what's hidden.
func renderHiddenBadges(
	specs []columnSpec,
	colCursor int,
	width int,
	styles Styles,
) string {
	var leftNames, rightNames []string
	for i, spec := range specs {
		if spec.HideOrder == 0 {
			continue
		}
		if i < colCursor {
			leftNames = append(leftNames, spec.Title)
		} else {
			rightNames = append(rightNames, spec.Title)
		}
	}
	if len(leftNames) == 0 && len(rightNames) == 0 {
		return ""
	}

	sep := styles.HeaderHint.Render(" · ")

	var leftStr, rightStr string
	if len(leftNames) > 0 {
		parts := make([]string, len(leftNames))
		for i, name := range leftNames {
			parts[i] = styles.HiddenLeft.Render(name)
		}
		leftStr = strings.Join(parts, sep)
	}
	if len(rightNames) > 0 {
		parts := make([]string, len(rightNames))
		for i, name := range rightNames {
			parts[i] = styles.HiddenRight.Render(name)
		}
		rightStr = strings.Join(parts, sep)
	}

	leftW := lipgloss.Width(leftStr)
	rightW := lipgloss.Width(rightStr)

	// Both sides present: left-align left group, right-align right group.
	if leftStr != "" && rightStr != "" {
		gap := width - leftW - rightW
		if gap < 1 {
			gap = 1
		}
		return leftStr + strings.Repeat(" ", gap) + rightStr
	}

	// Left only: left-aligned.
	if leftStr != "" {
		return leftStr
	}

	// Right only: right-aligned.
	pad := width - rightW
	if pad < 0 {
		pad = 0
	}
	return strings.Repeat(" ", pad) + rightStr
}
