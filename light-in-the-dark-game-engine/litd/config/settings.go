// Package config holds the persisted user settings (#311): graphics preset,
// audio volume groups, locale, and keymap profile, with TOML load/save.
//
// The core is filesystem-free — the library takes and returns bytes and the
// caller owns the disk, exactly as litd/input/keymap does — so it reads no files
// and is trivially testable. It imports nothing from litd/sim: settings touch
// render / audio / input only and carry zero determinism surface (a settings
// change must never alter the sim hash). LoadSettings is fail-closed: a corrupt,
// empty, or partial config never errors out, it falls back to safe defaults and
// reports a warning, so the game always launches.
package config

import (
	"bytes"
	"fmt"
	"io"
	"strings"

	"github.com/BurntSushi/toml"
)

// GraphicsPreset selects the render quality path.
type GraphicsPreset uint8

const (
	// PresetHigh is the PBR lit path (#141) — the default, full quality.
	PresetHigh GraphicsPreset = iota
	// PresetLow is the unlit low path (#143, R-RND-5) for weak GPUs.
	PresetLow
)

// String renders the preset as its TOML token.
func (p GraphicsPreset) String() string {
	if p == PresetLow {
		return "low"
	}
	return "high"
}

// AudioVolumes are the per-group gains, each in [0,1]. They map to the audio
// voice domains (audio.md §4-5): master scales the rest.
type AudioVolumes struct {
	Master   float64
	World    float64
	UI       float64
	Music    float64
	Ambience float64
}

// Settings is the full persisted user configuration.
type Settings struct {
	Graphics GraphicsPreset
	Audio    AudioVolumes
	Locale   string // a D-17 locale tag ("en", ...); validated by the locale loader
	Keymap   string // a keymap profile: "grid" | "classic"
}

// DefaultSettings is the safe baseline used on first run and as the fallback for
// any missing/invalid field.
func DefaultSettings() Settings {
	return Settings{
		Graphics: PresetHigh,
		Audio:    AudioVolumes{Master: 1, World: 1, UI: 1, Music: 0.8, Ambience: 0.8},
		Locale:   "en",
		Keymap:   "grid",
	}
}

// raw* mirror the on-disk TOML. Audio volumes are pointers so a missing field
// (nil) is distinguishable from an explicit 0 (muted) — without that, a config
// that omits a group could not be told apart from one that mutes it.
type rawSettings struct {
	Graphics rawGraphics `toml:"graphics"`
	Audio    rawAudio    `toml:"audio"`
	Input    rawInput    `toml:"input"`
	Locale   string      `toml:"locale"`
}

type rawGraphics struct {
	Preset string `toml:"preset"`
}

type rawAudio struct {
	Master   *float64 `toml:"master"`
	World    *float64 `toml:"world"`
	UI       *float64 `toml:"ui"`
	Music    *float64 `toml:"music"`
	Ambience *float64 `toml:"ambience"`
}

type rawInput struct {
	Keymap string `toml:"keymap"`
}

// LoadSettings parses blob into Settings, falling back to DefaultSettings() for
// every missing or invalid field and returning a warning describing each
// fallback. It NEVER fails (fail-closed): a corrupt or empty blob yields the
// defaults plus a warning, so the caller can always launch and rewrite a clean
// config. Out-of-range audio volumes are clamped to [0,1] with a warning.
func LoadSettings(blob []byte) (Settings, []string) {
	def := DefaultSettings()
	if len(bytes.TrimSpace(blob)) == 0 {
		return def, []string{"config: empty or missing — using defaults"}
	}
	var raw rawSettings
	if _, err := toml.Decode(string(blob), &raw); err != nil {
		return def, []string{fmt.Sprintf("config: parse error (%v) — using defaults", err)}
	}

	s := def
	var warns []string

	if raw.Graphics.Preset != "" {
		if p, ok := parseGraphicsPreset(raw.Graphics.Preset); ok {
			s.Graphics = p
		} else {
			warns = append(warns, fmt.Sprintf("config: unknown graphics.preset %q — using %q", raw.Graphics.Preset, def.Graphics))
		}
	}

	s.Audio.Master = clampVolume(raw.Audio.Master, def.Audio.Master, "master", &warns)
	s.Audio.World = clampVolume(raw.Audio.World, def.Audio.World, "world", &warns)
	s.Audio.UI = clampVolume(raw.Audio.UI, def.Audio.UI, "ui", &warns)
	s.Audio.Music = clampVolume(raw.Audio.Music, def.Audio.Music, "music", &warns)
	s.Audio.Ambience = clampVolume(raw.Audio.Ambience, def.Audio.Ambience, "ambience", &warns)

	if raw.Input.Keymap != "" {
		if k, ok := parseKeymapProfile(raw.Input.Keymap); ok {
			s.Keymap = k
		} else {
			warns = append(warns, fmt.Sprintf("config: unknown input.keymap %q — using %q", raw.Input.Keymap, def.Keymap))
		}
	}

	if raw.Locale != "" {
		s.Locale = raw.Locale
	}

	return s, warns
}

// WriteTOML writes the canonical TOML form of the settings (the clean form used
// to rewrite a corrupt config). It round-trips with LoadSettings: loading the
// output reproduces the same Settings.
func (s Settings) WriteTOML(w io.Writer) error {
	out := rawSettings{
		Graphics: rawGraphics{Preset: s.Graphics.String()},
		Audio: rawAudio{
			Master:   floatPtr(s.Audio.Master),
			World:    floatPtr(s.Audio.World),
			UI:       floatPtr(s.Audio.UI),
			Music:    floatPtr(s.Audio.Music),
			Ambience: floatPtr(s.Audio.Ambience),
		},
		Input:  rawInput{Keymap: s.Keymap},
		Locale: s.Locale,
	}
	return toml.NewEncoder(w).Encode(out)
}

// Marshal returns the canonical TOML bytes of the settings.
func (s Settings) Marshal() ([]byte, error) {
	var buf bytes.Buffer
	if err := s.WriteTOML(&buf); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func parseGraphicsPreset(s string) (GraphicsPreset, bool) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "high":
		return PresetHigh, true
	case "low":
		return PresetLow, true
	default:
		return PresetHigh, false
	}
}

func parseKeymapProfile(s string) (string, bool) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "grid":
		return "grid", true
	case "classic":
		return "classic", true
	default:
		return "", false
	}
}

// clampVolume resolves one audio group: nil (missing) → default silently; a
// present value is clamped to [0,1], warning on an out-of-range input.
func clampVolume(p *float64, def float64, name string, warns *[]string) float64 {
	if p == nil {
		return def
	}
	v := *p
	if v < 0 {
		*warns = append(*warns, fmt.Sprintf("config: audio.%s %.3f < 0 — clamped to 0", name, v))
		return 0
	}
	if v > 1 {
		*warns = append(*warns, fmt.Sprintf("config: audio.%s %.3f > 1 — clamped to 1", name, v))
		return 1
	}
	return v
}

func floatPtr(f float64) *float64 { return &f }
