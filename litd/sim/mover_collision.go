package sim

// Mover collision + impact (#587). After a mover moves (phase 4), if it
// carries a HitMask it radius-tests against units in range (bucket-grid
// broad-phase, deterministic ascending order), filtered by the shared
// MissileHit* class/team mask. Each fresh victim takes the Payload (effect
// list) or Packet (damage, with per-mille decay between hits), is recorded
// in a small already-hit ring so a piercing mover never double-hits, and
// raises EvMissileImpact (movers supersede missiles, #590 — same impact
// semantics). Pierce counts down; at 0 the mover completes (consumed).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// moverHitRing is the per-mover already-hit memory depth. A mover that
// pierces more than this many distinct units may re-hit the oldest — far
// beyond any real pierce budget.
const moverHitRing = 8

// moverCollide runs the collision pass for a live mover at its current
// Target position. Returns true if the mover completed (consumed) this pass.
func (w *World) moverCollide(r int32) bool {
	ms := w.Movers
	if ms.HitMask[r] == 0 {
		return false // collision disabled for this mover
	}
	tr := w.Transforms.Row(ms.Target[r])
	if tr == -1 {
		return false
	}
	at := w.Transforms.Pos[tr]
	team := w.moverSourceTeam(r)

	w.moverHitScratch = w.AppendUnitsInRange(w.moverHitScratch[:0], at, ms.Radius[r])
	for _, u := range w.moverHitScratch {
		if u == ms.Owner[r] || u == ms.Target[r] {
			continue // never hit the caster or the projectile body itself
		}
		if w.moverAlreadyHit(r, u) {
			continue
		}
		if !w.missileHitAllowed(u, team, ms.HitMask[r]) {
			continue
		}
		w.moverDeliver(r, u, at)
		w.moverRecordHit(r, u)
		w.Emit(Event{Kind: EvMissileImpact, Src: ms.Target[r], Dst: u})
		w.moverDecay(r)
		ms.Pierce[r]--
		if ms.Pierce[r] <= 0 {
			w.moverComplete(r)
			return true
		}
	}
	return false
}

// moverSourceTeam is the team of the mover's owner (caster), used for the
// ally/enemy mask test. A vanished owner is team 0.
func (w *World) moverSourceTeam(r int32) uint8 {
	if or := w.Owners.Row(w.Movers.Owner[r]); or != -1 {
		return w.Owners.Team[or]
	}
	return 0
}

// moverDeliver applies the payload to one victim: the effect list if set,
// otherwise the damage packet (homing/AoE delivery follows the live hit).
func (w *World) moverDeliver(r int32, victim EntityID, at fixed.Vec2) {
	ms := w.Movers
	// Per-hit presentation cue for a projectile body (#590 facade parity with
	// the legacy linear missile, which fired OnMissileImpact + a render event
	// per pierce hit). Gated on ProjRender so a non-projectile collision mover
	// stays silent. Non-hashing — the callback + render event never touch sim
	// state (moverImpact inlines its own delivery, so no double-fire).
	if w.ProjRender.Row(ms.Target[r]) != -1 {
		if w.OnMissileImpact != nil {
			w.OnMissileImpact(w.tick, ms.Target[r], at, victim)
		}
		w.EmitRenderEventAt(RenderMissileImpact, ms.Target[r], 0, at)
	}
	if lst := ms.Payload[r]; lst.Len > 0 {
		w.ExecuteEffects(lst, EffectCtx{Source: ms.Owner[r], Target: victim, Point: at})
		return
	}
	p := ms.Packet[r]
	p.Target = victim
	if p.Source == 0 {
		p.Source = ms.Owner[r]
	}
	if p.Target != 0 {
		w.QueueDamage(p)
	}
}

// moverDecay reduces the packet amount by Decay per-mille for the NEXT hit
// (cumulative along a piercing flight). No-op when Decay==0 or a payload
// effect list (not a packet) is in use.
func (w *World) moverDecay(r int32) {
	ms := w.Movers
	if ms.Decay[r] == 0 || ms.Payload[r].Len > 0 {
		return
	}
	keep := fixed.FromInt(int32(1000 - int64(ms.Decay[r])))
	ms.Packet[r].Amount = ms.Packet[r].Amount.Mul(keep).Div(fixed.FromInt(1000))
}

