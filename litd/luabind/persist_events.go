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
// chunk is a loud error at save time — never a silent drop. As of #446 the
// handler CLOSURES are serialized by SaveScripts into the one shared scheduler
// pool (so a mutable object shared between a handler and a coroutine round-trips
// as a single object); this section now carries only each handler's EventKind in
// registration order. Those kinds are replayed on a fresh game BEFORE the sim
// restore so the per-kind HandlerIDs re-allocate and LoadState's subscriptions
// resolve; the closures are then bound back by slot index post-sim (LoadScripts).
package luabind

import (
	"fmt"
	"io"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// eventSaveMagic tags the OnEvent-handler save section and pins its format
// version. A mismatched magic on load is a loud refusal, never a partial restore.
// v2 (#436) serialized each handler as a full closure via the shared-graph
// persister. v3 (#446) moves the closures into the shared scheduler pool
// (SaveScripts) and reduces this section to the handler kinds + order, so a
// coroutine↔handler shared mutable round-trips instead of being refused.
const eventSaveMagic = "LITDEVT\x03"

// scriptEventReg records one OnEvent(kind, fn) registration in the order it was
// made. The api side keeps only the Go trampoline closure (the originating Lua
// function is unrecoverable from it), so the scheduler must remember the Lua
// function itself for the handler table to be serializable.
type scriptEventReg struct {
	kind api.EventKind
	fn   *lua.LFunction
}

// subscribeHandlerSlot registers the OnEvent subscription for handler slot idx.
// The trampoline reads the Lua fn from s.eventHandlers[idx] AT DISPATCH TIME, not
// at registration, so the same subscription works whether the fn is bound now
// (live OnEvent) or filled in later (#446 restore: the kinds register pre-sim to
// re-allocate HandlerIDs, while the closures decode post-sim from the shared
// pool). A still-nil slot (between pre-sim registration and post-sim binding) is a
// no-op — no event fires during the sim restore window.
func subscribeHandlerSlot(L *lua.LState, g *api.Game, s *scriptScheduler, kind api.EventKind, idx int) api.Subscription {
	return g.OnEvent(kind, func(ev api.Event) {
		if idx < len(s.eventHandlers) {
			if fn := s.eventHandlers[idx].fn; fn != nil {
				callEventHandler(L, fn, ev)
			}
		}
	})
}

// registerScriptHandler binds fn as an OnEvent handler for kind on g and records
// the registration on s so it can be persisted. Used by the OnEvent Lua binding;
// the restore path appends a nil-fn record and calls subscribeHandlerSlot directly
// (the fn arrives post-sim).
func registerScriptHandler(L *lua.LState, g *api.Game, s *scriptScheduler, kind api.EventKind, fn *lua.LFunction) api.Subscription {
	idx := len(s.eventHandlers)
	s.eventHandlers = append(s.eventHandlers, scriptEventReg{kind: kind, fn: fn})
	return subscribeHandlerSlot(L, g, s, kind, idx)
}

// SaveEventHandlers serializes the OnEvent handler table's SHAPE on L to w: the
// public EventKind of each handler in registration order. The handler closures
// themselves are written by SaveScripts into the shared scheduler pool (#446) —
// keeping a coroutine↔handler shared object as a single interned object — so this
// section is closure-free. Pair it with SaveScripts and api.Game.SaveState at the
// same tick boundary; a handler whose closure is unpersistable is a loud error
// from SaveScripts (the pool encoder), preserving the fail-closed posture.
func SaveEventHandlers(L *lua.LState, reg *ChunkRegistry, w io.Writer) error {
	s := getScheduler(L)
	if s == nil {
		return fmt.Errorf("luabind: SaveEventHandlers: no scheduler bound to this LState")
	}
	_ = reg // closures are encoded by SaveScripts now; kept for signature symmetry
	bw := &errWriter{w: w}
	bw.writeRaw([]byte(eventSaveMagic))
	bw.u32(uint32(len(s.eventHandlers)))
	for _, h := range s.eventHandlers {
		bw.u16(uint16(h.kind))
	}
	return bw.err
}

// RestoreEventHandlers replays the OnEvent handler kinds from r on a fresh game,
// re-registering one subscription per handler (in order) so the per-kind
// HandlerIDs re-allocate to match the sim save — see the package doc. It MUST run
// BEFORE g.LoadState. The handler CLOSURES are bound later, by slot index, when
// LoadScripts decodes the shared pool post-sim (#446); until then each slot's fn
// is nil and its trampoline is a no-op (no event fires mid-restore). A nil
// scheduler, bad magic, or truncated data is a loud error and no partial table is
// left.
func RestoreEventHandlers(L *lua.LState, reg *ChunkRegistry, g *api.Game, r io.Reader) error {
	s := getScheduler(L)
	if s == nil {
		return fmt.Errorf("luabind: RestoreEventHandlers: no scheduler bound to this LState")
	}
	_ = reg // closures resolve later, in LoadScripts, against the shared pool
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
	kinds := make([]api.EventKind, n)
	for i := uint32(0); i < n; i++ {
		kinds[i] = api.EventKind(br.u16())
		if br.err != nil {
			return fmt.Errorf("luabind: RestoreEventHandlers: handler %d kind: %w", i, br.err)
		}
	}

	// Two restore flows reach here (#481):
	//   * register-only: the entry chunk is registered but NOT re-run, so the
	//     scheduler has no handlers yet — replay the kinds to re-allocate the
	//     per-kind HandlerIDs before the sim restore (the original #433 path).
	//   * entry re-run: the world loader re-ran main.lua (required to rebuild the
	//     Game_Every periodic slots, #464), so it ALREADY re-subscribed every
	//     OnEvent in the same order. Re-subscribing again would double the
	//     subscriptions (LoadState then sees more HandlerIDs than the save).
	// Detect the re-run case by an already-matching handler table and treat this
	// as a validation no-op; otherwise do the from-scratch replay.
	if rerunMatches(s.eventHandlers, kinds) {
		return br.err
	}
	s.eventHandlers = nil
	for i, kind := range kinds {
		s.eventHandlers = append(s.eventHandlers, scriptEventReg{kind: kind, fn: nil})
		subscribeHandlerSlot(L, g, s, kind, i)
	}
	return br.err
}

// rerunMatches reports whether the scheduler's live handler table already equals
// the saved kinds in count and order — the signature of an entry re-run having
// re-subscribed every OnEvent before the restore. In that case RestoreEventHandlers
// must NOT re-subscribe (it would double the sim subscriptions).
func rerunMatches(live []scriptEventReg, kinds []api.EventKind) bool {
	if len(live) != len(kinds) {
		return false
	}
	for i := range kinds {
		if live[i].kind != kinds[i] {
			return false
		}
	}
	return len(kinds) > 0
}
