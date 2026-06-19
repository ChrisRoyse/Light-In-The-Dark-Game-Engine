// persist_events.go is the save/load half of the OnEvent handler table (#433):
// it serializes every OnEvent(kind, fn) registration so a mid-game save that
// includes event handlers can be restored. It is the sibling of save.go (which
// persists the suspended-coroutine table) and persist.go (chunk registry).
//
// Why a separate section, restored BEFORE the sim (savegame ordering): the sim
// save records each kind's subscription as kind -> sim HandlerID, and
// api.Game.LoadState fails closed if a restored subscription names a HandlerID
// that is not currently registered (sim/save.go: "subscription ... references
// unregistered HandlerID"). The api allocates one HandlerID per kind on first
// OnEvent for that kind (api.ensureKind), deterministically from apiHandlerBase.
// So replaying the saved registrations in their ORIGINAL ORDER on a fresh game
// re-allocates the SAME HandlerIDs, and LoadState's subscriptions then resolve.
// The reserved WaitForEvent dispatcher (#413, fixed HandlerID 1) already survives
// this way; this extends the same guarantee to script-registered handlers.
//
// Fail-closed (§2.4): a handler that is a Go function or belongs to no registered
// chunk is a loud error from SaveEventHandlers — never a silent drop. Handler
// closures are serialized through the same shared-graph value persister the
// coroutine saver uses (serializeRegisters / graphDecoder), so a handler's OWN
// captured upvalues round-trip (#436). A closed upvalue CELL shared between two
// handlers is NOT preserved (the persister interns tables/funcs/userdata by
// identity but not upvalue cells — #437), and restoring it as two independent
// cells would silently diverge the run; SaveEventHandlers therefore REFUSES a
// handler set that shares a cell, rather than break determinism.
package luabind

import (
	"encoding/json"
	"fmt"
	"io"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// eventSaveMagic tags the OnEvent-handler save section and pins its format
// version. A mismatched magic on load is a loud refusal, never a partial restore.
// v2 (#436) serializes each handler as a full closure (proto ref + upvalues) via
// the shared-graph persister, replacing v1's proto-reference-only encoding.
const eventSaveMagic = "LITDEVT\x02"

// scriptEventReg records one OnEvent(kind, fn) registration in the order it was
// made. The api side keeps only the Go trampoline closure (the originating Lua
// function is unrecoverable from it), so the scheduler must remember the Lua
// function itself for the handler table to be serializable.
type scriptEventReg struct {
	kind api.EventKind
	fn   *lua.LFunction
}

// registerScriptHandler binds fn as an OnEvent handler for kind on g and records
// the registration on s so it can be persisted. Shared by the OnEvent Lua
// binding and RestoreEventHandlers so both build the EXACT same trampoline.
func registerScriptHandler(L *lua.LState, g *api.Game, s *scriptScheduler, kind api.EventKind, fn *lua.LFunction) api.Subscription {
	sub := g.OnEvent(kind, func(ev api.Event) {
		callEventHandler(L, fn, ev)
	})
	s.eventHandlers = append(s.eventHandlers, scriptEventReg{kind: kind, fn: fn})
	return sub
}

// SaveEventHandlers serializes the OnEvent handler table on L to w: the public
// EventKind of each handler (in registration order) followed by one shared-graph
// blob holding every handler closure — proto references (chunk id + proto path,
// never bytecode) plus captured upvalues, with shared cells interned once. Pair
// it with SaveScripts and api.Game.SaveState at the same tick boundary.
func SaveEventHandlers(L *lua.LState, reg *ChunkRegistry, w io.Writer) error {
	s := getScheduler(L)
	if s == nil {
		return fmt.Errorf("luabind: SaveEventHandlers: no scheduler bound to this LState")
	}
	bw := &errWriter{w: w}
	bw.writeRaw([]byte(eventSaveMagic))
	bw.u32(uint32(len(s.eventHandlers)))
	if len(s.eventHandlers) == 0 {
		return bw.err
	}
	// A closed upvalue CELL shared by two handlers cannot round-trip (the persister
	// does not intern upvalue cells — #437); restoring it as two cells would
	// silently diverge the run. Detect sharing by cell identity and fail closed.
	cellOwner := map[*lua.Upvalue]int{}
	for i := range s.eventHandlers {
		fn := s.eventHandlers[i].fn
		if fn == nil {
			continue // serializeRegisters reports the bad value below
		}
		for _, uv := range fn.Upvalues {
			if uv == nil {
				continue
			}
			if prev, ok := cellOwner[uv]; ok && prev != i {
				return fmt.Errorf("luabind: SaveEventHandlers: handlers %d and %d share an upvalue cell — shared mutable upvalue cells are not yet preserved across save/load and would diverge the run (#437)", prev, i)
			}
			cellOwner[uv] = i
		}
	}

	fns := make([]lua.LValue, len(s.eventHandlers))
	for i, h := range s.eventHandlers {
		bw.u16(uint16(h.kind))
		fns[i] = h.fn
	}
	// One shared graph for all handlers. serializeRegisters fails closed on a Go
	// function or a function from no registered chunk.
	blob, err := serializeRegisters(reg, L, fns, GameHandles{G: s.g})
	if err != nil {
		return fmt.Errorf("luabind: SaveEventHandlers: %w", err)
	}
	bw.u32(uint32(len(blob)))
	bw.writeRaw(blob)
	return bw.err
}

// RestoreEventHandlers rebuilds the OnEvent handler table from r against reg and
// re-binds each handler to g. It MUST run on a fresh game BEFORE g.LoadState —
// see the package doc for why registration-order replay re-allocates matching
// per-kind HandlerIDs. A nil scheduler, bad magic, truncated data, an
// unresolvable proto, or a root that is not a function is a loud error and no
// partial table is left.
func RestoreEventHandlers(L *lua.LState, reg *ChunkRegistry, g *api.Game, r io.Reader) error {
	s := getScheduler(L)
	if s == nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: no scheduler bound to this LState")
	}
	br := &errReader{r: r}
	magic := br.readRaw(len(eventSaveMagic))
	if br.err != nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: %w", br.err)
	}
	if string(magic) != eventSaveMagic {
		return fmt.Errorf("luabind: RestoreEventHandlers: bad magic %q", magic)
	}
	n := br.u32()
	if br.err != nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: %w", br.err)
	}
	// Rebuild from scratch; a re-save after restore must reproduce this table.
	s.eventHandlers = nil
	if n == 0 {
		return nil
	}
	kinds := make([]api.EventKind, n)
	for i := range kinds {
		kinds[i] = api.EventKind(br.u16())
	}
	blobLen := br.u32()
	blob := br.readRaw(int(blobLen))
	if br.err != nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: %w", br.err)
	}
	var vb valuesBlob
	if err := json.Unmarshal(blob, &vb); err != nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: malformed closure blob: %w", err)
	}
	d, err := newGraphDecoder(L, reg, &vb, GameHandles{G: s.g})
	if err != nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: %w", err)
	}
	roots, err := d.roots()
	if err != nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: %w", err)
	}
	// Wire closed upvalues (and any open ones, against L) AFTER the closures and
	// tables exist, so shared cells coincide.
	if err := d.wireUpvalues(L); err != nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: %w", err)
	}
	for i, root := range roots {
		fn, ok := root.(*lua.LFunction)
		if !ok {
			return fmt.Errorf("luabind: RestoreEventHandlers: handler %d decoded to %s, not a function", i, root.Type())
		}
		registerScriptHandler(L, g, s, kinds[i], fn)
	}
	return nil
}
