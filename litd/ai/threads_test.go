package ai_test

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

const threadBase sched.ContID = 1000 // reserved clear of controller conts (which use low ids)

// TestQuantizeSleepFSV — X+X=Y on the seconds→ticks rule. Sleep(0)/negative/NaN
// quantize to 0 (no wait); positive durations ceil onto the 50 ms grid with a
// 1-tick floor. Issue edge 1 (the quantization half).
func TestQuantizeSleepFSV(t *testing.T) {
	cases := []struct {
		s    float64
		want uint32
	}{
		{0, 0}, {-1, 0}, {-0.5, 0}, // <= 0 → no wait, no record
		{0.001, 1}, {0.049, 1}, {0.05, 1}, // sub-tick & exactly one tick → 1
		{0.051, 2}, {0.099, 2}, {0.1, 2}, // into the second tick → 2
		{1.0, 20}, {2.5, 50}, // whole seconds → 20 ticks/s
	}
	for _, c := range cases {
		got := ai.QuantizeSleep(c.s)
		t.Logf("FSV QuantizeSleep(%.3f s) = %d ticks (want %d)", c.s, got, c.want)
		if got != c.want {
			t.Fatalf("QuantizeSleep(%v)=%d want %d", c.s, got, c.want)
		}
	}
}

// TestTickMSDriftGuardFSV — the local TickMS must equal the canonical sim tick
// rate; this guards the duplicated constant against silent divergence (§2.10).
func TestTickMSDriftGuardFSV(t *testing.T) {
	t.Logf("FSV ai.TickMS=%d data.TickMS=%d", ai.TickMS, data.TickMS)
	if ai.TickMS != data.TickMS {
		t.Fatalf("ai.TickMS=%d diverged from data.TickMS=%d", ai.TickMS, data.TickMS)
	}
}

// TestThreadSleepNoRecordFSV — a Sleep(0) yield creates no suspension record and
// the thread continues in-line then ends; a Sleep(0.1) parks a record. Issue
// edge 1 (the behavioral half). SoT = the scheduler's pending-sleeper count.
func TestThreadSleepNoRecordFSV(t *testing.T) {
	clk := uint32(0)
	cmd := &recordingCommander{}
	d := ai.NewDomain()
	ctx := d.AddPlayer(1, staticView{self: 1, clock: &clk}, cmd, noopController{})
	ts := ai.NewThreadSet(ctx, threadBase)

	// Thread A: on its first resume, Sleep(0) — must NOT park, must end.
	idA := new(ai.ThreadID)
	*idA, _ = ts.Spawn(func(s *sched.Scheduler, st sched.State) {
		ts.Sleep(*idA, 0, sched.State{}) // immediate continuation, no record
	}, sched.State{})

	t.Logf("FSV after spawn (kickoff queued): pending=%d active=%d", ctx.PendingSleepers(), ts.Active())
	if ctx.PendingSleepers() != 1 || ts.Active() != 1 {
		t.Fatalf("post-spawn pending=%d active=%d want 1/1", ctx.PendingSleepers(), ts.Active())
	}
	clk++
	d.Tick(0) // resume A: Sleep(0) → no new record, thread ends
	t.Logf("FSV after Sleep(0) resume: pending=%d active=%d (want 0/0)", ctx.PendingSleepers(), ts.Active())
	if ctx.PendingSleepers() != 0 {
		t.Fatalf("Sleep(0) left a suspension record: pending=%d want 0", ctx.PendingSleepers())
	}
	if ts.Active() != 0 {
		t.Fatalf("thread did not end after running off the end: active=%d want 0", ts.Active())
	}

	// Thread B: Sleep(0.1) → parks a record (2 ticks), stays live.
	idB := new(ai.ThreadID)
	*idB, _ = ts.Spawn(func(s *sched.Scheduler, st sched.State) {
		ts.Sleep(*idB, 0.1, sched.State{}) // parks for 2 ticks
	}, sched.State{})
	clk++
	d.Tick(0) // resume B: Sleep(0.1) parks
	t.Logf("FSV after Sleep(0.1) resume: pending=%d active=%d (want 1/1)", ctx.PendingSleepers(), ts.Active())
	if ctx.PendingSleepers() != 1 || ts.Active() != 1 {
		t.Fatalf("Sleep(0.1) pending=%d active=%d want 1/1", ctx.PendingSleepers(), ts.Active())
	}
}