// moverSweptCollide runs the swept-segment collision pass for a linear
// skillshot mover (#620), called BEFORE the body advance with the pre-step
// position. It is a faithful port of the missile linear hit test
// (advanceLinear/linearHits in missile.go): the step segment is projected
// into an integer direction-scaled space (dirIntScale) so a foe whose
// centre lies within along ∈ [0, stepL) and lateral perp² ≤ radius²·l2 is
// crossed in exactly one advance over the whole flight — no already-hit
// ring is consulted (the [0, stepL) window referenced from the current
// position guarantees each foe is in range during exactly one tick).
// Front-to-back cursor discipline (lastAlong/lastIdx) orders multi-hit
// pierce deterministically. Radius is the mover's Radius column floored to
// whole world units, falling back to missileHitRadius when unset, matching
// the missile's integer-radius math. Fail-closed: an owner without a team
// (vanished/unowned) classifies nothing and the skillshot flies through.
// Zero alloc (bucket-grid walk, no scratch slice). Returns true if a hit
// drove Pierce to 0 (caller completes the mover).
func (w *World) moverSweptCollide(r int32) bool {
	ms := w.Movers
	tr := w.Transforms.Row(ms.Target[r])
	if tr == -1 {
		return false
	}
	pos := w.Transforms.Pos[tr]

	// Step actually swept this tick: the body advances unitStep(Dir, Speed),
	// but the final advance travels only the remaining range (mirrors the
	// missile clamp) so the window never overruns the skillshot's range.
	step := ms.Speed[r]
	if ms.RangeLeft[r] < step {
		step = ms.RangeLeft[r]
	}
	if step <= 0 {
		return false
	}
	dir := ms.Dir[r]

	// Integer, direction-scaled projection space (overflow-safe, det §2.1).
	dx := dir.X.Mul(fixed.FromInt(dirIntScale)).Floor()
	dy := dir.Y.Mul(fixed.FromInt(dirIntScale)).Floor()
	l2 := dx*dx + dy*dy
	if l2 <= 0 {
		return false
	}
	l := int64(fixed.SqrtU64(uint64(l2)))
	stepL := (int64(step) * l) >> 32 // window upper bound in along-space

	radInt := int64(missileHitRadius)
	if rf := ms.Radius[r].Floor(); rf > 0 {
		radInt = rf
	}
	radiusSqL2 := radInt * radInt * l2

	sor := w.Owners.Row(ms.Owner[r])
	if sor == -1 {
		return false // owner gone: no foe team to classify against (fail-closed)
	}
	team := w.Owners.Team[sor]
	radius := fixed.FromInt(int32(radInt))
	nextPos := pos.Add(dir.Scale(step))
	x0 := bucketCoord(minF(pos.X, nextPos.X).Sub(radius))
	x1 := bucketCoord(maxF(pos.X, nextPos.X).Add(radius))
	y0 := bucketCoord(minF(pos.Y, nextPos.Y).Sub(radius))
	y1 := bucketCoord(maxF(pos.Y, nextPos.Y).Add(radius))
	px, py := pos.X.Floor(), pos.Y.Floor()

	body := ms.Target[r]
	lastAlong, lastIdx := int64(-1), int64(-1) // cursor: pick strictly after
	for ms.Pierce[r] > 0 {
		var (
			bestAlong = int64(1) << 62
			bestIdx   = int64(1) << 62
			best      EntityID
			bestAt    fixed.Vec2
			found     bool
		)
		for by := y0; by <= y1; by++ {
			for bx := x0; bx <= x1; bx++ {
				for e := w.bucketHead[by*BucketGridSize+bx]; e != -1; e = w.bucketNext[e] {
					cid := w.bucketID[e]
					if cid == body || cid == ms.Owner[r] || !w.Ents.Alive(cid) {
						continue
					}
					if w.Healths.Row(cid) == -1 {
						continue // not damageable
					}
					if !w.missileHitAllowed(cid, team, ms.HitMask[r]) {
						continue
					}
					ctr := w.Transforms.Row(cid)
					if ctr == -1 {
						continue
					}
					cp := w.Transforms.Pos[ctr]
					relx, rely := cp.X.Floor()-px, cp.Y.Floor()-py
					along := relx*dx + rely*dy
					if along < 0 || along >= stepL {
						continue // not within this advance's swept window
					}
					perp := (relx*relx+rely*rely)*l2 - along*along
					if perp > radiusSqL2 {
						continue // outside the lateral collision radius
					}
					cIdx := int64(cid.Index())
					if along < lastAlong || (along == lastAlong && cIdx <= lastIdx) {
						continue // already delivered to (cursor discipline)
					}
					if !found || along < bestAlong || (along == bestAlong && cIdx < bestIdx) {
						found, bestAlong, bestIdx, best, bestAt = true, along, cIdx, cid, cp
					}
				}
			}
		}
		if !found {
			return false
		}
		w.moverDeliver(r, best, bestAt)
		w.moverRecordHit(r, best) // keep the ring coherent for save/loop re-arm
		w.Emit(Event{Kind: EvMissileImpact, Src: body, Dst: best})
		w.moverDecay(r)
		ms.Pierce[r]--
		lastAlong, lastIdx = bestAlong, bestIdx
	}
	return ms.Pierce[r] <= 0
}

// moverAlreadyHit reports whether victim is in the mover's recent-hit ring.
func (w *World) moverAlreadyHit(r int32, victim EntityID) bool {
	ms := w.Movers
	n := ms.HitN[r]
	if n > moverHitRing {
		n = moverHitRing
	}
	for i := int32(0); i < n; i++ {
		if ms.Hit[r][i] == victim {
			return true
		}
	}
	return false
}

// moverRecordHit appends victim to the ring (overwriting the oldest once
// full); HitN is the monotone write cursor.
func (w *World) moverRecordHit(r int32, victim EntityID) {
	ms := w.Movers
	ms.Hit[r][ms.HitN[r]%moverHitRing] = victim
	ms.HitN[r]++
}
