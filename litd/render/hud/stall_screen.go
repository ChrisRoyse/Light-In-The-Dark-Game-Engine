package hud

import (
	"strconv"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

// StallScreen is the render-side binding for the netplay stall overlay (#71): when
// the lockstep gate blocks on a lagging player, the match pauses and this centered
// card names the laggard(s) and counts down the grace period before they are
// dropped. It is the render twin of TerminalScreen — same centered-card + value-row
// discipline, same validation — driven by the net.StallController's decision, but
// it takes flat params (already-resolved laggard names + seconds remaining) so the
// hud package never imports litd/net: the game shell maps the controller output to
// these fields. Pure function of its inputs, so it is screenshot/dump-FSV'able.

// StallScreenStrings are the resolved chrome strings (D-17, resolved upstream).
type StallScreenStrings struct {
	Title    string // "Connection Stalled"
	Waiting  string // "Waiting for" — prefixes each laggard name
	Dropping string // "Dropping in" — prefixes the countdown
}

// StallScreenStringsFromLocale resolves the stall chrome from a locale table.
func StallScreenStringsFromLocale(table *locale.Table) StallScreenStrings {
	return StallScreenStrings{
		Title:    table.Must(locale.StallTitle),
		Waiting:  table.Must(locale.StallWaiting),
		Dropping: table.Must(locale.StallDropping),
	}
}

// StallScreenLayout is the resolved, validated geometry for the stall overlay.
type StallScreenLayout struct {
	ID                string            `json:"id"`
	Canvas            Canvas            `json:"canvas"`
	Widgets           []Widget          `json:"widgets"`
	Labels            []MenuScreenLabel `json:"labels"`
	Laggards          []string          `json:"laggards"`
	SecondsRemaining  int               `json:"secondsRemaining"`
	ExpectedDrawCalls int               `json:"expectedDrawCalls"`
	Issues            []LayoutIssue     `json:"issues,omitempty"`
}

// StallPanelName is the single centered card widget.
const StallPanelName = "stall-panel"

// StallScreenID is the stable layout id.
const StallScreenID = "stall"

const (
	stallPadX       = 40
	stallTitleY     = 24
	stallTitleH     = 40
	stallRowsTop    = 80
	stallRowStep    = 36
	stallRowH       = 28
	stallCountGap   = 16
	stallCountH     = 32
	stallBottomPad  = 24
	stallContentW   = 440
	stallCardRefW   = 520
	stallMinRowSlot = 1 // reserve at least one laggard row so the card never collapses
)

// StallRowName is the stable label id for laggard row i.
func StallRowName(i int) string { return "stall-laggard-" + strconv.Itoa(i) }

// StallCountdownName is the stable id of the countdown row.
const StallCountdownName = "stall-countdown"

// NewStallScreenLayout builds the validated overlay. laggards are the
// already-resolved display names of the lagging player(s); secondsRemaining is the
// grace countdown (clamped to >=0). It never panics; an empty laggard list still
// yields a valid card (the gate can be momentarily blocked with the name not yet
// resolved), sized for at least one row.
func NewStallScreenLayout(canvas Canvas, laggards []string, secondsRemaining int, strs StallScreenStrings) StallScreenLayout {
	if secondsRemaining < 0 {
		secondsRemaining = 0
	}
	rowSlots := len(laggards)
	if rowSlots < stallMinRowSlot {
		rowSlots = stallMinRowSlot
	}
	countTop := stallRowsTop + rowSlots*stallRowStep + stallCountGap
	cardRefH := float64(countTop + stallCountH + stallBottomPad)
	ref := RefRect{
		X: ReferenceWidth/2 - stallCardRefW/2,
		Y: ReferenceHeight/2 - cardRefH/2,
		W: stallCardRefW,
		H: cardRefH,
	}
	spec := WidgetSpec{Name: StallPanelName, Kind: WidgetNineSlice, Anchor: AnchorCenter, Ref: ref, AtlasRegion: "panel-large"}
	widgets := []Widget{{WidgetSpec: spec, Rect: canvas.Place(spec.Anchor, spec.Ref)}}

	layout := StallScreenLayout{
		ID:               StallScreenID,
		Canvas:           canvas,
		Widgets:          widgets,
		Laggards:         append([]string(nil), laggards...),
		SecondsRemaining: secondsRemaining,
	}
	add := stallLabelAdder(canvas, &layout)
	panel := widgets[0]

	add("stall-title", panel, strs.Title, stallPadX, stallTitleY, stallContentW, stallTitleH, true)
	for i, name := range laggards {
		text := strs.Waiting + " " + name
		y := float64(stallRowsTop + i*stallRowStep)
		add(StallRowName(i), panel, text, stallPadX, y, stallContentW, stallRowH, false)
	}
	countText := strs.Dropping + " " + strconv.Itoa(secondsRemaining) + "s"
	add(StallCountdownName, panel, countText, stallPadX, float64(countTop), stallContentW, stallCountH, false)

	layout.ExpectedDrawCalls = len(layout.Widgets) + len(layout.Labels)
	layout.Issues = ValidateStallScreenLayout(layout)
	return layout
}

func stallLabelAdder(canvas Canvas, layout *StallScreenLayout) func(string, Widget, string, float64, float64, float64, float64, bool) {
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

// ValidateStallScreenLayout reuses the menu-screen invariants.
func ValidateStallScreenLayout(layout StallScreenLayout) []LayoutIssue {
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
