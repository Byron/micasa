// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

package app

import (
	"sort"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// ladleStrings holds the border prefix/suffix for the ladle shape.
type ladleStrings struct {
	left  string // "│ " — applied to body, connector, and stack lines
	right string // " │"
	width int    // total horizontal chars consumed by both sides
}

// ladleChrome computes the ladle border strings for edge hidden columns.
func ladleChrome(hasLeading, hasTrailing bool) ladleStrings {
	ls := ladleStrings{}
	style := lipgloss.NewStyle().Foreground(secondary)
	if hasLeading {
		ls.left = style.Render("│") + " "
		ls.width += 2
	}
	if hasTrailing {
		ls.right = " " + style.Render("│")
		ls.width += 2
	}
	return ls
}

// renderLadleBottom draws the horizontal base of the ladle L-shape, closing
// off the bottom of the candy pill area. ╰──── on the left, ────╯ on the right.
func renderLadleBottom(
	stacks []collapsedStack,
	hasLeading, hasTrailing bool,
	leftWidth, colSpaceWidth int,
) string {
	if !hasLeading && !hasTrailing {
		return ""
	}
	style := lipgloss.NewStyle().Foreground(secondary)

	rightWidth := 0
	if hasTrailing {
		rightWidth = 2
	}
	fullWidth := leftWidth + colSpaceWidth + rightWidth

	// Both edges: single continuous curve spanning the full line width.
	if hasLeading && hasTrailing {
		if fullWidth < 2 {
			return ""
		}
		return style.Render("╰" + strings.Repeat("─", fullWidth-2) + "╯")
	}

	// Left edge only.
	if hasLeading {
		var leadW int
		for _, s := range stacks {
			if s.edge && s.offset == 0 {
				leadW = s.width
				break
			}
		}
		if leadW > 0 {
			return style.Render("╰" + strings.Repeat("─", 1+leadW))
		}
		return ""
	}

	// Right edge only.
	var trailOff int
	for _, s := range stacks {
		if s.edge {
			trailOff = s.offset
			break
		}
	}
	start := leftWidth + trailOff
	dashLen := colSpaceWidth - trailOff + 1
	return strings.Repeat(" ", start) +
		style.Render(strings.Repeat("─", dashLen)+"╯")
}

// wrapLines prepends prefix and appends suffix to every line in s.
func wrapLines(s, prefix, suffix string) string {
	if prefix == "" && suffix == "" {
		return s
	}
	lines := strings.Split(s, "\n")
	for i, line := range lines {
		lines[i] = prefix + line + suffix
	}
	return strings.Join(lines, "\n")
}

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

type stackEntry struct {
	name      string
	fullIndex int
	hideOrder int
}

type collapsedStack struct {
	entries []stackEntry // most recent (highest hideOrder) first
	offset  int          // horizontal character offset for pill left edge
	width   int          // pill width including padding
	edge    bool         // true for leading/trailing stacks (ladle handles connector)
}

func candyColor(index int) lipgloss.AdaptiveColor {
	palette := []lipgloss.AdaptiveColor{accent, secondary, warning, success, textMid}
	return palette[index%len(palette)]
}

// computeCollapsedStacks finds groups of hidden columns between visible ones
// and returns a positioned stack for each group.
func computeCollapsedStacks(
	specs []columnSpec,
	visToFull []int,
	widths []int,
	sepWidth int,
) []collapsedStack {
	n := len(visToFull)
	if n == 0 {
		return nil
	}

	var stacks []collapsedStack

	// Leading hidden columns (before first visible).
	if visToFull[0] > 0 {
		if entries := collectHiddenEntries(specs, 0, visToFull[0]); len(entries) > 0 {
			w := maxEntryWidth(entries) + 2
			stacks = append(stacks, collapsedStack{
				entries: entries, offset: 0, width: w, edge: true,
			})
		}
	}

	// Between visible columns (anchored to ⋯ gaps).
	offset := 0
	for i := 0; i < n; i++ {
		if i > 0 {
			lo := visToFull[i-1] + 1
			hi := visToFull[i]
			if hi > lo {
				if entries := collectHiddenEntries(specs, lo, hi); len(entries) > 0 {
					w := maxEntryWidth(entries) + 2
					gapCenter := offset + sepWidth/2
					pillOff := gapCenter - w/2
					if pillOff < 0 {
						pillOff = 0
					}
					stacks = append(stacks, collapsedStack{
						entries: entries, offset: pillOff, width: w,
					})
				}
			}
			offset += sepWidth
		}
		if i < len(widths) {
			offset += widths[i]
		}
	}

	// Trailing hidden columns (after last visible).
	if last := visToFull[n-1]; last < len(specs)-1 {
		if entries := collectHiddenEntries(specs, last+1, len(specs)); len(entries) > 0 {
			w := maxEntryWidth(entries) + 2
			trailOff := offset - w
			if trailOff < 0 {
				trailOff = 0
			}
			stacks = append(stacks, collapsedStack{
				entries: entries, offset: trailOff, width: w, edge: true,
			})
		}
	}

	// Clamp pill widths so they never exceed the total column space,
	// then merge any stacks that overlap after clamping.
	totalWidth := sumInts(widths)
	if len(widths) > 1 {
		totalWidth += (len(widths) - 1) * sepWidth
	}
	for i := range stacks {
		if stacks[i].width > totalWidth {
			stacks[i].width = totalWidth
		}
		if stacks[i].offset+stacks[i].width > totalWidth {
			stacks[i].offset = totalWidth - stacks[i].width
		}
		if stacks[i].offset < 0 {
			stacks[i].offset = 0
		}
	}

	// Merge overlapping stacks (e.g. leading+trailing both clamped to offset 0).
	merged := stacks[:0]
	for _, s := range stacks {
		if len(merged) > 0 {
			last := &merged[len(merged)-1]
			if s.offset < last.offset+last.width {
				end := last.offset + last.width
				if se := s.offset + s.width; se > end {
					end = se
				}
				last.entries = append(last.entries, s.entries...)
				last.width = end - last.offset
				if last.width > totalWidth {
					last.width = totalWidth
				}
				last.edge = last.edge || s.edge
				continue
			}
		}
		merged = append(merged, s)
	}
	// Re-sort merged entries by column index descending (rightmost on top).
	for i := range merged {
		sort.Slice(merged[i].entries, func(a, b int) bool {
			return merged[i].entries[a].fullIndex > merged[i].entries[b].fullIndex
		})
	}

	return merged
}

// collectHiddenEntries gathers hidden columns in [lo, hi) ordered so that the
// leftmost column is at the bottom of the stack and the rightmost is on top
// (closest to the data rows). This preserves spatial intuition: the column
// nearest the gap edge sits at the top of the visual stack.
func collectHiddenEntries(specs []columnSpec, lo, hi int) []stackEntry {
	var entries []stackEntry
	for i := hi - 1; i >= lo; i-- {
		if specs[i].HideOrder > 0 {
			entries = append(entries, stackEntry{
				name: specs[i].Title, fullIndex: i, hideOrder: specs[i].HideOrder,
			})
		}
	}
	return entries
}

func maxEntryWidth(entries []stackEntry) int {
	max := 0
	for _, e := range entries {
		if w := lipgloss.Width(e.name); w > max {
			max = w
		}
	}
	return max
}

// renderCollapsedStacks renders one line per stack depth, with candy-colored
// pills positioned horizontally at each gap.
func renderCollapsedStacks(stacks []collapsedStack) []string {
	if len(stacks) == 0 {
		return nil
	}
	maxDepth := 0
	for _, s := range stacks {
		if len(s.entries) > maxDepth {
			maxDepth = len(s.entries)
		}
	}
	lines := make([]string, 0, maxDepth)
	for depth := 0; depth < maxDepth; depth++ {
		lines = append(lines, renderStackLine(stacks, depth))
	}
	return lines
}

// renderStackConnector draws a thin vertical line at the center of each
// non-edge stack, connecting the data rows above to the candy pills below.
// Edge stacks are handled by the ladle curves instead.
func renderStackConnector(stacks []collapsedStack) string {
	connStyle := lipgloss.NewStyle().Foreground(secondary)
	var b strings.Builder
	cursor := 0
	any := false
	for _, stack := range stacks {
		if stack.edge {
			continue
		}
		any = true
		center := stack.offset + stack.width/2
		if center > cursor {
			b.WriteString(strings.Repeat(" ", center-cursor))
			cursor = center
		}
		b.WriteString(connStyle.Render("│"))
		cursor++
	}
	if !any {
		return ""
	}
	return b.String()
}

func renderStackLine(stacks []collapsedStack, depth int) string {
	type positioned struct {
		offset int
		text   string
		width  int
	}
	var pills []positioned
	for _, stack := range stacks {
		if depth >= len(stack.entries) {
			continue
		}
		entry := stack.entries[depth]
		style := lipgloss.NewStyle().
			Background(candyColor(entry.fullIndex)).
			Foreground(onAccent).
			Bold(true).
			Width(stack.width).
			Align(lipgloss.Center)
		pills = append(pills, positioned{
			offset: stack.offset,
			text:   style.Render(entry.name),
			width:  stack.width,
		})
	}
	sort.Slice(pills, func(i, j int) bool {
		return pills[i].offset < pills[j].offset
	})
	var b strings.Builder
	cursor := 0
	for _, p := range pills {
		if p.offset > cursor {
			b.WriteString(strings.Repeat(" ", p.offset-cursor))
			cursor = p.offset
		}
		b.WriteString(p.text)
		cursor += p.width
	}
	return b.String()
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
