package hud

import "strconv"

// TerminalScreen is the render-side binding for the end-of-match screen (#201):
// it turns a resolved result + end-match stats into a validated canvas layout (a
// centered card: result title, the duration/trained/lost stat rows, an exit
// button), the render twin of the match-flow terminal phase. It is the sibling
// of MenuScreen (#211) — same centered-card + labels + draw-time palette
// background discipline, same validation — differing only in content: a result
// headline plus dynamic stat rows instead of a button stack.
//
// D-17 split: the localizable chrome (Victory/Defeat headline, the stat-row
// labels, the Exit label) arrives already locale-resolved in
// TerminalScreenStrings; the dynamic numbers come from the flow's Stats and are
// formatted in here as "<label>: <value>". So a UIScreen (locale KEYS only)
// carries the chrome while the per-match values stay out of the locale table.
//
// Pure function of (canvas, result, stats, strings): identical inputs always
// produce identical rects, so it is screenshot- and dump-FSV'able headlessly.

// TerminalResult is the match outcome a terminal screen renders. It selects the
// headline string (chosen upstream) and the headline tint at draw time.
type TerminalResult uint8

const (
	// TerminalDefeat renders the defeat headline (the zero value: an
	// unresolved/again-defaulted result reads as a loss, fail-closed).
	TerminalDefeat TerminalResult = iota
	// TerminalVictory renders the victory headline.
	TerminalVictory
)

// Won reports whether this result is a victory (headline tint selector).
func (r TerminalResult) Won() bool { return r == TerminalVictory }

// TerminalStats are the dynamic end-of-match numbers — the values, not the
// labels. DurationTicks is sim ticks; the layout renders it as a whole-second
// count (TicksPerSecond is a sim constant the caller already divides by if it
// wants seconds — here it is shown verbatim as ticks to stay sim-unit honest).
type TerminalStats struct {
	DurationTicks int
	UnitsTrained  int
	UnitsLost     int
}

// TerminalScreenStrings are the resolved chrome strings of a terminal screen.
// Title is the already-result-selected headline (the resolved Victory or Defeat
// string); the three *Label fields are the stat-row labels; ExitLabel is the
// button. All resolved upstream (D-17).
type TerminalScreenStrings struct {
	Title         string
	DurationLabel string
	TrainedLabel  string
	LostLabel     string
	ExitLabel     string
}

// TerminalScreenLayout is the resolved, validated geometry for one terminal
// screen. It reuses MenuScreenLabel (the generic positioned-label type) and the
// shared widget validation so the two screens stay consistent.
type TerminalScreenLayout struct {
	ID                string            `json:"id"`
	Canvas            Canvas            `json:"canvas"`
	Result            TerminalResult    `json:"result"`
	Widgets           []Widget          `json:"widgets"`
	Labels            []MenuScreenLabel `json:"labels"`
	Stats             TerminalStats     `json:"stats"`
	ExpectedDrawCalls int               `json:"expectedDrawCalls"`
	Issues            []LayoutIssue     `json:"issues,omitempty"`
}

// TerminalPanelName is the single centered card widget. As in MenuScreen, the
// background behind it is a draw-time palette fill, not a validated widget, so
// the card is the only layout widget and the overlap/onscreen invariants stay
// meaningful.
const TerminalPanelName = "terminal-panel"

// terminal content geometry, in reference units inside the card. Mirrors the
// menu geometry; the card is sized to title + 3 stat rows + exit.
const (
	terminalPadX      = 40
	terminalTitleY    = 24
	terminalTitleH    = 48
	terminalStatsTop  = 96
	terminalStatStep  = 40
	terminalStatH     = 32
	terminalExitGap   = 20
	terminalExitH     = 40
	terminalBottomPad = 24
	terminalContentW  = 360
	terminalCardRefW  = 440
	terminalStatRows  = 3
)

// TerminalRowName is the stable label id for stat row i (0=duration, 1=trained,
// 2=lost). Stable ids let a dump-FSV assert a specific row's resolved text.
func TerminalRowName(i int) string { return "terminal-stat-" + strconv.Itoa(i) }