// TestThreadCapRefusalFSV — the 6-thread cap refuses the 7th spawn
// deterministically with a loud diagnostic; freeing a slot re-allows a spawn.
// Issue edge 3.
func TestThreadCapRefusalFSV(t *testing.T) {
	clk := uint32(0)
	d := ai.NewDomain()
	ctx := d.AddPlayer(1, staticView{self: 1, clock: &clk}, &recordingCommander{}, noopController{})
	var diag bytes.Buffer
	ts := ai.NewThreadSet(ctx, threadBase)
	ts.SetDiagnostics(&diag)

	ids := make([]ai.ThreadID, 0, 6)
	for i := 0; i < 6; i++ {
		idp := new(ai.ThreadID)
		id, ok := ts.Spawn(func(s *sched.Scheduler, st sched.State) {
			ts.Sleep(*idp, 100.0, sched.State{}) // park ~2000 ticks, stays live
		}, sched.State{})
		*idp = id
		if !ok {
			t.Fatalf("spawn %d refused below the cap", i)
		}
		ids = append(ids, id)
	}
	// Kick them off so they park (otherwise they are merely "active" pre-run).
	clk++
	d.Tick(0)
	t.Logf("FSV 6 threads spawned & parked: active=%d (want 6)", ts.Active())
	if ts.Active() != 6 {
		t.Fatalf("active=%d want 6", ts.Active())
	}

	// 7th spawn must be refused.
	id7, ok := ts.Spawn(func(s *sched.Scheduler, st sched.State) {}, sched.State{})
	t.Logf("FSV 7th spawn: ok=%v id=%d active=%d refused=%d", ok, id7, ts.Active(), ts.Refused())
	if ok || ts.Refused() != 1 || ts.Active() != 6 {
		t.Fatalf("7th spawn ok=%v refused=%d active=%d want false/1/6", ok, ts.Refused(), ts.Active())
	}
	if diag.Len() == 0 {
		t.Fatal("cap refusal produced no diagnostic (must be loud)")
	}
	t.Logf("FSV cap diagnostic=%q", diag.String())

	// Free a slot, then a spawn succeeds again.
	ts.Kill(ids[0])
	t.Logf("FSV after Kill: active=%d (want 5)", ts.Active())
	if ts.Active() != 5 {
		t.Fatalf("after Kill active=%d want 5", ts.Active())
	}
	_, ok = ts.Spawn(func(s *sched.Scheduler, st sched.State) {}, sched.State{})
	if !ok || ts.Active() != 6 {
		t.Fatalf("spawn after Kill ok=%v active=%d want true/6", ok, ts.Active())
	}
	t.Logf("FSV freeing a slot re-allowed a spawn; active back to %d", ts.Active())
}

// TestTwoThreadsWakeSameTickOrderFSV — two threads that wake on the same tick
// resume in (wakeTick, seq) order, i.e. arm/spawn order. Issue edge 2.
func TestTwoThreadsWakeSameTickOrderFSV(t *testing.T) {
	clk := uint32(0)
	d := ai.NewDomain()
	ctx := d.AddPlayer(1, staticView{self: 1, clock: &clk}, &recordingCommander{}, noopController{})
	ts := ai.NewThreadSet(ctx, threadBase)

	var order []ai.ThreadID
	spawn := func() ai.ThreadID {
		idp := new(ai.ThreadID)
		id, _ := ts.Spawn(func(s *sched.Scheduler, st sched.State) {
			order = append(order, *idp) // record resume order; then end
		}, sched.State{})
		*idp = id
		return id
	}
	t0 := spawn() // seq 1 (kickoff After(1))
	t1 := spawn() // seq 2
	clk++
	d.Tick(0) // both kicked off at tick 1; resume order must be seq order
	t.Logf("FSV two threads (t%d, t%d) wake tick 1; resume order=%v (want [%d %d])", t0, t1, order, t0, t1)
	if len(order) != 2 || order[0] != t0 || order[1] != t1 {
		t.Fatalf("resume order %v want [%d %d] (seq order)", order, t0, t1)
	}
}

// makeSleeper builds a thread body that logs its wake tick, sleeps `period`
// seconds, and ends after `runs` resumes. The run counter rides in State[0], so
// it persists across a save. *idp is filled in by the caller after Spawn.
func makeSleeper(ts *ai.ThreadSet, idp *ai.ThreadID, log *[]uint32, period float64, runs int) sched.Func {
	return func(s *sched.Scheduler, st sched.State) {
		*log = append(*log, s.Now())
		n := int(st[0]) + 1
		if n < runs {
			ts.Sleep(*idp, period, sched.State{int64(n)})
		} // else: return without re-arming → thread ends
	}
}

