package sim

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func clockFSV(w *World) string {
	return fmt.Sprintf("tick=%d tod=%s raw=%d scale=%s scaleRaw=%d frozen=%v carry=%d dayLength=%d",
		w.tick, fixedClock(w.tod), int64(w.tod), fixedClock(w.todScale), int64(w.todScale),
		w.todFrozen, w.todCarry, w.dayLengthTicks)
}

func fixedClock(v fixed.F64) string {
	raw := int64(v)
	sign := ""
	if raw < 0 {
		sign = "-"
		raw = -raw
	}
	whole := raw / int64(fixed.One)
	frac := (raw % int64(fixed.One)) * 10000 / int64(fixed.One)
	return fmt.Sprintf("%s%d.%04d", sign, whole, frac)
}

func TestClockDefaultsAndOneHourAdvance(t *testing.T) {
	w := NewWorld(Caps{})
	before := clockFSV(w)
	for i := 0; i < 400; i++ {
		w.Step()
	}
	after := clockFSV(w)
	t.Logf("FSV happy path BEFORE: %s", before)
	t.Logf("FSV happy path AFTER:  %s", after)

	if w.tod != fixed.One {
		t.Fatalf("tod raw=%d, want one hour raw=%d", int64(w.tod), int64(fixed.One))
	}
	if w.todScale != fixed.One || w.todFrozen || w.dayLengthTicks != DefaultDayLengthTicks {
		t.Fatalf("clock defaults changed: %s", clockFSV(w))
	}
	if TimeDawn != 6*fixed.One || TimeDusk != 18*fixed.One {
		t.Fatalf("dawn/dusk constants = %d/%d, want %d/%d",
			int64(TimeDawn), int64(TimeDusk), int64(6*fixed.One), int64(18*fixed.One))
	}
}

func TestClockDeterministicCheckpoints(t *testing.T) {
	w1 := NewWorld(Caps{})
	w2 := NewWorld(Caps{})
	start := 5*fixed.One + fixed.One/3
	scale := fixed.One + fixed.One/2
	w1.SetTimeOfDay(start)
	w2.SetTimeOfDay(start)
	if !w1.SetTimeOfDayScale(scale) || !w2.SetTimeOfDayScale(scale) {
		t.Fatal("positive scale rejected")
	}

	t.Logf("FSV determinism BEFORE w1: %s", clockFSV(w1))
	t.Logf("FSV determinism BEFORE w2: %s", clockFSV(w2))
	for i := 1; i <= 1000; i++ {
		w1.Step()
		w2.Step()
		if i%100 == 0 {
			t.Logf("FSV determinism checkpoint %d w1: %s", i, clockFSV(w1))
			t.Logf("FSV determinism checkpoint %d w2: %s", i, clockFSV(w2))
			if w1.tod != w2.tod || w1.todCarry != w2.todCarry || w1.tick != w2.tick {
				t.Fatalf("clock diverged at checkpoint %d:\nw1 %s\nw2 %s", i, clockFSV(w1), clockFSV(w2))
			}
		}
	}
}

func TestClockWrapEdge(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetTimeOfDay(23*fixed.One + 99*fixed.One/100)
	before := clockFSV(w)
	crossed := false
	for i := 0; i < 8; i++ {
		prev := w.tod
		w.Step()
		if w.tod < prev {
			crossed = true
		}
		if w.tod < 0 || w.tod >= clockDay {
			t.Fatalf("tod left [0,24) at step %d: %s", i+1, clockFSV(w))
		}
	}
	after := clockFSV(w)
	t.Logf("FSV wrap BEFORE: %s", before)
	t.Logf("FSV wrap AFTER:  %s", after)
	if !crossed {
		t.Fatalf("tod never wrapped across midnight: before %s after %s", before, after)
	}
	if w.tod >= fixed.One {
		t.Fatalf("tod after wrap=%s, want first hour after midnight", fixedClock(w.tod))
	}
}

func TestClockSetTimeWrapsSignedInputs(t *testing.T) {
	w := NewWorld(Caps{})
	before := clockFSV(w)
	w.SetTimeOfDay(-1 * fixed.One)
	negative := clockFSV(w)
	if w.tod != 23*fixed.One {
		t.Fatalf("SetTimeOfDay(-1) raw=%d, want 23h raw=%d", int64(w.tod), int64(23*fixed.One))
	}
	w.SetTimeOfDay(25 * fixed.One)
	over := clockFSV(w)
	t.Logf("FSV signed wrap BEFORE: %s", before)
	t.Logf("FSV signed wrap AFTER -1h: %s", negative)
	t.Logf("FSV signed wrap AFTER 25h: %s", over)
	if w.tod != fixed.One {
		t.Fatalf("SetTimeOfDay(25) raw=%d, want 1h raw=%d", int64(w.tod), int64(fixed.One))
	}
}

func TestClockFrozenEdge(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetTimeOfDay(13*fixed.One + 37*fixed.One/100)
	w.SuspendTimeOfDay(true)
	beforeTOD, beforeCarry := w.tod, w.todCarry
	before := clockFSV(w)
	for i := 0; i < 1000; i++ {
		w.Step()
	}
	after := clockFSV(w)
	t.Logf("FSV frozen BEFORE: %s", before)
	t.Logf("FSV frozen AFTER:  %s", after)
	if w.tick != 1000 {
		t.Fatalf("tick=%d, want 1000", w.tick)
	}
	if w.tod != beforeTOD || w.todCarry != beforeCarry {
		t.Fatalf("frozen tod changed:\nbefore %s\nafter  %s", before, after)
	}
}

func TestClockScaleTwoNoDrift(t *testing.T) {
	w := NewWorld(Caps{})
	if !w.SetTimeOfDayScale(2 * fixed.One) {
		t.Fatal("scale 2x rejected")
	}
	before := clockFSV(w)
	for i := 0; i < 2400; i++ {
		w.Step()
	}
	mid := clockFSV(w)
	if w.tod != 12*fixed.One {
		t.Fatalf("midday raw=%d, want %d; state %s", int64(w.tod), int64(12*fixed.One), mid)
	}
	for i := 0; i < 2400; i++ {
		w.Step()
	}
	after := clockFSV(w)
	t.Logf("FSV scale 2x BEFORE: %s", before)
	t.Logf("FSV scale 2x MID:    %s", mid)
	t.Logf("FSV scale 2x AFTER:  %s", after)
	if w.tod != 0 || w.todCarry != 0 {
		t.Fatalf("scale 2x drift after full day: %s", after)
	}
}

func TestClockRejectsNegativeScale(t *testing.T) {
	w := NewWorld(Caps{})
	if !w.SetTimeOfDayScale(3 * fixed.One) {
		t.Fatal("positive setup scale rejected")
	}
	before := clockFSV(w)
	ok := w.SetTimeOfDayScale(-1 * fixed.One)
	after := clockFSV(w)
	t.Logf("FSV reject negative scale BEFORE: %s", before)
	t.Logf("FSV reject negative scale AFTER:  %s returned=%v", after, ok)
	if ok {
		t.Fatal("negative scale accepted")
	}
	if w.todScale != 3*fixed.One {
		t.Fatalf("scale changed after rejection: %s", after)
	}
}