// NewTerminalScreenLayout builds the validated layout for a terminal screen. The
// stat rows are formatted "<label>: <value>" from strings + stats. It never
// panics; a screen with empty strings still produces a valid (if sparse) card.
func NewTerminalScreenLayout(canvas Canvas, id string, result TerminalResult, stats TerminalStats, strs TerminalScreenStrings) TerminalScreenLayout {
	// Card height is fixed by the row count (title + 3 stats + exit), centered.
	exitTop := terminalStatsTop + terminalStatRows*terminalStatStep + terminalExitGap
	cardRefH := float64(exitTop + terminalExitH + terminalBottomPad)
	ref := RefRect{
		X: ReferenceWidth/2 - terminalCardRefW/2,
		Y: ReferenceHeight/2 - cardRefH/2,
		W: terminalCardRefW,
		H: cardRefH,
	}
	spec := WidgetSpec{Name: TerminalPanelName, Kind: WidgetNineSlice, Anchor: AnchorCenter, Ref: ref, AtlasRegion: "panel-large"}
	widgets := []Widget{{WidgetSpec: spec, Rect: canvas.Place(spec.Anchor, spec.Ref)}}

	layout := TerminalScreenLayout{
		ID:      id,
		Canvas:  canvas,
		Result:  result,
		Widgets: widgets,
		Stats:   stats,
	}
	add := terminalLabelAdder(canvas, &layout)
	panel := widgets[0]

	// Headline (Victory / Defeat), focused=true so the draw step tints it.
	add("terminal-title", panel, strs.Title, terminalPadX, terminalTitleY, terminalContentW, terminalTitleH, true)

	rows := []struct {
		label string
		value int
	}{
		{strs.DurationLabel, stats.DurationTicks},
		{strs.TrainedLabel, stats.UnitsTrained},
		{strs.LostLabel, stats.UnitsLost},
	}
	for i, row := range rows {
		text := row.label + ": " + strconv.Itoa(row.value)
		y := float64(terminalStatsTop + i*terminalStatStep)
		add(TerminalRowName(i), panel, text, terminalPadX, y, terminalContentW, terminalStatH, false)
	}

	add("terminal-exit", panel, strs.ExitLabel, terminalPadX, float64(exitTop), terminalContentW, terminalExitH, false)

	layout.ExpectedDrawCalls = len(layout.Widgets) + len(layout.Labels)
	layout.Issues = ValidateTerminalScreenLayout(layout)
	return layout
}

func terminalLabelAdder(canvas Canvas, layout *TerminalScreenLayout) func(string, Widget, string, float64, float64, float64, float64, bool) {
	return func(name string, parent Widget, text string, x, y, w, h float64, focused bool) {
		layout.Labels = append(layout.Labels, MenuScreenLabel{
			Name:    name,
			Parent:  parent.Name,
			Text:    text,
			Focused: focused,
			Rect: Rect{
				X: parent.Rect.X + canvas.Snap(x),
				Y: parent.Rect.Y + canvas.Snap(y),
				W: canvas.Snap(w),
				H: canvas.Snap(h),
			},
		})
	}
}

// ValidateTerminalScreenLayout reuses the menu-screen invariants: the panel
// on-canvas, every label inside its parent, no two labels overlapping inside the
// same parent.
func ValidateTerminalScreenLayout(layout TerminalScreenLayout) []LayoutIssue {
	issues := ValidateWidgets(layout.Widgets, layout.Canvas.Width, layout.Canvas.Height)
	for i, label := range layout.Labels {
		parent, ok := findWidget(layout.Widgets, label.Parent)
		if !ok {
			issues = append(issues, LayoutIssue{Widget: label.Name, Rule: "label-parent", Msg: "label parent is missing"})
			continue
		}
		if !label.Rect.InsideRect(parent.Rect) {
			issues = append(issues, LayoutIssue{Widget: label.Name, Rule: "label-offscreen", Msg: "label leaves parent widget"})
		}
		for j := 0; j < i; j++ {
			prev := layout.Labels[j]
			if prev.Parent == label.Parent && label.Rect.Overlaps(prev.Rect) {
				issues = append(issues, LayoutIssue{Widget: label.Name, Rule: "label-overlap", Msg: "labels overlap inside " + label.Parent})
			}
		}
	}
	return issues
}
