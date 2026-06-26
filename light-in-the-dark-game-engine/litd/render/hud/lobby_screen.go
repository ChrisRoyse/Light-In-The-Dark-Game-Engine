package hud

import (
	"strconv"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

// LobbyScreen is the render-side binding for the 2-8 player session-bootstrap lobby
// (#80): a centered card with one row per slot (name + localized status) and a Start
// action row. It is the render twin of the settings/stall screens — same
// centered-card + value-row discipline, same validation — driven by the net.Lobby's
// slot snapshot, but it takes flat rows (already-resolved name + a hud-local status
// enum) so the hud package never imports litd/net: the game shell maps each
// net.LobbySlot to a LobbyScreenSlot. Pure function of its inputs, so it is
// screenshot/dump-FSV'able.

// LobbySlotStatus is the hud-local mirror of a slot's display state. The game shell
// maps net.SlotState (+ IsHost) onto it: empty->Open, host->Host, occupied->Waiting,
// ready->Ready.
type LobbySlotStatus uint8

const (
	LobbySlotStatusOpen LobbySlotStatus = iota
	LobbySlotStatusHost
	LobbySlotStatusWaiting
	LobbySlotStatusReady
)

// LobbyScreenSlot is one resolved slot row.
type LobbyScreenSlot struct {
	Name   string          `json:"name,omitempty"`
	Status LobbySlotStatus `json:"status"`
}

// LobbyScreenStrings are the resolved chrome strings (D-17, resolved upstream).
type LobbyScreenStrings struct {
	Title   string
	Open    string
	Host    string
	Ready   string
	Waiting string
	Start   string
}

// LobbyScreenStringsFromLocale resolves the lobby chrome from a locale table.
func LobbyScreenStringsFromLocale(table *locale.Table) LobbyScreenStrings {
	return LobbyScreenStrings{
		Title:   table.Must(locale.LobbyTitle),
		Open:    table.Must(locale.LobbySlotOpen),
		Host:    table.Must(locale.LobbySlotHost),
		Ready:   table.Must(locale.LobbySlotReady),
		Waiting: table.Must(locale.LobbySlotWaiting),
		Start:   table.Must(locale.LobbyStart),
	}
}

// LobbyScreenLayout is the resolved, validated geometry for the lobby overlay.
type LobbyScreenLayout struct {
	ID                string            `json:"id"`
	Canvas            Canvas            `json:"canvas"`
	Widgets           []Widget          `json:"widgets"`
	Labels            []MenuScreenLabel `json:"labels"`
	Slots             []LobbyScreenSlot `json:"slots"`
	CanStart          bool              `json:"canStart"`
	Focused           int               `json:"focused"`
	ExpectedDrawCalls int               `json:"expectedDrawCalls"`
	Issues            []LayoutIssue     `json:"issues,omitempty"`
}

const (
	// LobbyPanelName is the single centered card widget.
	LobbyPanelName = "lobby-panel"
	// LobbyScreenID is the stable layout id.
	LobbyScreenID = "lobby"
	// LobbyStartName is the stable id of the Start action row.
	LobbyStartName = "lobby-start"
)

const (
	lobbyPadX      = 40
	lobbyTitleY    = 24
	lobbyTitleH    = 40
	lobbyRowsTop   = 84
	lobbyRowStep   = 34
	lobbyRowH      = 28
	lobbyStartGap  = 18
	lobbyStartH    = 34
	lobbyBottomPad = 26
	lobbyContentW  = 460
	lobbyCardRefW  = 540
)

// LobbyRowName is the stable label id for slot row i.
func LobbyRowName(i int) string { return "lobby-slot-" + strconv.Itoa(i) }

// NewLobbyScreenLayout builds the validated lobby card. slots are the resolved
// per-slot rows (index 0 = host); canStart gates the Start row's highlight; focused
// indexes the focusable rows [0..len(slots)] where the last index is the Start row
// (clamped into range). It never panics.
func NewLobbyScreenLayout(canvas Canvas, slots []LobbyScreenSlot, canStart bool, focused int, strs LobbyScreenStrings) LobbyScreenLayout {
	focusCount := len(slots) + 1 // slot rows + the Start row
	if focused < 0 {
		focused = 0
	}
	if focused >= focusCount {
		focused = focusCount - 1
	}
	startTop := lobbyRowsTop + len(slots)*lobbyRowStep + lobbyStartGap
	cardRefH := float64(startTop + lobbyStartH + lobbyBottomPad)
	ref := RefRect{
		X: ReferenceWidth/2 - lobbyCardRefW/2,
		Y: ReferenceHeight/2 - cardRefH/2,
		W: lobbyCardRefW,
		H: cardRefH,
	}
	spec := WidgetSpec{Name: LobbyPanelName, Kind: WidgetNineSlice, Anchor: AnchorCenter, Ref: ref, AtlasRegion: "panel-large"}
	widgets := []Widget{{WidgetSpec: spec, Rect: canvas.Place(spec.Anchor, spec.Ref)}}

	layout := LobbyScreenLayout{
		ID:       LobbyScreenID,
		Canvas:   canvas,
		Widgets:  widgets,
		Slots:    append([]LobbyScreenSlot(nil), slots...),
		CanStart: canStart,
		Focused:  focused,
	}
	add := lobbyLabelAdder(canvas, &layout)
	panel := widgets[0]

	add("lobby-title", panel, strs.Title, lobbyPadX, lobbyTitleY, lobbyContentW, lobbyTitleH, false)
	for i, slot := range slots {
		text := lobbyRowText(i, slot, strs)
		y := float64(lobbyRowsTop + i*lobbyRowStep)
		add(LobbyRowName(i), panel, text, lobbyPadX, y, lobbyContentW, lobbyRowH, focused == i)
	}
	startText := strs.Start
	if !canStart {
		startText = strs.Start + " (waiting…)"
	}
	add(LobbyStartName, panel, startText, lobbyPadX, float64(startTop), lobbyContentW, lobbyStartH, focused == len(slots))

	layout.ExpectedDrawCalls = len(layout.Widgets) + len(layout.Labels)
	layout.Issues = ValidateLobbyScreenLayout(layout)
	return layout
}

func lobbyRowText(i int, slot LobbyScreenSlot, strs LobbyScreenStrings) string {
	prefix := strconv.Itoa(i+1) + ". "
	if slot.Status == LobbySlotStatusOpen {
		return prefix + strs.Open
	}
	return prefix + slot.Name + " — " + lobbyStatusValue(slot.Status, strs)
}

func lobbyStatusValue(s LobbySlotStatus, strs LobbyScreenStrings) string {
	switch s {
	case LobbySlotStatusHost:
		return strs.Host
	case LobbySlotStatusReady:
		return strs.Ready
	case LobbySlotStatusWaiting:
		return strs.Waiting
	default:
		return strs.Open
	}
}

func lobbyLabelAdder(canvas Canvas, layout *LobbyScreenLayout) func(string, Widget, string, float64, float64, float64, float64, bool) {
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

// ValidateLobbyScreenLayout reuses the menu-screen invariants.
func ValidateLobbyScreenLayout(layout LobbyScreenLayout) []LayoutIssue {
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
