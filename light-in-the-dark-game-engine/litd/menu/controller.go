package menu

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/config"
)

// RuntimeSink is the seam between the settings reducer and the live game (#311
// slice 3): the controller pushes resolved settings to render / audio / input
// without the menu package importing any of them. The game shell implements it;
// tests fake it. ApplyGraphics returns whether the preset change needs a restart
// to fully take effect (some g3n state cannot swap live) — surfaced honestly to
// the player rather than silently no-op'd.
type RuntimeSink interface {
	ApplyGraphics(config.GraphicsPreset) (restartRequired bool)
	ApplyAudio(config.AudioVolumes)
	ApplyLocale(locale string)
	ApplyKeymap(keymap string)
}

// SettingsController is the non-render heart of the settings menu: it owns the
// current Settings, routes a menu command through config.Apply, persists the
// result, and re-applies the whole settings to the runtime sink. It is pure Go
// (no g3n) so the menu's behavior is fully testable; the g3n screen is a thin
// view that emits commands into Route and reads Settings/Screen back.
type SettingsController struct {
	settings        config.Settings
	installed       []string
	sink            RuntimeSink
	save            func(config.Settings) error
	restartRequired bool
}

// NewSettingsController seeds the controller and applies the initial settings to
// the sink so the runtime starts consistent with the loaded config. installed is
// the set of locale tags CycleLocale rotates through; save persists (typically
// config.Save) — a nil save makes persistence a no-op (e.g. a transient menu).
func NewSettingsController(s config.Settings, installed []string, sink RuntimeSink, save func(config.Settings) error) *SettingsController {
	c := &SettingsController{settings: s, installed: installed, sink: sink, save: save}
	c.apply() // start the runtime consistent with the loaded settings
	return c
}

// Route applies one menu command. On a recognized command it mutates the
// settings, persists them, and re-applies everything to the sink; it returns
// handled=true. An unrecognized command leaves settings untouched, does NOT
// persist, and returns handled=false (fail-closed: a stray command never writes
// the config or perturbs the runtime). A persistence error is returned but the
// in-memory + runtime change still stands (the player sees the effect; the next
// save retries) — the error is surfaced, never swallowed.
func (c *SettingsController) Route(command string) (handled bool, err error) {
	next, ok := config.Apply(c.settings, command, c.installed)
	if !ok {
		return false, nil
	}
	c.settings = next
	c.apply()
	if c.save != nil {
		if err := c.save(c.settings); err != nil {
			return true, err
		}
	}
	return true, nil
}

// apply pushes the full current settings to the runtime sink. Re-applying
// everything on any change keeps the logic trivial and idempotent — there is no
// per-field diffing to get wrong.
func (c *SettingsController) apply() {
	if c.sink == nil {
		return
	}
	c.restartRequired = c.sink.ApplyGraphics(c.settings.Graphics)
	c.sink.ApplyAudio(c.settings.Audio)
	c.sink.ApplyLocale(c.settings.Locale)
	c.sink.ApplyKeymap(c.settings.Keymap)
}

// Settings returns the current resolved settings.
func (c *SettingsController) Settings() config.Settings { return c.settings }

// RestartRequired reports whether the last graphics apply needs a restart to
// fully take effect — the menu shows this honestly next to the graphics control.
func (c *SettingsController) RestartRequired() bool { return c.restartRequired }

// Screen returns the settings menu spec to hand to g.UI().Show.
func (c *SettingsController) Screen() api.UIScreen { return SettingsScreen() }
