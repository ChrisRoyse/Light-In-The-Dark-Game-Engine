package luabind

// loader.go is the runtime world loader (#268): it makes a world directory
// (a Lua entry point + supporting chunks) loadable WITHOUT a Go toolchain or an
// engine rebuild. It compiles every chunk into the persister's ChunkRegistry
// (#264 content-addressed ids), then executes the world entry point on a
// caller-supplied LState that is already sandboxed (#266) and bound to a game
// (Register, #267).
//
// LoadWorld is a SETUP step (R-API-5 split): a missing entry, a chunk that will
// not compile, or a fault while running the entry is returned as a loud error
// HERE — a broken world fails at load, never mid-match.
//
// NOTE: the api-side `g.LoadWorld(path)` verb (#268 deliverable) is not here:
// litd/api cannot import litd/luabind (luabind imports api), so the public verb
// needs an injection seam, tracked on #268. This is the luabind-side engine.

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"

	lua "github.com/yuin/gopher-lua"
)

// WorldEntry is the fixed entry-point filename a world directory must contain.
const WorldEntry = "main.lua"

// WorldChunk is one loaded source file: its slash-path relative to the world
// directory and the content-addressed chunk-id it registered under.
type WorldChunk struct {
	Rel string
	ID  string
}

// WorldInfo describes a loaded world: the directory, the entry chunk executed,
// and every chunk registered (sorted by Rel — deterministic across loads).
type WorldInfo struct {
	Dir    string
	Entry  string
	Chunks []WorldChunk
}

// collectLuaFiles returns every .lua file under dir, as slash-separated paths
// relative to dir, in lexical order (so registration + load order are
// deterministic across machines and runs).
func collectLuaFiles(dir string) ([]string, error) {
	var rels []string
	err := filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".lua") {
			return nil
		}
		rel, err := filepath.Rel(dir, path)
		if err != nil {
			return err
		}
		rels = append(rels, filepath.ToSlash(rel))
		return nil
	})
	if err != nil {
		return nil, err
	}
	sort.Strings(rels)
	return rels, nil
}

// LoadWorld compiles every .lua file under dir into reg, then runs the world
// entry point (main.lua) on L. L must already be sandboxed and game-bound. It
// returns a WorldInfo describing what was loaded, or a loud error on the first
// failure (no partial-load: a world either loads whole or not at all).
func LoadWorld(L *lua.LState, reg *ChunkRegistry, dir string) (*WorldInfo, error) {
	rels, err := collectLuaFiles(dir)
	if err != nil {
		return nil, fmt.Errorf("luabind: load world %q: %w", dir, err)
	}
	if len(rels) == 0 {
		return nil, fmt.Errorf("luabind: world %q has no .lua files", dir)
	}

	info := &WorldInfo{Dir: dir, Entry: WorldEntry}
	haveEntry := false
	for _, rel := range rels {
		src, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(rel)))
		if err != nil {
			return nil, fmt.Errorf("luabind: world %q read %s: %w", dir, rel, err)
		}
		// Register compiles the chunk; a syntax error is returned loudly here,
		// already naming the chunk and the offending line.
		id, err := reg.Register(rel, string(src))
		if err != nil {
			return nil, fmt.Errorf("luabind: world %q: %w", dir, err)
		}
		info.Chunks = append(info.Chunks, WorldChunk{Rel: rel, ID: id})
		if rel == WorldEntry {
			haveEntry = true
		}
	}
	if !haveEntry {
		return nil, fmt.Errorf("luabind: world %q has no %s entry point", dir, WorldEntry)
	}

	// Execute the entry on L. L.Load is the Go-side compile API (NOT the Lua
	// `load` global, which the sandbox strips); the "@" chunkname makes runtime
	// errors carry main.lua:line.
	entrySrc, err := os.ReadFile(filepath.Join(dir, WorldEntry))
	if err != nil {
		return nil, fmt.Errorf("luabind: world %q read entry: %w", dir, err)
	}
	fn, err := L.Load(strings.NewReader(string(entrySrc)), "@"+WorldEntry)
	if err != nil {
		return nil, fmt.Errorf("luabind: world %q entry compile: %w", dir, err)
	}
	L.Push(fn)
	if err := L.PCall(0, 0, nil); err != nil {
		return nil, fmt.Errorf("luabind: world %q entry run: %w", dir, err)
	}
	return info, nil
}
