package luabind

import (
	"fmt"

	lua "github.com/yuin/gopher-lua"
)

// PRD2 timer surface for Lua authors (#558, 01-timer-wheel/api.md §2).
//
// Unlike the Go-closure Game.After/Every (save-UNSAFE, #557), the Lua
// timer verbs are SAVE-SAFE: they are thin Lua wrappers over the
// cooperative coroutine scheduler (Run/PolledWait, sched.go), and a
// suspended Lua coroutine — together with its upvalues, including the
// handle table that carries the cancel/pause flags — is serialized by
// the gopher-lua deterministic persister (#264/#270). So a Times(0.3,5)
// midway through its run round-trips a save/load and finishes correctly.
//
// This is a pure Lua prelude: it adds NO Go scheduler state and reuses
// the existing save-safe primitives, which is why it cannot reintroduce
// the cross-system ordering regression that moving timers onto the sim
// wheel would (the #269 invariant — Go-closure timers and Lua waits
// share one (wakeTick,seq) order — is untouched).
//
// Handle: every constructor returns a plain Lua table. CancelTimer sets
// h.cancelled (the loop exits at its next check); SetTimerPaused sets
// h.paused (the loop keeps its cadence but skips the callback);
// TimerRemaining reads h.wakeAt against Game_ElapsedTime(). All of that
// is ordinary Lua state and therefore persists.
//
// Determinism: callbacks run on coroutine resumes, which fire in the
// scheduler's (wakeTick,seq) order; sub-tick / non-positive durations
// resume on the next tick (PolledWait's floor), matching the Go timer
// quantization.

const timerPreludeSrc = `
-- After(secs, fn): run fn once after secs of game time. Returns a handle
-- so it can be cancelled before it fires.
function After(secs, fn)
  local h = { wakeAt = Game_ElapsedTime() + secs }
  Run(function()
    PolledWait(secs)
    if not h.cancelled then fn() end
  end)
  return h
end

-- Every(secs, fn): run fn every secs until CancelTimer. fn receives the
-- handle so it can cancel/inspect itself. Paused timers keep their
-- cadence but skip the callback.
function Every(secs, fn)
  local h = {}
  Run(function()
    while not h.cancelled do
      h.wakeAt = Game_ElapsedTime() + secs
      PolledWait(secs)
      if h.cancelled then break end
      if not h.paused then fn(h) end
    end
  end)
  return h
end

-- Times(secs, n, fn): run fn exactly n times, every secs; fn(handle, i)
-- with i = 1..n. A paused tick does not count toward n.
function Times(secs, n, fn)
  local h = {}
  Run(function()
    local i = 0
    while i < n and not h.cancelled do
      h.wakeAt = Game_ElapsedTime() + secs
      PolledWait(secs)
      if h.cancelled then break end
      if not h.paused then
        i = i + 1
        fn(h, i)
      end
    end
  end)
  return h
end

-- EveryOwned(unit, secs, fn): like Every, but auto-cancels when the unit
-- dies (R-TMR-6 — the spawner/respawn pattern).
function EveryOwned(unit, secs, fn)
  local h = {}
  Run(function()
    while not h.cancelled and Unit_Alive(unit) do
      h.wakeAt = Game_ElapsedTime() + secs
      PolledWait(secs)
      if h.cancelled or not Unit_Alive(unit) then break end
      if not h.paused then fn(h) end
    end
  end)
  return h
end

-- CancelTimer(h): stop a timer. Idempotent and nil-safe.
function CancelTimer(h)
  if h then h.cancelled = true end
end

-- SetTimerPaused(h, p): freeze (true) or resume (false) callbacks.
function SetTimerPaused(h, p)
  if h then h.paused = p and true or false end
end

-- TimerRemaining(h): game seconds until the next fire (0 if unknown or
-- already fired/cancelled).
function TimerRemaining(h)
  if not h or not h.wakeAt then return 0 end
  local r = h.wakeAt - Game_ElapsedTime()
  if r < 0 then return 0 end
  return r
end
`

// timerPreludeChunkName is the reserved chunk name the prelude registers
// under. It is engine code, content-addressed like any chunk, so its id
// is stable across runs and a saved coroutine spawned by a timer verb
// resolves its prototype here on load.
const timerPreludeChunkName = "@litd/timer-prelude"

// RegisterTimerPrelude compiles the timer-verb prelude into reg and runs
// it on L through the REGISTERED prototype, so coroutines spawned by
// After/Every/Times/EveryOwned carry registry-resolvable protos and
// therefore round-trip a save/load (the point of the save-safe Lua
// surface, #558). Run after Register (which installs Run/PolledWait/
// Unit_Alive/Game_ElapsedTime); LoadWorld calls it automatically, tests
// call it directly. Idempotent: same source → same id; re-running merely
// re-binds the globals to the same protos.
func RegisterTimerPrelude(L *lua.LState, reg *ChunkRegistry) error {
	cid, err := reg.Register(timerPreludeChunkName, timerPreludeSrc)
	if err != nil {
		return err
	}
	proto, err := reg.ResolveProto(cid, "")
	if err != nil {
		return err
	}
	L.Push(L.NewFunctionFromProto(proto))
	if err := L.PCall(0, 0, nil); err != nil {
		return fmt.Errorf("luabind: timer prelude: %w", err)
	}
	// Fold the verb names into the builtin baseline so they are not saved
	// as world data — they are code, re-installed on load.
	if s := getScheduler(L); s != nil {
		for _, name := range []string{"After", "Every", "Times", "EveryOwned", "CancelTimer", "SetTimerPaused", "TimerRemaining"} {
			s.markBuiltinGlobal(name)
		}
	}
	return nil
}
