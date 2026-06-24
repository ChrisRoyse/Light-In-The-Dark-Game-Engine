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
	"sort"
	"strings"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
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

// collectLuaFilesFS returns every .lua file in fsys, as slash-separated paths
// relative to the fsys root, in lexical order (so registration + load order are
// deterministic across machines and runs).
func collectLuaFilesFS(fsys fs.FS) ([]string, error) {
	var rels []string
	err := fs.WalkDir(fsys, ".", func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".lua") {
			return nil
		}
		rels = append(rels, p) // fs.WalkDir paths are already slash-separated
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
//
// LoadWorld is the dev (directory) path; it is a thin wrapper over LoadWorldFS
// so a packaged world archive loads through the identical code (#205/#209).
func LoadWorld(L *lua.LState, reg *ChunkRegistry, dir string) (*WorldInfo, error) {
	return LoadWorldFS(L, reg, os.DirFS(dir), dir)
}

// LoadWorldFS is the filesystem-source-agnostic world loader: fsys may be a
// directory (os.DirFS) or a verified world archive (worldarchive.FS). label is
// used only for error messages and WorldInfo.Dir (e.g. the dir path or the
// archive filename). The entry point main.lua must sit at the fsys root — for
// an archive whose Lua lives under scripts/, pass fs.Sub(archive.FS(), "scripts").
func LoadWorldFS(L *lua.LState, reg *ChunkRegistry, fsys fs.FS, label string) (*WorldInfo, error) {
	rels, err := collectLuaFilesFS(fsys)
	if err != nil {
		return nil, fmt.Errorf("luabind: load world %q: %w", label, err)
	}
	if len(rels) == 0 {
		return nil, fmt.Errorf("luabind: world %q has no .lua files", label)
	}

	info := &WorldInfo{Dir: label, Entry: WorldEntry}
	entryID := ""
	relToID := make(map[string]string, len(rels)) // rel -> chunk-id, for require (#412)
	for _, rel := range rels {
		src, err := fs.ReadFile(fsys, rel)
		if err != nil {
			return nil, fmt.Errorf("luabind: world %q read %s: %w", label, rel, err)
		}
		// Register compiles AND indexes the chunk's prototypes by pointer identity;
		// a syntax error is returned loudly here, already naming the chunk and the
		// offending line.
		id, err := reg.Register(rel, string(src))
		if err != nil {
			return nil, fmt.Errorf("luabind: world %q: %w", label, err)
		}
		info.Chunks = append(info.Chunks, WorldChunk{Rel: rel, ID: id})
		relToID[rel] = id
		if rel == WorldEntry {
			entryID = id
		}
	}
	if entryID == "" {
		return nil, fmt.Errorf("luabind: world %q has no %s entry point", label, WorldEntry)
	}

	// Install the composition shim so main.lua can pull in sibling chunks (#412):
	// a sandbox-safe require resolving ONLY against this world's compiled chunks
	// (no filesystem, no arbitrary load — the stripped stdlib require stays gone).
	installRequire(L, reg, relToID)

	// Install the save-safe timer verbs (#558) as a registered chunk in
	// reg, so coroutines they spawn carry registry-resolvable protos and
	// survive a mid-match save — same proto-ownership requirement as the
	// world entry below.
	if err := RegisterTimerPrelude(L, reg); err != nil {
		return nil, fmt.Errorf("luabind: world %q timer prelude: %w", label, err)
	}

	// Execute the entry through its REGISTERED prototype (not a fresh L.Load
	// recompile), so every closure the entry creates — Game_Every actions, OnEvent
	// handlers, trigger conditions/actions — has a prototype the registry owns by
	// pointer identity. That is what lets SaveScripts persist a mid-game save
	// (#481/#450): a separately-compiled entry proto belongs to no chunk and
	// SaveScripts fails closed on it. Same fix the require shim applies to siblings.
	entryProto, err := reg.ResolveProto(entryID, "")
	if err != nil {
		return nil, fmt.Errorf("luabind: world %q entry proto: %w", label, err)
	}
	L.Push(L.NewFunctionFromProto(entryProto))
	if err := L.PCall(0, 0, nil); err != nil {
		return nil, fmt.Errorf("luabind: world %q entry run: %w", label, err)
	}
	return info, nil
}

// installRequire adds a host require(name) to L that composes a world from its
// sibling chunks (#412). It resolves name against modules (this world's compiled
// chunks only — no filesystem, no Lua `load`, so R-SEC-1 is preserved), runs each
// chunk AT MOST ONCE into the shared environment, caches its return value, and
// detects cycles. Accepted name forms: the exact rel ("scripts/beacon.lua"), the
// rel without extension ("scripts/beacon"), or Lua dotted form ("scripts.beacon").
func installRequire(L *lua.LState, reg *ChunkRegistry, relToID map[string]string) {
	loaded := map[string]lua.LValue{} // rel -> module value (run-once cache)
	loading := map[string]bool{}      // rel currently on the require stack (cycle guard)

	// require is engine infrastructure (a host Go-function), installed here per
	// world AFTER Register captured the builtin-global baseline — fold it into the
	// baseline so SaveScripts does not mistake it for world data and fail closed on
	// the unpersistable Go value (#481).
	if s := getScheduler(L); s != nil {
		s.markBuiltinGlobal("require")
	}
	L.SetGlobal("require", L.NewFunction(func(L *lua.LState) int {
		name := L.CheckString(1)
		rel, ok := resolveModule(name, relToID)
		if !ok {
			L.RaiseError("require: no module %q in this world (worlds compose only sibling chunks)", name)
			return 0
		}
		if v, ok := loaded[rel]; ok {
			L.Push(v)
			return 1
		}
		if loading[rel] {
			L.RaiseError("require: cyclic dependency through %q", rel)
			return 0
		}
		loading[rel] = true
		// Run the module through its REGISTERED prototype (not a fresh L.Load), so
		// closures it creates are persistable for a mid-game save (#481) — same
		// reason the entry is run from the registry.
		proto, err := reg.ResolveProto(relToID[rel], "")
		if err != nil {
			delete(loading, rel)
			L.RaiseError("require %q: %v", rel, err)
			return 0
		}
		L.Push(L.NewFunctionFromProto(proto))
		if err := L.PCall(0, 1, nil); err != nil {
			delete(loading, rel)
			L.RaiseError("require %q: %v", rel, err)
			return 0
		}
		ret := L.Get(-1)
		L.Pop(1)
		if ret == lua.LNil {
			ret = lua.LTrue // module with no explicit return → true (Lua convention)
		}
		loaded[rel] = ret
		delete(loading, rel)
		L.Push(ret)
		return 1
	}))
}

// resolveModule maps a require name to a registered chunk rel, trying the exact
// name, the name with ".lua", and the Lua dotted form (dots → slashes) with
// ".lua". Returns the matched rel and whether one was found.
func resolveModule(name string, modules map[string]string) (string, bool) {
	candidates := []string{
		name,
		name + ".lua",
		strings.ReplaceAll(name, ".", "/") + ".lua",
	}
	for _, c := range candidates {
		if _, ok := modules[c]; ok {
			return c, true
		}
	}
	return "", false
}

// InstallWorldLoader wires the luabind loader as g's LoadWorld backend (#268),
// closing over L (already sandboxed and Register'd to g) and reg. After this,
// the public verb g.LoadWorld(path) loads worlds through this engine — keeping
// litd/api Lua-agnostic (it never imports luabind; the backend crosses in
// through the api.WorldLoader seam). The api-side WorldInfo is discarded; call
// LoadWorld directly when the caller needs it.
func InstallWorldLoader(g *api.Game, L *lua.LState, reg *ChunkRegistry) {
	g.SetWorldLoader(func(_ *api.Game, path string) error {
		_, err := LoadWorld(L, reg, path)
		return err
	})
}
