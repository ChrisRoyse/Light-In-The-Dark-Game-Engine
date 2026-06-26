package ai

// AI attach/detach plumbing (#281; ai-natives.md public surface; R-EXEC-3).
// The api layer's AttachAI/PauseAI/AIDifficulty hooks wire a computer player's
// strategy into this domain through here: FuncController adapts a "run this
// every AI tick" closure into the Install/continuation shape the domain expects
// (so the api need never touch scheduler ContIDs), and RemovePlayer lets
// AttachAI replace a controller wholesale or DetachAI tear one down — the
// isolation boundary guarantees neither perturbs anything outside the context.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"

// contFuncTick is the single continuation a FuncController registers on its
// context's scheduler. Each AI context has its own scheduler, so the id is
// unique per context (no cross-context collision).
const contFuncTick sched.ContID = 1

// FuncController adapts a per-tick function into an AIController. It registers
// one continuation that calls tick() every AI tick (re-arming itself each
// tick), so a caller that just wants "run this strategy every tick" — e.g. the
// api wrapping a public AIController.Tick — does not deal with the scheduler
// directly. Because the continuation re-arms with After(1), a frozen
// (Disable'd) context simply does not advance its clock and resumes on the
// identical relative schedule when re-enabled (pause = shift, not drop).
type FuncController struct {
	tick func()
}

// NewFuncController wraps tick into an AIController.
func NewFuncController(tick func()) *FuncController { return &FuncController{tick: tick} }

// Install registers the per-tick continuation and arms it for the next tick.
func (f *FuncController) Install(ctx *Context) {
	ctx.Register(contFuncTick, func(_ *sched.Scheduler, st sched.State) {
		f.tick()
		ctx.After(1, contFuncTick, st) // re-arm: run again next tick
	})
	ctx.After(1, contFuncTick, sched.State{}) // first run is next tick
}

// RemovePlayer drops player's context from the domain (detach / replace). The
// context's scheduler and suspensions go with it; because the domain holds no
// hooks into anything outside a context, removal cannot perturb the map-script
// domain or another player. No-op if the player has no context. Returns whether
// a context was removed.
func (d *Domain) RemovePlayer(player int) bool {
	for i, c := range d.ctxs {
		if c.player == player {
			// preserve insertion order of the rest (deterministic tick order)
			d.ctxs = append(d.ctxs[:i], d.ctxs[i+1:]...)
			return true
		}
	}
	return false
}
