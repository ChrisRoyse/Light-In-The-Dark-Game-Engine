package sim

// Custom mover step (#586) — MoverCustom delegates each tick to a
// registered continuation that computes the next position from the mover's
// serializable CState. Mirrors the scheduler's ContID model (sched.go):
// the function is code (re-registered at setup, never serialized), the
// CState is data (hashed + saved by #590). So a custom mover survives
// save/load as long as the creator re-registers its step before LoadState
// — exactly the timer-cont / handler discipline.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// MoverStepFunc computes one tick of a MoverCustom. It receives the world
// (read-only intent — for seeking other state deterministically), the
// Target's current position, and the mover's CState payload; it returns
// the next position, the updated CState, and whether the motion completed.
// Must be deterministic (no wall-clock, no map iteration, sim PRNG only).
type MoverStepFunc func(w *World, pos fixed.Vec2, cs [4]int64) (next fixed.Vec2, ncs [4]int64, done bool)

// RegisterMoverStep binds a ContID to a custom step. Call at world setup in
// deterministic order (eventlint #618 guards against mid-match
// registration of event kinds; mover steps follow the same setup-only
// rule). Panics on nil or duplicate — a half-registered step table must
// fail loudly, never silently run the wrong code. id 0 is reserved (an
// unregistered MoverCustom fails closed → completes).
func (s *MoverStore) RegisterMoverStep(id uint16, fn MoverStepFunc) {
	if id == 0 {
		panic("sim: RegisterMoverStep id 0 is reserved")
	}
	if fn == nil {
		panic("sim: RegisterMoverStep with nil func")
	}
	if s.steps == nil {
		s.steps = make(map[uint16]MoverStepFunc, 8)
	}
	if _, dup := s.steps[id]; dup {
		panic("sim: duplicate MoverStep ContID registration")
	}
	s.steps[id] = fn
}

// moverStepCustom dispatches one tick of a MoverCustom. A missing/unset
// ContID completes the mover (fail closed).
func (w *World) moverStepCustom(r int32) {
	ms := w.Movers
	pos, tr, ok := w.moverPos(r)
	if !ok {
		w.moverComplete(r)
		return
	}
	fn := ms.steps[ms.Cont[r]]
	if fn == nil {
		if ms.DebugAssert != nil {
			ms.DebugAssert("MoverCustom with unregistered ContID", makeMoverID(uint32(r), ms.Gen[r]))
		}
		w.moverComplete(r)
		return
	}
	next, ncs, done := fn(w, pos, ms.CState[r])
	w.moverWrite(tr, next)
	ms.CState[r] = ncs
	if done {
		w.moverComplete(r)
	}
}

// moverCustomNext is a tiny helper for step authors: advance pos by a fixed
// per-tick delta. Kept here so custom steps need not import fixed plumbing.
func moverCustomNext(pos, delta fixed.Vec2) fixed.Vec2 { return pos.Add(delta) }
