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
// coroutine saver uses (serializeRegisters / graphDecoder), so captured upvalues
// round-trip (#436) — INCLUDING a closed cell shared between two handlers, now
// that the persister interns closed upvalue cells by identity (#437).
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

// threadReachRoots returns the live values reachable from a suspended coroutine
// (its register stack + each frame's executing function) — the same object set
// SaveThread serializes, used for cross-thread sharing detection. Nil register
// slots reach nothing and are skipped.
func threadReachRoots(th *lua.LState) []lua.LValue {
	v := th.LitdSnapshot()
	roots := make([]lua.LValue, 0, len(v.Stack)+len(v.Frames))
	for _, sv := range v.Stack {
		if sv != nil {
			roots = append(roots, sv)
		}
	}
	for _, f := range v.Frames {
		if f.Fn != nil {
			roots = append(roots, f.Fn)
		}
	}
	return roots
}

// DetectCrossThreadSharing fails closed (§2.4) if a mutable Lua object — a closed
// upvalue cell or a table — is reachable from more than one independently
// serialized graph: two coroutines, or a coroutine and the OnEvent handler set.
// Such an object is serialized once per graph and reconstructed as independent
// copies, so a mutation through one would be invisible to the other after a
// restore — a SILENT determinism divergence (#440). Sharing WITHIN one graph (two
// handlers, or closures inside one coroutine) round-trips fine and is allowed.
//
// Call it before writing a save. It is conservative: it refuses rather than emit
// a container that would desync. The proper fix (one save-wide intern pool) is
// tracked in #440; until then this turns the silent hole into a loud refusal.
func DetectCrossThreadSharing(L *lua.LState, reg *ChunkRegistry) error {
	s := getScheduler(L)
	if s == nil {
		return fmt.Errorf("luabind: DetectCrossThreadSharing: no scheduler bound to this LState")
	}
	handles := GameHandles{G: s.g}
	cellGroup := map[*lua.Upvalue]string{}
	tableGroup := map[*lua.LTable]string{}
	check := func(group string, owner *lua.LState, roots []lua.LValue) error {
		cells, tables, err := reachableMutables(reg, owner, roots, handles)
		if err != nil {
			return err // an unpersistable value the real save would also reject
		}
		for c := range cells {
			if prev, ok := cellGroup[c]; ok && prev != group {
				return fmt.Errorf("luabind: %s and %s share a mutable upvalue cell — shared mutable state across independently-saved graphs would diverge on restore (#440)", prev, group)
			}
			cellGroup[c] = group
		}
		for t := range tables {
			if prev, ok := tableGroup[t]; ok && prev != group {
				return fmt.Errorf("luabind: %s and %s share a mutable table — shared mutable state across independently-saved graphs would diverge on restore (#440)", prev, group)
			}
			tableGroup[t] = group
		}
		return nil
	}
	if len(s.eventHandlers) > 0 {
		roots := make([]lua.LValue, len(s.eventHandlers))
		for i, h := range s.eventHandlers {
			roots[i] = h.fn
		}
		if err := check("the event-handler set", L, roots); err != nil {
			return err
		}
	}
	for i := range s.threads {
		e := &s.threads[i]
		if !e.alive || e.co == nil {
			continue
		}
		if err := check(fmt.Sprintf("coroutine slot %d", i), e.co, threadReachRoots(e.co)); err != nil {
			return err
		}
	}
	return nil
}
