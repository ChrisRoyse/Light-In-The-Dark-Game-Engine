// Package main — desyncbisect (#251, R-OBS-5, observability-and-debugging.md
// §5): given two replays of the SAME inputs that produced diverging state, walk
// their per-system sub-hash checkpoint traces and pinpoint the FIRST divergence
// as (tick, system) with both sub-hash values — so a desync names its culprit
// system, not just "the runs differ". Consumed by M7 desync handling (#77) and
// the debug-report bundle (#250).
package main

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Divergence is the bisect result. Found=false means the traces are identical
// over their common checkpoints.
type Divergence struct {
	Found  bool
	Tick   uint32
	System string // HashSystems name, "top" (sub-equal but top differs), or "(structure)"
	SubA   uint64
	SubB   uint64
	TopA   uint64
	TopB   uint64
	Detail string // human note for structural cases
}

// headerMismatch reports the first incompatibility that makes two replays
// non-comparable as a desync (they describe different inputs, so any state
// difference is expected, not a desync). Empty string = comparable.
func headerMismatch(a, b *sim.Replay) string {
	switch {
	case a.Seed != b.Seed:
		return fmt.Sprintf("seed %d vs %d", a.Seed, b.Seed)
	case a.Roster != b.Roster:
		return fmt.Sprintf("roster %d vs %d", a.Roster, b.Roster)
	case a.Fingerprint != b.Fingerprint:
		return fmt.Sprintf("data-table fingerprint %016x vs %016x", a.Fingerprint, b.Fingerprint)
	case a.MapHash != b.MapHash:
		return fmt.Sprintf("map hash %016x vs %016x", a.MapHash, b.MapHash)
	case a.Ticks != b.Ticks:
		return fmt.Sprintf("tick count %d vs %d", a.Ticks, b.Ticks)
	}
	return ""
}

// Bisect finds the first checkpoint divergence between a and b. It is
// fail-closed: replays describing different inputs are refused (an error),
// because a state difference there is not a desync. Checkpoints are walked in
// recorded (ascending-tick) order; the first system to differ is reported in
// HashSystems order, so the result is deterministic when several systems
// diverge on the same tick.
func Bisect(a, b *sim.Replay) (Divergence, error) {
	if mm := headerMismatch(a, b); mm != "" {
		return Divergence{}, fmt.Errorf("replays describe different inputs (%s): not a desync — cannot bisect", mm)
	}

	byTickB := make(map[uint32]*sim.ReplayCheckpoint, len(b.Checkpoints))
	for i := range b.Checkpoints {
		byTickB[b.Checkpoints[i].Tick] = &b.Checkpoints[i]
	}

	for i := range a.Checkpoints {
		ca := &a.Checkpoints[i]
		cb, ok := byTickB[ca.Tick]
		if !ok {
			return Divergence{
				Found:  true,
				Tick:   ca.Tick,
				System: "(structure)",
				Detail: fmt.Sprintf("checkpoint at tick %d present in A but absent in B", ca.Tick),
			}, nil
		}
		if ca.Top == cb.Top {
			continue
		}
		// Tops differ — name the first system whose sub-hash differs.
		n := len(ca.Subs)
		if len(cb.Subs) < n {
			n = len(cb.Subs)
		}
		for s := 0; s < n; s++ {
			if ca.Subs[s] != cb.Subs[s] {
				name := fmt.Sprintf("sub[%d]", s)
				if s < len(sim.HashSystems) {
					name = sim.HashSystems[s]
				}
				return Divergence{
					Found:  true,
					Tick:   ca.Tick,
					System: name,
					SubA:   ca.Subs[s],
					SubB:   cb.Subs[s],
					TopA:   ca.Top,
					TopB:   cb.Top,
				}, nil
			}
		}
		// Tops differ but every sub matched — the top is a hash over the subs,
		// so this only happens if the sub vectors differ in length.
		return Divergence{
			Found:  true,
			Tick:   ca.Tick,
			System: "top",
			TopA:   ca.Top,
			TopB:   cb.Top,
			Detail: fmt.Sprintf("top hashes differ (%016x vs %016x) but no sub-hash did; sub-vector lengths %d vs %d", ca.Top, cb.Top, len(ca.Subs), len(cb.Subs)),
		}, nil
	}

	// A's checkpoints all matched; a tick present only in B is also structural.
	byTickA := make(map[uint32]struct{}, len(a.Checkpoints))
	for i := range a.Checkpoints {
		byTickA[a.Checkpoints[i].Tick] = struct{}{}
	}
	for i := range b.Checkpoints {
		if _, ok := byTickA[b.Checkpoints[i].Tick]; !ok {
			return Divergence{
				Found:  true,
				Tick:   b.Checkpoints[i].Tick,
				System: "(structure)",
				Detail: fmt.Sprintf("checkpoint at tick %d present in B but absent in A", b.Checkpoints[i].Tick),
			}, nil
		}
	}

	return Divergence{Found: false}, nil
}
