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
	case MoverDoneImpact:
		w.moverImpact(r)
	case MoverDoneCont:
		if ms.OnDone[r] != 0 {
			w.AfterMS(0, sched.ContID(ms.OnDone[r]), sched.State(ms.CState[r]))
		}
	case MoverDoneExpire:
		w.projectileExpire(r) // missile-expiry signal for a projectile body (#590)
	}
	if ms.Flags[r]&MoverConsume != 0 {
		w.KillUnit(ms.Target[r]) // consume the projectile body
	}
	w.moverFree(r)
}

// projectileExpire fires the missile-expiry presentation cue + EvMissileExpired
// when a projectile body completes payload-less (range spent or pierce spent —
// both reach DoneExpire). Gated on ProjRender so a plain DoneExpire mover (the
// zero value, used by many non-projectile movers) stays silent. Mirrors the
// legacy expireMissileAt (#590 facade parity). Non-hashing presentation.
func (w *World) projectileExpire(r int32) {
	ms := w.Movers
	if w.ProjRender.Row(ms.Target[r]) == -1 {
		return
	}
	at := ms.Goal[r]
	if tr := w.Transforms.Row(ms.Target[r]); tr != -1 {
		at = w.Transforms.Pos[tr]
	}
	if w.OnMissileExpire != nil {
		w.OnMissileExpire(w.tick, ms.Target[r], at)
	}
	w.Emit(Event{Kind: EvMissileExpired, Src: ms.Target[r]})
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

// moverImpact delivers the payload exactly ONCE at the mover's final
// position — the missile point/homing-arrival impact model (#590). The
// effect list runs a single time with Point = final pos and Target = the
// homing guide (Anchor) if still alive, else 0 (a point impact); the
// packet variant damages that target (or its preset Packet.Target). This
// is the single-shot counterpart to moverDetonate's per-unit AoE: a
// point-targeted projectile whose effect list does its own area must not
// re-run the list once per nearby unit. Mirrors impactMissile exactly for
// the hashing-relevant delivery + EvMissileImpact; the render cue is a
// non-hashing presentation event (#309).
func (w *World) moverImpact(r int32) {
	ms := w.Movers
	tr := w.Transforms.Row(ms.Target[r])
	if tr == -1 {
		return // body gone: nothing to anchor the impact to (fail-safe)
	}
	at := w.Transforms.Pos[tr]
	tgt := ms.Anchor[r] // homing guide; 0 for a point mover
	if tgt != 0 && !w.Ents.Alive(tgt) {
		tgt = 0 // guide died: degrade to a point impact (never deliver to a corpse)
	}
	if lst := ms.Payload[r]; lst.Len > 0 {
		w.ExecuteEffects(lst, EffectCtx{Source: ms.Owner[r], Target: tgt, Point: at})
	} else {
		p := ms.Packet[r]
		if tgt != 0 {
			p.Target = tgt // homing delivery follows the live guide
		}
		if p.Source == 0 {
			p.Source = ms.Owner[r]
		}
		if p.Target != 0 {
			w.QueueDamage(p)
		}
	}
	// Presentation parity with the legacy missile (#590): a projectile body
	// fires the OnMissileImpact hook. Gated on ProjRender so a non-projectile
	// DoneImpact mover (an ability's point-cast AoE) never spuriously fires a
	// "missile" cue. Non-hashing presentation, like the render event below.
	if w.OnMissileImpact != nil && w.ProjRender.Row(ms.Target[r]) != -1 {
		w.OnMissileImpact(w.tick, ms.Target[r], at, tgt)
	}
	w.EmitRenderEventAt(RenderMissileImpact, ms.Target[r], 0, at)
	w.Emit(Event{Kind: EvMissileImpact, Src: ms.Target[r], Dst: tgt})
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