// TestThreadAcrossSaveFSV — a thread sleeping across a save resumes at the
// identical wake tick as an unbroken run. Issue edge 4. SoT = the wake-tick log.
func TestThreadAcrossSaveFSV(t *testing.T) {
	const period = 0.25 // → 5 ticks
	const runs = 3

	// --- unbroken control: run 14 ticks ---
	clk := uint32(0)
	cd := ai.NewDomain()
	cctx := cd.AddPlayer(1, staticView{self: 1, clock: &clk}, &recordingCommander{}, noopController{})
	cts := ai.NewThreadSet(cctx, threadBase)
	var ctrlLog []uint32
	cidp := new(ai.ThreadID)
	*cidp, _ = cts.Spawn(makeSleeper(cts, cidp, &ctrlLog, period, runs), sched.State{})
	for i := 0; i < 14; i++ {
		clk++
		cd.Tick(0)
	}
	t.Logf("FSV control wake ticks = %v", ctrlLog)

	// --- broken run: tick to 3, save mid-sleep, rebuild, Reinstall, Load ---
	clk = 0
	ad := ai.NewDomain()
	actx := ad.AddPlayer(1, staticView{self: 1, clock: &clk}, &recordingCommander{}, noopController{})
	ats := ai.NewThreadSet(actx, threadBase)
	var preLog []uint32
	aidp := new(ai.ThreadID)
	*aidp, _ = ats.Spawn(makeSleeper(ats, aidp, &preLog, period, runs), sched.State{})
	for i := 0; i < 3; i++ {
		clk++
		ad.Tick(0)
	}
	t.Logf("FSV broken pre-save wake ticks = %v; pending=%d (parked mid-sleep)", preLog, actx.PendingSleepers())
	if actx.PendingSleepers() != 1 {
		t.Fatalf("expected 1 parked thread at save, got %d", actx.PendingSleepers())
	}
	blob := ad.Save(nil)

	// Rebuild fresh, Reinstall the thread (same id, same body) BEFORE Load.
	bd := ai.NewDomain()
	bctx := bd.AddPlayer(1, staticView{self: 1, clock: &clk}, &recordingCommander{}, noopController{})
	bts := ai.NewThreadSet(bctx, threadBase)
	var postLog []uint32
	bidp := new(ai.ThreadID)
	*bidp = *aidp
	bts.Reinstall(*bidp, makeSleeper(bts, bidp, &postLog, period, runs))
	if err := bd.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	t.Logf("FSV post-restore: AI tick=%d pending=%d active=%d", bctx.Now(), bctx.PendingSleepers(), bts.Active())
	if bctx.Now() != 3 || bctx.PendingSleepers() != 1 || bts.Active() != 1 {
		t.Fatalf("post-restore tick=%d pending=%d active=%d want 3/1/1", bctx.Now(), bctx.PendingSleepers(), bts.Active())
	}
	for i := 3; i < 14; i++ {
		clk++
		bd.Tick(0)
	}
	combined := append(append([]uint32{}, preLog...), postLog...)
	t.Logf("FSV broken: pre=%v + post-restore=%v = %v vs control %v", preLog, postLog, combined, ctrlLog)
	if !equalU32(combined, ctrlLog) {
		t.Fatalf("thread wake ticks across save %v != unbroken %v", combined, ctrlLog)
	}
	t.Logf("FSV thread resumed at identical wake ticks across the save boundary")
}

// TestThreadWaitSignalFSV — a thread parked on a signal resumes only when the
// signal fires, then ends.
func TestThreadWaitSignalFSV(t *testing.T) {
	clk := uint32(0)
	const sig sched.EventID = 42
	d := ai.NewDomain()
	ctx := d.AddPlayer(1, staticView{self: 1, clock: &clk}, &recordingCommander{}, noopController{})
	ts := ai.NewThreadSet(ctx, threadBase)

	resumed := 0
	idp := new(ai.ThreadID)
	stage := 0
	*idp, _ = ts.Spawn(func(s *sched.Scheduler, st sched.State) {
		if stage == 0 {
			stage = 1
			ts.WaitSignal(*idp, sig, sched.State{}) // park on the signal
			return
		}
		resumed++ // resumed by the signal; then end
	}, sched.State{})

	clk++
	d.Tick(0) // first resume: parks on signal
	t.Logf("FSV after kickoff: waiters-on-sig=%d active=%d", ctx.PendingWaiters(sig), ts.Active())
	if ctx.PendingWaiters(sig) != 1 || ts.Active() != 1 {
		t.Fatalf("thread not parked on signal: waiters=%d active=%d", ctx.PendingWaiters(sig), ts.Active())
	}
	// Tick without firing — thread stays parked.
	clk++
	d.Tick(0)
	if resumed != 0 {
		t.Fatal("thread resumed without the signal")
	}
	// Fire the signal → resumes this tick.
	ts.Signal(sig)
	t.Logf("FSV after Signal: resumed=%d waiters=%d active=%d", resumed, ctx.PendingWaiters(sig), ts.Active())
	if resumed != 1 || ts.Active() != 0 {
		t.Fatalf("signal resume=%d active=%d want 1/0", resumed, ts.Active())
	}
	t.Logf("FSV wait-for-signal: parked until Signal, then resumed and ended")
}

func equalU32(a, b []uint32) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
