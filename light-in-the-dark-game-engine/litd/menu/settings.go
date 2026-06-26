// Package menu builds the api.UIScreen specs for the game's menu screens (#311 and
// the wider menu-flow cluster). A screen is pure data — locale KEYS, stable button
// ids, and command tags — that g.UI().Show validates and the render layer
// (hud.NewMenuScreenLayout) lays out. Holding the specs here, rather than inline at
// each call site, gives the eventual menu controller and the render harness one
// canonical source, and lets the toggle/cycle controls carry the config.Apply
// command tags so a click routes straight into the settings reducer.
package menu

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/config"
)

// SettingsScreenID is the stable id of the settings screen (Show replaces it, Hide
// targets it).
const SettingsScreenID = "settings"

// BackCommand is the command tag the Back button emits; it is handled by the menu
// controller (return to the previous screen), not by the settings reducer.
const BackCommand = "settings.back"

// SettingsScreen builds the settings menu spec. The toggle/cycle controls carry the
// config.Apply command tags (settings.graphics.toggle, settings.keymap.toggle,
// settings.locale.cycle), so a button press routes straight into the reducer. The
// per-group audio volumes are render-side value rows formatted from live Settings
// (like the terminal screen keeps its stat rows out of the UIScreen), not buttons.
func SettingsScreen() api.UIScreen {
	return api.UIScreen{
		ID:       SettingsScreenID,
		TitleKey: string(locale.SettingsTitle),
		Buttons: []api.UIButton{
			{ID: "graphics", LabelKey: string(locale.SettingsGraphics), Command: string(config.ToggleGraphics)},
			{ID: "keymap", LabelKey: string(locale.SettingsKeymap), Command: string(config.ToggleKeymap)},
			{ID: "locale", LabelKey: string(locale.SettingsLocale), Command: string(config.CycleLocale)},
			{ID: "back", LabelKey: string(locale.SettingsBack), Command: BackCommand},
		},
	}
}
