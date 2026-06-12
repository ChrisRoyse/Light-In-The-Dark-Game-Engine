package sim

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

const (
	contNoteW   sched.ContID = 1
	contZeroW   sched.ContID = 2
	contRewaitW sched.ContID = 3
	evPingW     sched.EventID = 1
)

func newScriptWorld(trace *[]string) *World {
	w := NewWorld(Caps{Units: 16})
	w.Sched.Register(contNoteW, func(s *sched.Scheduler, st sched.State) {
		*trace = append(*trace, fmt.Sprintf("t%02d note id=%d", w.Tick(), st[0]))
	})
	w.Sched.Register(contZeroW, func(s *sched.Scheduler, st sched.State) {
		*trace = append(*trace, fmt.Sprintf("t%02d zero id=%d", w.Tick(), st[0]))
		w.AfterMS(0, contNoteW, st) // Wait(0): must resume NEXT tick
	})
	w.Sched.Register(contRewaitW, func(s *sched.Scheduler, st sched.State) {
		*trace = append(*trace, fmt.Sprintf("t%02d rewait id=%d", w.Tick(), st[0]))
		s.WaitEvent(evPingW, contRewaitW, st)
	})
	return w
}

// Edge: Wait(0) at tick N resumes at N+1, inside phase 2 of N+1.
func TestSchedulerPhaseWaitZeroNextTick(t *testing.T) {
	var trace []string
	w := newScriptWorld(&trace)
	w.AfterMS(0, contZeroW, sched.State{9}) // staged pre-tick: resumes tick 1
	w.Step()                                // tick 1: zero runs, schedules Wait(0)
	afterTick1 := len(trace)
	w.Step() // tick 2: note runs
	t.Logf("trace: %v (entries after tick 1: %d)", trace, afterTick1)
	want := []string{"t01 zero id=9", "t02 note id=9"}
	if fmt.Sprint(trace) != fmt.Sprint(want) || afterTick1 != 1 {
		t.Fatalf("Wait(0) timing wrong:\ngot  %v\nwant %v", trace, want)
	}
}

// Edge: 49 ms and 50 ms quantize to +1 tick; 51 ms to +2 (always UP).
func TestSchedulerPhaseQuantization(t *testing.T) {
	cases := []struct {
		ms   uint32
		want uint32
	}{{0, 0}, {1, 1}, {49, 1}, {50, 1}, {51, 2}, {100, 2}, {101, 3}, {5000, 100}}
	t.Logf("%6s %s", "ms", "ticks (quantized up)")
	for _, c := range cases {
		got := QuantizeMS(c.ms)
		t.Logf("%6d %d", c.ms, got)
		if got != c.want {
			t.Fatalf("QuantizeMS(%d) = %d, want %d", c.ms, got, c.want)
		}
	}

	// end-to-end through World.Step: 49/50/51 ms from tick 0
	var trace []string
	w := newScriptWorld(&trace)
	w.AfterMS(49, contNoteW, sched.State{49})
	w.AfterMS(50, contNoteW, sched.State{50})
	w.AfterMS(51, contNoteW, sched.State{51})
	for i := 0; i < 3; i++ {
		w.Step()
	}
	t.Logf("resume trace: %v", trace)
	want := []string{"t01 note id=49", "t01 note id=50", "t02 note id=51"}
	if fmt.Sprint(trace) != fmt.Sprint(want) {
		t.Fatalf("quantized resumes wrong:\ngot  %v\nwant %v", trace, want)
	}
}

// Edge: two suspensions on the same wakeTick resume in seq order;
// swapping registration order flips the trace.
func TestSchedulerPhaseSeqOrderFlips(t *testing.T) {
	run := func(firstID, secondID int64) string {
		var trace []string
		w := newScriptWorld(&trace)
		w.AfterMS(100, contNoteW, sched.State{firstID})  // seq 1
		w.AfterMS(100, contNoteW, sched.State{secondID}) // seq 2
		for i := 0; i < 2; i++ {
			w.Step()
		}
		return strings.Join(trace, " | ")
	}
	ab := run(1, 2)
	ba := run(2, 1)
	t.Logf("registered A then B: %s", ab)
	t.Logf("registered B then A: %s", ba)
	if ab != "t02 note id=1 | t02 note id=2" || ba != "t02 note id=2 | t02 note id=1" {
		t.Fatalf("seq order wrong: %q / %q", ab, ba)
	}
}

// Edge: a continuation that re-waits mid-resume lands at the back of
// the queue with a new seq — it is not re-run this tick.
func TestSchedulerPhaseRewaitNotRerunSameTick(t *testing.T) {
	var trace []string
	w := newScriptWorld(&trace)
	w.Sched.WaitEvent(evPingW, contRewaitW, sched.State{5})
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.Sched.FireEvent(evPingW) // dispatch: rewait runs once, re-arms
		}
	}
	w.Step() // tick 1: fire happens in phase 5; handler re-waits
	if len(trace) != 1 {
		t.Fatalf("re-armed waiter re-ran same tick: %v", trace)
	}
	w.OnCombatPhase = func(tick uint32) {
		if tick == 2 {
			w.Sched.FireEvent(evPingW)
		}
	}
	w.Step() // tick 2: re-armed waiter runs once more
	t.Logf("trace: %v; pending waiters after: %d", trace, w.Sched.PendingWaiters(evPingW))
	want := []string{"t01 rewait id=5", "t02 rewait id=5"}
	if fmt.Sprint(trace) != fmt.Sprint(want) {
		t.Fatalf("re-wait dispatch wrong:\ngot  %v\nwant %v", trace, want)
	}
}

// Scheduler tick stays in lockstep with the world tick.
func TestSchedulerPhaseLockstep(t *testing.T) {
	var trace []string
	w := newScriptWorld(&trace)
	for i := 0; i < 137; i++ {
		w.Step()
		if w.Sched.Now() != w.Tick() {
			t.Fatalf("desync: sched now=%d world tick=%d", w.Sched.Now(), w.Tick())
		}
	}
	t.Logf("after 137 steps: sched.Now()=%d == world.Tick()=%d", w.Sched.Now(), w.Tick())
}
