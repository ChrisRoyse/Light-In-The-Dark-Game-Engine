package config

// actions.go is the settings-menu reducer (#311 slice 2): the pure mutation core
// behind every settings control. The menu UI (slice 3, over g.UI()) emits a command
// tag per click; Apply maps that tag to one deterministic change of Settings. It is
// the testable behavior of the menu with no render or api dependency — a button
// press is "load → Apply → Save". Unknown tags leave the settings untouched and
// report not-handled, so a stray command can never corrupt the config.

import (
	"math"
	"strings"
)

// Action is a settings-menu command tag (UIButton.Command). The menu screen carries
// these; Apply consumes them.
type Action string

const (
	// ToggleGraphics flips the graphics preset high<->low.
	ToggleGraphics Action = "settings.graphics.toggle"
	// CycleLocale advances to the next installed locale (wrapping).
	CycleLocale Action = "settings.locale.cycle"
	// ToggleKeymap flips the keymap profile grid<->classic.
	ToggleKeymap Action = "settings.keymap.toggle"
)

// volumeStep is one notch of an audio slider; volumes move on a 0.1 grid in [0,1].
const volumeStep = 0.1

// audioActionPrefix tags the per-group volume commands:
// "settings.audio.<group>.up" / ".down" for group in master|world|ui|music|ambience.
const audioActionPrefix = "settings.audio."

// Apply returns the settings after applying one menu action and whether the action
// was recognized. installed is the set of locale tags CycleLocale rotates through
// (the loader's installed tables); it is ignored by the other actions. An
// unrecognized action returns the input unchanged and false (fail-closed: a stray
// command never mutates state).
func Apply(s Settings, action string, installed []string) (Settings, bool) {
	switch Action(action) {
	case ToggleGraphics:
		if s.Graphics == PresetHigh {
			s.Graphics = PresetLow
		} else {
			s.Graphics = PresetHigh
		}
		return s, true
	case ToggleKeymap:
		if s.Keymap == "classic" {
			s.Keymap = "grid"
		} else {
			s.Keymap = "classic"
		}
		return s, true
	case CycleLocale:
		next, ok := nextLocale(s.Locale, installed)
		if !ok {
			return s, false // no installed locales to rotate through
		}
		s.Locale = next
		return s, true
	}
	if grp, dir, ok := parseAudioAction(action); ok {
		return adjustVolume(s, grp, dir), true
	}
	return s, false
}

// nextLocale picks the locale after cur in installed, wrapping at the end. If cur is
// absent from installed, the first entry is returned (recover a stale tag). Reports
// false only when installed is empty.
func nextLocale(cur string, installed []string) (string, bool) {
	if len(installed) == 0 {
		return cur, false
	}
	for i, tag := range installed {
		if tag == cur {
			return installed[(i+1)%len(installed)], true
		}
	}
	return installed[0], true
}

// parseAudioAction splits an "settings.audio.<group>.<up|down>" tag into the group
// pointer-selector and a +1/-1 direction. Returns ok=false for any other tag.
func parseAudioAction(action string) (group string, dir int, ok bool) {
	if !strings.HasPrefix(action, audioActionPrefix) {
		return "", 0, false
	}
	rest := action[len(audioActionPrefix):] // "<group>.<dir>"
	dot := strings.LastIndexByte(rest, '.')
	if dot <= 0 || dot == len(rest)-1 {
		return "", 0, false
	}
	group = rest[:dot]
	switch rest[dot+1:] {
	case "up":
		dir = +1
	case "down":
		dir = -1
	default:
		return "", 0, false
	}
	switch group {
	case "master", "world", "ui", "music", "ambience":
		return group, dir, true
	default:
		return "", 0, false
	}
}

// adjustVolume nudges one audio group by one step in dir, clamped to [0,1] and
// snapped to the 0.1 grid so repeated steps never drift off it (float error).
func adjustVolume(s Settings, group string, dir int) Settings {
	step := func(v float64) float64 {
		v = math.Round((v+float64(dir)*volumeStep)*10) / 10
		if v < 0 {
			return 0
		}
		if v > 1 {
			return 1
		}
		return v
	}
	switch group {
	case "master":
		s.Audio.Master = step(s.Audio.Master)
	case "world":
		s.Audio.World = step(s.Audio.World)
	case "ui":
		s.Audio.UI = step(s.Audio.UI)
	case "music":
		s.Audio.Music = step(s.Audio.Music)
	case "ambience":
		s.Audio.Ambience = step(s.Audio.Ambience)
	}
	return s
}
