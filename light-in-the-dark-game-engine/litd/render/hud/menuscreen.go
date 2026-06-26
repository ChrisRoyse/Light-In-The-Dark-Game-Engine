package hud

import "strconv"

// MenuScreen is the render-side binding for the g.UI() screen surface (#211,
// R-UI-1): it turns a resolved main-menu / terminal-screen spec into a
// validated canvas layout (a centered card panel + labels, over a draw-time
// palette background), the render twin of the api g.UI() event surface (#526).
// Strings arrive already locale-resolved by
// the composition root (D-17: locale KEYS live in the api UIScreen; the
// resolver sits at the g.OnUIScreen sink), so this file is presentation
// geometry only — no locale / sim / api dependency, same posture as
// CampaignMenu's *StringsFromLocale split.
//
// The layout is a pure function of (canvas, strings, buttons, focus): the same
// inputs always produce the same rects, so it is screenshot- and dump-FSV'able
// headlessly. Keyboard navigation is upstream input — it only moves the focus
// index (MenuFocusNext/Prev); the layout renders the focused button marked.

// MenuButton is one menu entry: a stable id for click/command routing and a
// resolved label.
type MenuButton struct {
	ID    string `json:"id"`
	Label string `json:"label"`
}

// MenuScreenStrings are the resolved chrome strings of a menu screen.
type MenuScreenStrings struct {
	Title    string
	Subtitle string
	Version  string
}

// MenuScreenLabel is one positioned text label, parented to a widget panel.
type MenuScreenLabel struct {
	Name    string `json:"name"`
	Parent  string `json:"parent"`
	Text    string `json:"text"`
	Focused bool   `json:"focused,omitempty"`
	Rect    Rect   `json:"rect"`
}

// MenuScreenLayout is the resolved, validated geometry for one menu screen.
type MenuScreenLayout struct {
	ID                string            `json:"id"`
	Canvas            Canvas            `json:"canvas"`
	Widgets           []Widget          `json:"widgets"`
	Labels            []MenuScreenLabel `json:"labels"`
	Buttons           []MenuButton      `json:"buttons"`
	Focused           int               `json:"focused"`
	ExpectedDrawCalls int               `json:"expectedDrawCalls"`
	Issues            []LayoutIssue     `json:"issues,omitempty"`
}

// MenuPanelName is the single centered card widget that holds the menu chrome
// and button stack. The background behind it is a draw-time palette fill
// (identity §6), not a validated widget — so the only layout widget is the card
// and the overlap/onscreen invariants stay meaningful.
const MenuPanelName = "menu-panel"

// menuFocusMarker prefixes the focused entry so the highlighted item is legible
// even in a pure-geometry screenshot (no atlas highlight sprite required).
const menuFocusMarker = "> "
const menuUnfocusPad = "  "

// menu content geometry, in reference units inside the card.
const (
	menuPadX       = 40
	menuTitleY     = 24
	menuTitleH     = 44
	menuSubtitleY  = 76
	menuSubtitleH  = 32
	menuButtonsTop = 124
	menuButtonStep = 52
	menuButtonH    = 40
	menuVersionH   = 24
	menuVersionGap = 12
	menuBottomPad  = 20
	menuContentW   = 360
	menuCardRefW   = 440
)

// NewMenuScreenLayout builds the validated layout for a menu screen. focused is
// clamped to a valid button index (or -1 when there are no buttons). It never
// panics on out-of-range focus or empty buttons (a title-only screen is valid).
func NewMenuScreenLayout(canvas Canvas, id string, strs MenuScreenStrings, buttons []MenuButton, focused int) MenuScreenLayout {
	focused = clampFocus(focused, len(buttons))

	// Card height grows with the button count so a 2-entry and a 5-entry menu
	// both stay centered and every entry stays inside the card.
	versionBottom := menuButtonsTop + len(buttons)*menuButtonStep + menuVersionGap + menuVersionH
	cardRefH := float64(versionBottom + menuBottomPad)
	// Center the card by putting its reference center on the reference center.
	ref := RefRect{
		X: ReferenceWidth/2 - menuCardRefW/2,
		Y: ReferenceHeight/2 - cardRefH/2,
		W: menuCardRefW,
		H: cardRefH,
	}
	spec := WidgetSpec{Name: MenuPanelName, Kind: WidgetNineSlice, Anchor: AnchorCenter, Ref: ref, AtlasRegion: "panel-large"}
	widgets := []Widget{{WidgetSpec: spec, Rect: canvas.Place(spec.Anchor, spec.Ref)}}

	layout := MenuScreenLayout{
		ID:      id,
		Canvas:  canvas,
		Widgets: widgets,
		Buttons: append([]MenuButton(nil), buttons...),
		Focused: focused,
	}
	add := menuLabelAdder(canvas, &layout)
	panel := widgets[0]

	// Title, subtitle, button stack, version — all parented to the card, stacked
	// vertically so no two labels overlap.
	add("menu-title", panel, strs.Title, menuPadX, menuTitleY, menuContentW, menuTitleH, false)
	if strs.Subtitle != "" {
		add("menu-subtitle", panel, strs.Subtitle, menuPadX, menuSubtitleY, menuContentW, menuSubtitleH, false)
	}
	for i, b := range buttons {
		text := menuUnfocusPad + b.Label
		isFocused := i == focused
		if isFocused {
			text = menuFocusMarker + b.Label
		}
		add("menu-button-"+strconv.Itoa(i), panel, text, menuPadX, float64(menuButtonsTop+i*menuButtonStep), menuContentW, menuButtonH, isFocused)
	}
	if strs.Version != "" {
		add("menu-version", panel, strs.Version, menuPadX, float64(menuButtonsTop+len(buttons)*menuButtonStep+menuVersionGap), menuContentW, menuVersionH, false)
	}

	finalizeMenuScreenLayout(&layout)
	return layout
}

func clampFocus(focused, n int) int {
	if n == 0 {
		return -1
	}
	if focused < 0 {
		return 0
	}
	if focused >= n {
		return n - 1
	}
	return focused
}

// MenuFocusNext / MenuFocusPrev move the keyboard focus with wrap-around. They
// are the navigation primitives the input layer drives; pure functions so nav
// is testable without a window. n is the button count.
func MenuFocusNext(focused, n int) int {
	if n <= 0 {
		return -1
	}
	if focused < 0 {
		return 0
	}
	return (focused + 1) % n
}

func MenuFocusPrev(focused, n int) int {
	if n <= 0 {
		return -1
	}
	if focused <= 0 {
		return n - 1
	}
	return focused - 1
}

func menuLabelAdder(canvas Canvas, layout *MenuScreenLayout) func(string, Widget, string, float64, float64, float64, float64, bool) {
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

func finalizeMenuScreenLayout(layout *MenuScreenLayout) {
	layout.ExpectedDrawCalls = len(layout.Widgets) + len(layout.Labels)
	layout.Issues = ValidateMenuScreenLayout(*layout)
}

// ValidateMenuScreenLayout reuses the campaign-menu invariants: every panel
// on-canvas, every label inside its parent, no two labels overlapping inside
// the same parent.
func ValidateMenuScreenLayout(layout MenuScreenLayout) []LayoutIssue {
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
