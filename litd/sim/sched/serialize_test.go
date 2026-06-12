package sched

import (
	"crypto/sha256"
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// buildWorkload populates a scheduler with the standard mixed workload:
// chained waits at staggered delays plus event waiters, fired every 7
// ticks by the driver below.
func buildWorkload(e *env) *Scheduler {
	s := newTestSched(e)
	for i := 0; i < 6; i++ {
		s.After(uint32(i%3)+1, contChain, State{int64(i)})
		s.WaitEvent(evPing, contNote, State{int64(100 + i)})
	}
	return s
}

func drive(s *Scheduler, ticks int) {
	for i := 0; i < ticks; i++ {
		s.Step()
		if s.Now()%7 == 0 {
			s.FireEvent(evPing)
		}
	}
}

func traceHash(tr []string) uint64 {
	h := statehash.New()
	for _, line := range tr {
		h.WriteBytes([]byte(line))
		h.WriteU8('\n')
	}
	return h.Sum64()
}

// The D-28 bar, on the real format: run k ticks -> save -> run to end
// must equal restore(save) -> run to end, trace and final state
// bit-identical. This test is the permanent CI fixture (runs in the
// `go test ./...` CI step).
func TestRoundTripMidRunFixture(t *testing.T) {
	// k=4 saves a LIVE scheduler: chained notes still sleeping, all six
	// event waiters still parked (the ping fires at tick 7) — the blob
	// must carry real records, not an empty header.
	const k, total = 4, 25

	// uninterrupted reference
	eRef := &env{}
	sRef := buildWorkload(eRef)
	drive(sRef, total)

	// interrupted: run k, save, keep running to the end
	eA := &env{}
	sA := buildWorkload(eA)
	drive(sA, k)
	preSaveTrace := len(eA.trace)
	blob := sA.Save(nil)
	drive(sA, total-k)

	// restored: fresh scheduler + registry, load, run the remainder
	eB := &env{}
	sB := newTestSched(eB)
	if err := sB.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	drive(sB, total-k)

	if len(blob) <= 26 {
		t.Fatalf("fixture saved an empty scheduler (%d bytes) — move k earlier", len(blob))
	}
	t.Logf("blob: %d bytes, sha256 %x", len(blob), sha256.Sum256(blob))
	t.Logf("header hex: % x", blob[:22])
	t.Logf("%-28s %-18s %s", "run", "trace hash", "entries")
	t.Logf("%-28s 0x%016X %d", "uninterrupted", traceHash(eRef.trace), len(eRef.trace))
	resumed := append(append([]string{}, eA.trace[:preSaveTrace]...), eB.trace...)
	t.Logf("%-28s 0x%016X %d", fmt.Sprintf("save@%d + restore + run", k), traceHash(resumed), len(resumed))
	t.Logf("%-28s 0x%016X %d", fmt.Sprintf("save@%d + keep running", k), traceHash(eA.trace), len(eA.trace))

	if strings.Join(eRef.trace, "\n") != strings.Join(eA.trace, "\n") {
		t.Fatal("saving must not perturb the running scheduler")
	}
	if strings.Join(eRef.trace, "\n") != strings.Join(resumed, "\n") {
		t.Fatalf("restored run diverged:\nref: %v\nres: %v", eRef.trace, resumed)
	}

	finalRef, finalB := sRef.Save(nil), sB.Save(nil)
	t.Logf("final state: uninterrupted sha256 %x", sha256.Sum256(finalRef))
	t.Logf("final state: restored      sha256 %x", sha256.Sum256(finalB))
	if string(finalRef) != string(finalB) {
		t.Fatal("final scheduler state differs between uninterrupted and restored runs")
	}
}

// Edge: save while a script sleeps mid-Wait with a far-future
// wakeTick; the restored script wakes on the exact tick.
func TestMidWaitWakeTickSurvives(t *testing.T) {
	eA := &env{}
	sA := newTestSched(eA)
	sA.After(50, contNote, State{42})
	drive(sA, 3)
	t.Logf("before save: now=%d pending wakeTick=%d seq=%d", sA.now, sA.sleep[0].wakeTick, sA.sleep[0].seq)
	blob := sA.Save(nil)

	eB := &env{}
	sB := newTestSched(eB)
	if err := sB.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	t.Logf("after restore: now=%d pending wakeTick=%d seq=%d", sB.now, sB.sleep[0].wakeTick, sB.sleep[0].seq)
	if sB.sleep[0].wakeTick != 50 || sB.now != 3 {
		t.Fatalf("restore mangled state: now=%d wakeTick=%d", sB.now, sB.sleep[0].wakeTick)
	}
	for sB.Now() < 49 {
		sB.Step()
	}
	if len(eB.trace) != 0 {
		t.Fatalf("woke early: %v", eB.trace)
	}
	sB.Step() // tick 50
	t.Logf("restored trace: %v", eB.trace)
	if fmt.Sprint(eB.trace) != fmt.Sprint([]string{"t50 note id=42"}) {
		t.Fatalf("wrong wake: %v", eB.trace)
	}
}

// Edge: pending event waiters cross the save; restored FIFO intact.
func TestEventWaitersFIFOSurvives(t *testing.T) {
	eA := &env{}
	sA := newTestSched(eA)
	sA.WaitEvent(evPing, contNote, State{1})
	sA.WaitEvent(evPing, contNote, State{2})
	sA.WaitEvent(evPing, contNote, State{3})
	blob := sA.Save(nil)

	eB := &env{}
	sB := newTestSched(eB)
	if err := sB.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	sB.FireEvent(evPing)
	t.Logf("restored dispatch: %v", eB.trace)
	want := []string{"t00 note id=1", "t00 note id=2", "t00 note id=3"}
	if fmt.Sprint(eB.trace) != fmt.Sprint(want) {
		t.Fatalf("FIFO broken after restore:\ngot  %v\nwant %v", eB.trace, want)
	}
}

// Edge: seq-counter continuity — a suspension created after restore
// gets the same seq the unbroken run would have issued.
func TestSeqContinuityAcrossRestore(t *testing.T) {
	eA := &env{}
	sA := buildWorkload(eA)
	drive(sA, 8)
	blob := sA.Save(nil)

	eB := &env{}
	sB := newTestSched(eB)
	if err := sB.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	sA.After(5, contNote, State{777}) // unbroken run issues next seq
	sB.After(5, contNote, State{777}) // restored run must match
	seqA := sA.sleep[len(sA.sleep)-1].seq
	var seqB uint32
	for i := range sB.sleep {
		if sB.sleep[i].state[0] == 777 {
			seqB = sB.sleep[i].seq
		}
	}
	// find actual seq via max, heap tail isn't guaranteed: scan both
	for i := range sA.sleep {
		if sA.sleep[i].state[0] == 777 {
			seqA = sA.sleep[i].seq
		}
	}
	t.Logf("post-save suspension seq: unbroken=%d restored=%d (nextSeq continued)", seqA, seqB)
	if seqA != seqB {
		t.Fatalf("seq diverged: unbroken %d restored %d", seqA, seqB)
	}
}

// Edge: serialize-twice determinism — byte-identical blobs.
func TestSaveTwiceByteIdentical(t *testing.T) {
	e := &env{}
	s := buildWorkload(e)
	drive(s, 4) // live state: sleepers + parked waiters in the blob
	b1, b2 := s.Save(nil), s.Save(nil)
	h1, h2 := sha256.Sum256(b1), sha256.Sum256(b2)
	t.Logf("save #1: %d bytes sha256 %x", len(b1), h1)
	t.Logf("save #2: %d bytes sha256 %x", len(b2), h2)
	if h1 != h2 || len(b1) != len(b2) {
		t.Fatal("two saves of identical state differ")
	}
	if len(b1) != s.SaveSize() {
		t.Fatalf("SaveSize %d != actual %d", s.SaveSize(), len(b1))
	}
}

// Edge: corrupted/truncated blobs fail loudly with ZERO partial state
// applied — the scheduler still equals its pre-Load state afterwards.
func TestCorruptBlobFailsClosed(t *testing.T) {
	// explicit populated state: two sleep records + two waiters, so the
	// field-offset mutations below hit real bytes
	e := &env{}
	s := newTestSched(e)
	s.After(5, contNote, State{1})
	s.After(5, contNote, State{2})
	s.WaitEvent(evPing, contNote, State{3})
	s.WaitEvent(evPing, contNote, State{4})
	good := s.Save(nil)
	t.Logf("base blob: %d bytes (2 sleepers, 2 waiters)", len(good))

	mutate := func(name string, f func([]byte) []byte) {
		eV := &env{}
		victim := newTestSched(eV)
		victim.After(9, contNote, State{1}) // pre-existing state to protect
		before := victim.Save(nil)
		err := victim.Load(f(append([]byte{}, good...)))
		after := victim.Save(nil)
		if err == nil {
			t.Fatalf("%s: Load accepted corrupt blob", name)
		}
		t.Logf("%-22s -> %v", name, err)
		if string(before) != string(after) {
			t.Fatalf("%s: partial state applied despite error", name)
		}
	}

	mutate("truncated-mid-record", func(b []byte) []byte { return b[:len(b)-13] })
	mutate("truncated-header", func(b []byte) []byte { return b[:5] })
	mutate("bad-magic", func(b []byte) []byte { b[0] = 'X'; return b })
	mutate("bad-version", func(b []byte) []byte { b[8] = 99; return b })
	mutate("trailing-garbage", func(b []byte) []byte { return append(b, 0xAB) })
	mutate("unregistered-cont", func(b []byte) []byte {
		// first sleep record's cont field: 8+2+4+4+4 + 4+4 = offset 30
		b[30] = 0xEE
		return b
	})
	mutate("non-canonical-order", func(b []byte) []byte {
		// swap first two sleep records (each recordSize bytes from offset 22)
		tmp := make([]byte, recordSize)
		copy(tmp, b[22:22+recordSize])
		copy(b[22:], b[22+recordSize:22+2*recordSize])
		copy(b[22+recordSize:], tmp)
		return b
	})
}
