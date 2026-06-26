// Package helpers is the D4 "keep" library (deduplication-policy.md §5):
// the blizzard.j BJ functions that carry real logic — batch creation,
// weighted random, the polled wait, gate/elevator widgets, transmissions —
// reimplemented PURELY on the public litd/api surface. It is the dogfood
// proof that the deduplicated core API loses no power: every helper here
// is something a game developer could have written themselves with nothing
// but the exported `litd` package.
//
// Import discipline (architecture.md §1.1, the no-power-lost gate): this
// package and its sub-packages import ONLY litd/api (package name `litd`)
// and the standard library (melee adds BurntSushi/toml for its data
// tables). They never import litd/sim, litd/render, or any other internal
// package — a helper that needed to reach past the public boundary would
// prove the boundary was incomplete, which is exactly the M5 defect this
// library is built to surface. (litd/api itself transitively depends on
// litd/sim, since it IS the public face of the sim; the gate is on
// helpers' own imports and on litd/render staying unreachable — see
// import_test.go.)
package helpers

import (
	"time"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// PolledWait suspends the currently running script thread for d of game
// time, resuming on a later tick (R-EXEC-5: durations quantize UP to whole
// 50 ms ticks). d <= 0 returns immediately without suspending and without
// creating any scheduler record — matching JASS's `if duration > 0` guard
// in PolledWait. The D4 mapping (deduplication-policy.md §5 row 1): the
// real-time-drifting timer-polling BJ becomes a direct, drift-free
// scheduler suspension.
//
// The bare (duration-only) signature is intentional: the cooperative
// scheduler runs exactly one thread at a time (execution-model.md §2 S-1),
// so "the current thread" is unambiguous — PolledWait resolves it via
// litd.CurrentThread(). It must therefore be called from inside a thread
// started with Game.Run; calling it with d > 0 outside any thread is a
// script bug and panics (fail-closed: there is no thread to suspend, so
// silently returning would skip a wait the script asked for).
func PolledWait(d time.Duration) {
	if d <= 0 {
		return // JASS guard: non-positive waits are a same-tick no-op
	}
	t := litd.CurrentThread()
	if t == nil {
		panic("helpers: PolledWait(d>0) called outside a script thread — start the sequence with Game.Run")
	}
	t.PolledWait(d)
}
