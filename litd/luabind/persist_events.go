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
// Fail-closed (§2.4): a handler that is a Go function, belongs to no registered
// chunk, or captures upvalues is a loud error from SaveEventHandlers — never a
// silent drop. Upvalue-capturing handlers are not yet supported (a proto
// reference alone cannot rebind captured cells); tracked as a follow-up.
package luabind

import (
	"fmt"
	"io"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// eventSaveMagic tags the OnEvent-handler save section and pins its format
// version. A mismatched magic on load is a loud refusal, never a partial restore.
const eventSaveMagic = "LITDEVT\x01"

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

// SaveEventHandlers serializes the OnEvent handler table on L to w: for each
// handler, in registration order, its public EventKind and a content-addressed
// reference (chunk id + proto path, never bytecode) to its Lua function. Pair it
// with SaveScripts and api.Game.SaveState at the same tick boundary.
func SaveEventHandlers(L *lua.LState, reg *ChunkRegistry, w io.Writer) error {
	s := getScheduler(L)
	if s == nil {
		return fmt.Errorf("luabind: SaveEventHandlers: no scheduler bound to this LState")
	}
	bw := &errWriter{w: w}
	bw.writeRaw([]byte(eventSaveMagic))
	bw.u32(uint32(len(s.eventHandlers)))
	for i, h := range s.eventHandlers {
		fn := h.fn
		if fn == nil || fn.IsG || fn.Proto == nil {
			return fmt.Errorf("luabind: SaveEventHandlers: handler %d (kind %d) is not a Lua function — cannot persist", i, h.kind)
		}
		if len(fn.Upvalues) > 0 {
			return fmt.Errorf("luabind: SaveEventHandlers: handler %d (kind %d) captures %d upvalue(s) — not yet save-serializable (see #433 follow-up)", i, h.kind, len(fn.Upvalues))
		}
		chunkID, protoPath, err := reg.PathOf(fn.Proto)
		if err != nil {
			return fmt.Errorf("luabind: SaveEventHandlers: handler %d (kind %d): %w", i, h.kind, err)
		}
		bw.u16(uint16(h.kind))
		writeStr(bw, chunkID)
		writeStr(bw, protoPath)
	}
	return bw.err
}

// RestoreEventHandlers rebuilds the OnEvent handler table from r against reg and
// re-binds each handler to g. It MUST run on a fresh game BEFORE g.LoadState —
// see the package doc for why registration-order replay re-allocates matching
// per-kind HandlerIDs. A nil scheduler, bad magic, truncated data, or
// unresolvable proto is a loud error and no partial table is left.
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
	for i := uint32(0); i < n; i++ {
		kind := api.EventKind(br.u16())
		chunkID := readStr(br)
		protoPath := readStr(br)
		if br.err != nil {
			return fmt.Errorf("luabind: RestoreEventHandlers: handler %d: %w", i, br.err)
		}
		proto, err := reg.ResolveProto(chunkID, protoPath)
		if err != nil {
			return fmt.Errorf("luabind: RestoreEventHandlers: handler %d (kind %d): %w", i, kind, err)
		}
		registerScriptHandler(L, g, s, kind, L.NewFunctionFromProto(proto))
	}
	return nil
}

// writeStr frames a string as a u32 length prefix followed by its bytes.
func writeStr(bw *errWriter, s string) {
	bw.u32(uint32(len(s)))
	bw.writeRaw([]byte(s))
}

// readStr reads a u32-length-prefixed string written by writeStr.
func readStr(br *errReader) string {
	n := br.u32()
	if br.err != nil {
		return ""
	}
	return string(br.readRaw(int(n)))
}
