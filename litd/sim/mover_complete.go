package sim

// Mover completion policy (#589). When a mover's motion finishes — range
// exhausted, goal reached, spline end, custom done, or pierce spent —
// moverComplete dispatches on DoneMode:
//
//	Expire   free the slot.
//	Loop     re-arm and keep flying (no free): reset spline param + range,
//	         clear the pierce/hit ring for a fresh pass.
//	Detonate deliver the payload to everything in Radius at the final
//	         position (AoE), then free.
//	Cont     schedule the OnDone continuation (CState as its [4]int64
//	         state) for next tick, then free.
//
// Two cross-cutting policies: MoverConsume kills the Target entity on free
// (a spent projectile body), and owner death auto-cancels a mover
// (CancelOwnedBy from phaseCleanup, R-MOV-10).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"

func (w *World) moverComplete(r int32) {
	ms := w.Movers
	switch MoverDoneMode(ms.DoneMode[r]) {
	case MoverDoneLoop:
		ms.RangeLeft[r] = ms.Range0[r] // re-arm linear range
		ms.WpParam[r] = 0              // restart spline param
		ms.HitN[r] = 0                 // fresh pierce budget per lap
		return                         // stays live — no free
	case MoverDoneDetonate:
		w.moverDetonate(r)
	case MoverDoneCont:
		if ms.OnDone[r] != 0 {
			w.AfterMS(0, sched.ContID(ms.OnDone[r]), sched.State(ms.CState[r]))
		}
	}
	if ms.Flags[r]&MoverConsume != 0 {
		w.KillUnit(ms.Target[r]) // consume the projectile body
	}
	w.moverFree(r)
}

// moverFree releases the slot, bumping the generation so outstanding
// handles go stale.
func (w *World) moverFree(r int32) {
	ms := w.Movers
	ms.live[r] = false
	ms.Gen[r]++
	ms.free = append(ms.free, r)
	ms.count--
}

// moverDetonate delivers the payload to every masked unit within Radius of
// the mover's final position (AoE), ignoring the already-hit ring, then
// raises the impact event. Reuses the zero-alloc broad-phase scratch.
func (w *World) moverDetonate(r int32) {
	ms := w.Movers
	tr := w.Transforms.Row(ms.Target[r])
	if tr == -1 {
		return
	}
	at := w.Transforms.Pos[tr]
	team := w.moverSourceTeam(r)
	w.moverHitScratch = w.AppendUnitsInRange(w.moverHitScratch[:0], at, ms.Radius[r])
	for _, u := range w.moverHitScratch {
		if u == ms.Owner[r] || u == ms.Target[r] {
			continue
		}
		if !w.missileHitAllowed(u, team, ms.HitMask[r]) {
			continue
		}
		w.moverDeliver(r, u, at)
	}
	w.Emit(Event{Kind: EvMissileImpact, Src: ms.Target[r]})
}
