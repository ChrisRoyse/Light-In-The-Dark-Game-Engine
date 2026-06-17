// Package litd — runtime world loading (#268).
package litd

import "errors"

// WorldLoader loads and runs the world directory at path into g (compiling its
// Lua entry point + chunks and executing them against the bound game). It is
// supplied by the host, not litd/api: the api package never imports the script
// runtime (litd/luabind imports api, not the reverse), so the loader backend
// crosses in through this seam. litd/luabind.InstallWorldLoader wires it.
type WorldLoader func(g *Game, path string) error

// SetWorldLoader installs the world-loading backend. Setup-only — the host
// wires the luabind loader here once, before any LoadWorld call. Passing nil
// clears it (LoadWorld then fails closed).
func (g *Game) SetWorldLoader(fn WorldLoader) { g.worldLoader = fn }

// HasWorldLoader reports whether a loader backend is installed.
func (g *Game) HasWorldLoader() bool { return g.worldLoader != nil }

// LoadWorld loads and runs the world directory at path. It is a setup verb
// (R-API-5: returns error, never panics): a broken world — or no loader backend
// installed — is a loud error here, before the match runs, never a silent no-op
// and never a mid-match failure.
func (g *Game) LoadWorld(path string) error {
	if g.worldLoader == nil {
		return errors.New("litd: LoadWorld: no world loader installed (call SetWorldLoader; see litd/luabind.InstallWorldLoader)")
	}
	return g.worldLoader(g, path)
}
