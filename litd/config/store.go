package config

// store.go is the disk layer for user settings (#311). The byte-core in
// settings.go stays filesystem-free; this file owns the platform config path and
// the read/write, mirroring litd/obs/crash.go's use of os.UserConfigDir()/litd.
// Both Load and Save are fail-closed: a missing file loads defaults (no error), a
// corrupt file loads defaults plus the parse warning, and the write is atomic
// (temp + rename) so a crash mid-write never leaves a half-written config.

import (
	"os"
	"path/filepath"
)

// fileName is the settings file under Dir().
const fileName = "settings.toml"

// Dir is the per-user config directory for litd (os.UserConfigDir()/litd) — the
// same root crash reports use. It is not created here; SaveTo creates it on demand.
func Dir() (string, error) {
	base, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(base, "litd"), nil
}

// Path is the absolute settings-file path (Dir()/settings.toml).
func Path() (string, error) {
	d, err := Dir()
	if err != nil {
		return "", err
	}
	return filepath.Join(d, fileName), nil
}

// LoadFrom reads and parses the settings file at path. Fail-closed: a missing file
// yields defaults plus a note and no error (first run is not a failure); a present
// but corrupt file yields defaults plus the parse warning and no error. Only a
// genuine I/O error on an existing file (permissions, a directory in the way)
// returns a non-nil error — the caller can still launch on the returned defaults.
func LoadFrom(path string) (Settings, []string, error) {
	blob, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return DefaultSettings(), []string{"config: no settings file — using defaults"}, nil
		}
		return DefaultSettings(), []string{"config: unreadable — using defaults"}, err
	}
	s, warns := LoadSettings(blob)
	return s, warns, nil
}

// SaveTo writes settings as canonical TOML to path, creating the parent directory
// on demand. The write is atomic: it lands in a sibling temp file that is renamed
// over path, so a reader never observes a partial config and a crash mid-write
// leaves the previous file intact.
func SaveTo(path string, s Settings) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	blob, err := s.Marshal()
	if err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, blob, 0o644); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		os.Remove(tmp) // best effort: don't leak the temp on a failed rename
		return err
	}
	return nil
}

// Load reads the user settings from the platform config path (Path()).
func Load() (Settings, []string, error) {
	p, err := Path()
	if err != nil {
		return DefaultSettings(), []string{"config: no config dir — using defaults"}, err
	}
	return LoadFrom(p)
}

// Save writes the user settings to the platform config path (Path()).
func Save(s Settings) error {
	p, err := Path()
	if err != nil {
		return err
	}
	return SaveTo(p, s)
}
