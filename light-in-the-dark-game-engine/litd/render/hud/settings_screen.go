package hud

import (
	"strconv"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/config"
)

// SettingsScreen is the render-side binding for the settings menu (#311): it turns
// the live config.Settings into a validated centered-card layout whose rows show
// the CURRENT value of each setting ("Graphics: High", "Master: 0.8", ...). It is
// the sibling of TerminalScreen — same centered-card + value-row discipline, same
// validation — differing only in content: the persisted settings instead of
// end-of-match stats. menu.SettingsScreen carries the clickable command tags (the
// reducer routing); this screen renders the live values those commands change, the
// "render-side value rows formatted from live Settings" the menu spec defers here.
//
// D-17 split: the localizable chrome (the row labels, the localized preset/keymap
// value names) arrives already resolved in SettingsScreenStrings; the dynamic
// values (volumes, locale tag) are formatted in here. So the locale table carries
// the chrome while the per-user values stay out of it.
//
// Pure function of (canvas, settings, strings, focused): identical inputs always
// produce identical rects, so it is screenshot- and dump-FSV'able headlessly.

// SettingsScreenStrings are the resolved chrome strings of the settings screen,
// all locale-resolved upstream (D-17). The *Value fields are the localized names
// of the current enum values (graphics preset, keymap profile).
type SettingsScreenStrings struct {
	Title         string
	GraphicsLabel string
	GraphicsHigh  string
	GraphicsLow   string
	MasterLabel   string
	WorldLabel    string
	UILabel       string
	MusicLabel    string
	AmbienceLabel string
	LocaleLabel   string
	KeymapLabel   string
	KeymapGrid    string
	KeymapClassic string
	BackLabel     string
}

// SettingsScreenStringsFromLocale resolves every settings-screen string from a
// locale table (cf. CampaignMenuStringsFromLocale).
func SettingsScreenStringsFromLocale(table *locale.Table) SettingsScreenStrings {
	return SettingsScreenStrings{
		Title:         table.Must(locale.SettingsTitle),
		GraphicsLabel: table.Must(locale.SettingsGraphics),
		GraphicsHigh:  table.Must(locale.SettingsGraphicsHigh),
		GraphicsLow:   table.Must(locale.SettingsGraphicsLow),
		MasterLabel:   table.Must(locale.SettingsAudioMaster),
		WorldLabel:    table.Must(locale.SettingsAudioWorld),
		UILabel:       table.Must(locale.SettingsAudioUI),
		MusicLabel:    table.Must(locale.SettingsAudioMusic),
		AmbienceLabel: table.Must(locale.SettingsAudioAmbience),
		LocaleLabel:   table.Must(locale.SettingsLocale),
		KeymapLabel:   table.Must(locale.SettingsKeymap),
		KeymapGrid:    table.Must(locale.SettingsKeymapGrid),
		KeymapClassic: table.Must(locale.SettingsKeymapClassic),
		BackLabel:     table.Must(locale.SettingsBack),
	}
}

// SettingsScreenLayout is the resolved, validated geometry for the settings screen.
// It reuses MenuScreenLabel (the generic positioned-label type) and the shared
// widget validation so the menu screens stay consistent.
type SettingsScreenLayout struct {
	ID                string            `json:"id"`
	Canvas            Canvas            `json:"canvas"`
	Widgets           []Widget          `json:"widgets"`
	Labels            []MenuScreenLabel `json:"labels"`
	Settings          config.Settings   `json:"-"`
	Focused           int               `json:"focused"`
	ExpectedDrawCalls int               `json:"expectedDrawCalls"`
	Issues            []LayoutIssue     `json:"issues,omitempty"`
}

// SettingsPanelName is the single centered card widget (as in TerminalScreen the
// background fill behind it is a draw-time palette fill, not a validated widget).
const SettingsPanelName = "settings-panel"

// SettingsScreenID is the stable layout id (matches menu.SettingsScreenID).
const SettingsScreenID = "settings"

// settings content geometry, in reference units inside the card. Mirrors the
// terminal geometry; the card is sized to title + 8 value rows + back.
const (
	settingsPadX      = 40
	settingsTitleY    = 24
	settingsTitleH    = 48
	settingsRowsTop   = 96
	settingsRowStep   = 40
	settingsRowH      = 32
	settingsBackGap   = 20
	settingsBackH     = 40
	settingsBottomPad = 24
	settingsContentW  = 420
	settingsCardRefW  = 500
	// SettingsValueRows is the count of value rows (graphics, 5 audio groups,
	// locale, keymap) — the focusable rows before Back.
	SettingsValueRows = 8
)

