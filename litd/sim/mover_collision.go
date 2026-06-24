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
