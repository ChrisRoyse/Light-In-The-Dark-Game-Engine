package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #555 — timers hash section + save block + load. SoT for these tests
// is the state hash (the "timers" sub) and the rebuilt store columns.

// armTimerPopulation creates a representative timer population on w,
// including a cancelled hole so a slot carries a non-zero generation
// and the free-list is non-trivial — the exact state most likely to
// expose a slot/gen/free-list serialization bug.
func armTimerPopulation(w *World) {
	w.Timers.Create(w.Tick(), TimerSingle, 5, 0, 3, [4]int64{1, 2, 3, 4}, 0)
	w.Timers.Create(w.Tick(), TimerLoop, 3, 0, 3, [4]int64{7, 0, 0, 0}, 0)
	w.Timers.Create(w.Tick(), TimerCount, 2, 4, 3, [4]int64{9, 0, 0, 0}, 0)
	hole := w.Timers.Create(w.Tick(), TimerSingle, 9, 0, 3, [4]int64{}, 0)
	w.Timers.Cancel(hole) // gen bump + free-list entry mid-pool
	w.Timers.Create(w.Tick(), TimerLoop, 7, 0, 3, [4]int64{5, 5, 0, 0}, 0)
}

func TestTimerSaveRoundTripHash(t *testing.T) {
	src := NewWorld(Caps{Units: 8, Timers: 64})
	src.Sched.Register(3, func(*sched.Scheduler, sched.State) {})
	armTimerPopulation(src)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	saved := append([]byte(nil), buf.Bytes()...)

	dst := NewWorld(Caps{Units: 8, Timers: 64})
	dst.Sched.Register(3, func(*sched.Scheduler, sched.State) {})
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}

	var after statehash.Snapshot
	dst.HashState(reg, &after)

	ti := hashSystemIndex(t, "timers")
	if before.Subs[ti] != after.Subs[ti] {
		t.Fatalf("timers sub-hash differs: %016x -> %016x", before.Subs[ti], after.Subs[ti])
	}
	if before.Top != after.Top {
		t.Fatalf("top hash differs; diverged: %v", snapDiff(t, &before, &after))
	}

	// SoT: rebuilt store columns match the source exactly.
	if dst.Timers.Count() != src.Timers.Count() {
		t.Fatalf("loaded Count=%d, want %d", dst.Timers.Count(), src.Timers.Count())
	}
	if dst.Timers.nextSeq != src.Timers.nextSeq {
		t.Fatalf("loaded nextSeq=%d, want %d", dst.Timers.nextSeq, src.Timers.nextSeq)
	}
	if dst.Timers.HeapLen() != src.Timers.HeapLen() {
		t.Fatalf("rebuilt heap len=%d, want %d", dst.Timers.HeapLen(), src.Timers.HeapLen())
	}

	// Re-save the loaded world: byte-identical.
	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, 0); err != nil {
		t.Fatalf("re-SaveState: %v", err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("re-save of restored world is not byte-identical")
	}
}

// TestTimerSurvivesSaveLoadAndFires proves the rebuilt schedule index
// actually drives firing post-load: a loaded loop timer fires on the
// same ticks as the un-saved original.
func TestTimerSurvivesSaveLoadAndFires(t *testing.T) {
	var srcLog, dstLog []uint32
	src := NewWorld(Caps{Units: 8, Timers: 64})
	src.Sched.Register(3, func(s *sched.Scheduler, _ sched.State) {
		srcLog = append(srcLog, s.Now())
	})
	src.Timers.Create(src.Tick(), TimerLoop, 4, 0, 3, [4]int64{}, 0) // fires 4,8,12,...

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	dst := NewWorld(Caps{Units: 8, Timers: 64})
	dst.Sched.Register(3, func(s *sched.Scheduler, _ sched.State) {
		dstLog = append(dstLog, s.Now())
	})
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}

	for i := 0; i < 12; i++ {
		src.Step()
		dst.Step()
	}
	if len(dstLog) == 0 {
		t.Fatal("loaded timer never fired — schedule index not rebuilt")
	}
	if len(srcLog) != len(dstLog) {
		t.Fatalf("fire count src=%v dst=%v", srcLog, dstLog)
	}
	for i := range srcLog {
		if srcLog[i] != dstLog[i] {
			t.Fatalf("fire ticks differ at %d: %v vs %v", i, srcLog, dstLog)
		}
	}
}

// TestTimerHashLocalizesToTimers proves a corrupted timer column moves
// only the "timers" sub-hash — divergence localizes (R-SIM-6).
func TestTimerHashLocalizesToTimers(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Timers: 64})
	w.Sched.Register(3, func(*sched.Scheduler, sched.State) {})
	armTimerPopulation(w)

	reg := NewHashRegistry()
	var a statehash.Snapshot
	w.HashState(reg, &a)

	// Corrupt one live timer's WakeTick.
	for idx := int32(1); idx < int32(len(w.Timers.live)); idx++ {
		if w.Timers.live[idx] {
			w.Timers.WakeTick[idx] += 1000
			break
		}
	}
	var b statehash.Snapshot
	w.HashState(reg, &b)

	diverged := snapDiff(t, &a, &b)
	if len(diverged) != 1 || diverged[0] != "timers" {
		t.Fatalf("corruption diverged %v, want exactly [timers]", diverged)
	}
}

// #611 — a paused timer round-trips through save/load: hash identical,
// pause state + remaining preserved, and it stays out of the heap.
func TestTimerPauseSaveLoadRoundTrip(t *testing.T) {
	src := NewWorld(Caps{Units: 8, Timers: 64})
	src.Sched.Register(5, func(*sched.Scheduler, sched.State) {})
	id := src.Timers.Create(src.Tick(), TimerLoop, 10, 0, 5, [4]int64{1}, 0)
	for i := 0; i < 3; i++ {
		src.Step()
	}
	if !src.Timers.Pause(id, src.Tick()) {
		t.Fatal("Pause failed")
	}
	wantRem := src.Timers.PausedRem[id.Index()]

	reg := NewHashRegistry()
	var before, after statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	dst := NewWorld(Caps{Units: 8, Timers: 64})
	dst.Sched.Register(5, func(*sched.Scheduler, sched.State) {})
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	dst.HashState(reg, &after)
	if before.Top != after.Top {
		t.Fatalf("paused-timer save/load hash mismatch; diverged: %v", snapDiff(t, &before, &after))
	}
	if !dst.Timers.IsPaused(id) {
		t.Fatal("loaded timer not paused")
	}
	if dst.Timers.PausedRem[id.Index()] != wantRem {
		t.Fatalf("loaded PausedRem=%d, want %d", dst.Timers.PausedRem[id.Index()], wantRem)
	}
	if dst.Timers.HeapLen() != 0 {
		t.Fatalf("paused timer re-entered heap on load (len=%d)", dst.Timers.HeapLen())
	}
}