// SettingsRowName is the stable label id for value row i. Stable ids let a
// dump-FSV assert a specific row's resolved text. Row 0 is graphics, 1..5 are the
// audio groups, 6 locale, 7 keymap; SettingsBackName is the trailing button.
func SettingsRowName(i int) string { return "settings-row-" + strconv.Itoa(i) }

// SettingsBackName is the stable id of the Back button row.
const SettingsBackName = "settings-back"

// NewSettingsScreenLayout builds the validated layout. Each value row is formatted
// "<label>: <value>" from the live settings; focused (clamped to the focusable
// rows: 0..SettingsValueRows where SettingsValueRows is Back) highlights one row
// for the arrow-key cursor. It never panics; empty strings still yield a valid card.
func NewSettingsScreenLayout(canvas Canvas, s config.Settings, strs SettingsScreenStrings, focused int) SettingsScreenLayout {
	backTop := settingsRowsTop + SettingsValueRows*settingsRowStep + settingsBackGap
	cardRefH := float64(backTop + settingsBackH + settingsBottomPad)
	ref := RefRect{
		X: ReferenceWidth/2 - settingsCardRefW/2,
		Y: ReferenceHeight/2 - cardRefH/2,
		W: settingsCardRefW,
		H: cardRefH,
	}
	spec := WidgetSpec{Name: SettingsPanelName, Kind: WidgetNineSlice, Anchor: AnchorCenter, Ref: ref, AtlasRegion: "panel-large"}
	widgets := []Widget{{WidgetSpec: spec, Rect: canvas.Place(spec.Anchor, spec.Ref)}}

	focused = clampFocus(focused, SettingsValueRows+1) // value rows + Back
	layout := SettingsScreenLayout{
		ID:       SettingsScreenID,
		Canvas:   canvas,
		Widgets:  widgets,
		Settings: s,
		Focused:  focused,
	}
	add := settingsLabelAdder(canvas, &layout)
	panel := widgets[0]

	add("settings-title", panel, strs.Title, settingsPadX, settingsTitleY, settingsContentW, settingsTitleH, false)

	rows := []struct {
		label string
		value string
	}{
		{strs.GraphicsLabel, graphicsValue(s.Graphics, strs)},
		{strs.MasterLabel, volumeValue(s.Audio.Master)},
		{strs.WorldLabel, volumeValue(s.Audio.World)},
		{strs.UILabel, volumeValue(s.Audio.UI)},
		{strs.MusicLabel, volumeValue(s.Audio.Music)},
		{strs.AmbienceLabel, volumeValue(s.Audio.Ambience)},
		{strs.LocaleLabel, s.Locale},
		{strs.KeymapLabel, keymapValue(s.Keymap, strs)},
	}
	for i, row := range rows {
		text := row.label + ": " + row.value
		y := float64(settingsRowsTop + i*settingsRowStep)
		add(SettingsRowName(i), panel, text, settingsPadX, y, settingsContentW, settingsRowH, focused == i)
	}

	add(SettingsBackName, panel, strs.BackLabel, settingsPadX, float64(backTop), settingsContentW, settingsBackH, focused == SettingsValueRows)

	layout.ExpectedDrawCalls = len(layout.Widgets) + len(layout.Labels)
	layout.Issues = ValidateSettingsScreenLayout(layout)
	return layout
}

// graphicsValue resolves the localized name of the current graphics preset.
func graphicsValue(p config.GraphicsPreset, strs SettingsScreenStrings) string {
	if p == config.PresetLow {
		return strs.GraphicsLow
	}
	return strs.GraphicsHigh
}

// keymapValue resolves the localized name of the current keymap profile.
func keymapValue(keymap string, strs SettingsScreenStrings) string {
	if keymap == "classic" {
		return strs.KeymapClassic
	}
	return strs.KeymapGrid
}

// volumeValue formats an audio gain on the 0.1 grid as one decimal ("0.0".."1.0").
func volumeValue(v float64) string {
	return strconv.FormatFloat(v, 'f', 1, 64)
}

func settingsLabelAdder(canvas Canvas, layout *SettingsScreenLayout) func(string, Widget, string, float64, float64, float64, float64, bool) {
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

// ValidateSettingsScreenLayout reuses the menu-screen invariants: the panel
// on-canvas, every label inside its parent, no two labels overlapping inside the
// same parent.
func ValidateSettingsScreenLayout(layout SettingsScreenLayout) []LayoutIssue {
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
