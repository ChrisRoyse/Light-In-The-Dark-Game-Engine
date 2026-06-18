package litd

// Script-thread scheduling seam (#270, execution-model.md §2.1 S-5).
//
// A script VM (the Lua host, litd/luabind) runs cooperative coroutines that
// suspend at PolledWait and resume on a later tick — the same cooperative model
// as the Go Thread layer (thread.go), but with one decisive difference: a
// suspended Lua coroutine's state lives in the VM and IS serializable (the #264
// persister writes it), whereas a parked Go thread's stack is opaque and a save
// must fail closed on it. So script coroutines can survive a mid-wait save/load.
//
// For that to work the WAKE RECORD must be descriptive — a (wakeTick, seq,
// ContID, value-typed State) entry on the stackless scheduler, never a Go
// closure capturing the coroutine (which cannot be written to a save file).
// This seam lets the VM own that: it registers one resume continuation under a
// script-owned, low-numbered ContID (api timers/threads sit at >=1<<30 and can
// never collide) and posts wake records carrying its coroutine handle by value.
//
// The api stays Lua-agnostic: it never imports the VM. It only forwards a
// value-typed (a, b) payload — the VM packs its own (slot, generation) into it.

import (
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// maxScriptContID is the exclusive upper bound on a script-owned ContID. api's
// own continuations (contGoTimer 1<<30, contThreadResume 1<<30+1) sit at or
// above this, so a script id below it can never collide with them.
const maxScriptContID = uint32(contGoTimer) // 1<<30

// RegisterScriptCont registers fn as the wake continuation for the script
// continuation id, on this game's scheduler. When a record posted with
// AfterScript(id, …) reaches its wake tick, fn runs synchronously on the sim
// goroutine in phase 2, receiving the value-typed (a, b) the record carried.
//
// Call once per id at VM setup. The continuation registry is code, not state:
// on a save load the scheduler rebuilds it from these registrations and rejects
// any record naming an unregistered id, so the VM must re-register before load.
// id must be < 1<<30 (the script-owned range); a higher id panics (it would
// shadow the api's own timer/thread continuations). A nil game or fn is a no-op.
func (g *Game) RegisterScriptCont(id uint32, fn func(a, b int64)) {
	if g == nil || fn == nil {
		return
	}
	if id >= maxScriptContID {
		panic("litd: script ContID must be < 1<<30 (the api reserves 1<<30+ for timers/threads)")
	}
	g.w.Sched.Register(sched.ContID(id), func(_ *sched.Scheduler, st sched.State) {
		fn(st[0], st[1])
	})
}

// AfterScript posts a descriptive wake record on script continuation id, firing
// after d of game time, carrying the value-typed (a, b). The delay quantizes to
// whole ticks exactly as timers do (durationToTicks: up to whole ticks, one-tick
// floor) so a script wait and a timer of the same duration land on the same
// tick. The record serializes with the scheduler blob and, on load, re-resolves
// against the re-registered continuation (RegisterScriptCont). A nil game is a
// no-op. id must be < 1<<30.
func (g *Game) AfterScript(d time.Duration, id uint32, a, b int64) {
	if g == nil {
		return
	}
	if id >= maxScriptContID {
		panic("litd: script ContID must be < 1<<30 (the api reserves 1<<30+ for timers/threads)")
	}
	g.w.Sched.After(durationToTicks(d), sched.ContID(id), sched.State{a, b})
}
