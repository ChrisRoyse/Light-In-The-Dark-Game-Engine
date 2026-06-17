package ai_test

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// --- test capabilities (the sim-side contract; the command sink is the SoT) ---

// recordingCommander is the source of truth for everything an AI emits: the
// ordered command stream. Real sims drain this into phase-1's command queue;
// here we read it directly to verify what the AI decided.
type recordingCommander struct{ cmds []ai.AICommand }

func (r *recordingCommander) Issue(c ai.AICommand) { r.cmds = append(r.cmds, c) }

// staticView is a fixed read-only view. UnitCount returns a per-player constant
// so a controller's reads are observable but never mutate anything.
type staticView struct {
	self  int
	clock *uint32 // shared sim clock the AI reads through Now()
}

func (v staticView) Now() uint32                    { return *v.clock }
func (v staticView) Self() int                      { return v.self }
func (v staticView) UnitCount(player, typeID int) int { return player*100 + typeID }

// --- test controllers (receive ONLY *ai.Context — compile-time isolation) ---

const periodCont sched.ContID = 1

// periodicController issues one CmdTrain every `period` ticks, carrying a
// per-resume counter in the suspension State (which proves State is per-context
// and survives a save). A = the running count, B = the AI scheduler tick it
// fired on.
type periodicController struct {
	period uint32
	train  int32 // unitTypeID to put in command A-adjacent fields
}

func (pc periodicController) Install(ctx *ai.Context) {
	ctx.Register(periodCont, func(s *sched.Scheduler, st sched.State) {
		n := st[0] + 1
		ctx.Commander().Issue(ai.AICommand{
			Kind:   ai.CmdTrain,
			Player: int32(ctx.Player()),
			A:      pc.train,
			B:      int32(s.Now()),
		})
		s.After(pc.period, periodCont, sched.State{n}) // suspend again
	})
	ctx.After(pc.period, periodCont, sched.State{0}) // first fire at t=period
}

const fanoutCont sched.ContID = 1

// fanoutController schedules n continuations that all wake on the next tick —
// a deliberately heavy decision tick, to exercise the slice watchdog.
type fanoutController struct{ n int }

func (f fanoutController) Install(ctx *ai.Context) {
	ctx.Register(fanoutCont, func(s *sched.Scheduler, st sched.State) {})
	for i := 0; i < f.n; i++ {
		ctx.After(1, fanoutCont, sched.State{int64(i)})
	}
}

// noopController registers nothing — an AI player that simply idles.
type noopController struct{}

func (noopController) Install(ctx *ai.Context) {}

// ---------------------------------------------------------------------------

// TestDomainTickCommandStreamFSV — X+X=Y on the command stream. A period-3
// controller must emit exactly at AI ticks 3,6,9,12 over 12 ticks, with the
// running counter 1,2,3,4. SoT = the recorded command stream.
func TestDomainTickCommandStreamFSV(t *testing.T) {
	clk := uint32(0)
	cmd := &recordingCommander{}
	d := ai.NewDomain()
	d.AddPlayer(2, staticView{self: 2, clock: &clk}, cmd, periodicController{period: 3, train: 77})

	t.Logf("FSV before: pending sleepers=%d, commands=%d", d.Context(2).PendingSleepers(), len(cmd.cmds))
	if d.Context(2).PendingSleepers() != 1 {
		t.Fatalf("after install want 1 pending sleeper (the kickoff), got %d", d.Context(2).PendingSleepers())
	}
	for i := 0; i < 12; i++ {
		clk++
		d.Tick(0)
	}
	// Expected: one command per period at ticks 3,6,9,12, counters 1..4.
	wantTicks := []int32{3, 6, 9, 12}
	wantN := []int32{1, 2, 3, 4}
	t.Logf("FSV after 12 ticks: %d commands emitted", len(cmd.cmds))
	if len(cmd.cmds) != len(wantTicks) {
		t.Fatalf("emitted %d commands, want %d: %+v", len(cmd.cmds), len(wantTicks), cmd.cmds)
	}
	for i, c := range cmd.cmds {
		t.Logf("  cmd[%d] = {Kind:%d Player:%d A:%d B(tick):%d} want tick=%d n=%d",
			i, c.Kind, c.Player, c.A, c.B, wantTicks[i], wantN[i])
		if c.Kind != ai.CmdTrain || c.Player != 2 || c.A != 77 || c.B != wantTicks[i] {
			t.Fatalf("cmd[%d]=%+v want Train/p2/A77/tick%d", i, c, wantTicks[i])
		}
	}
}

// TestIsolationTwoContextsIndependentFSV — two AI players advance independent
// schedulers and independent command streams; one player's State never leaks
// into the other's. Edge: two AI players' contexts (issue edge 2).
func TestIsolationTwoContextsIndependentFSV(t *testing.T) {
	clk := uint32(0)
	cmdA := &recordingCommander{}
	cmdB := &recordingCommander{}
	d := ai.NewDomain()
	// A fires every 2 ticks, B every 5 ticks.
	d.AddPlayer(1, staticView{self: 1, clock: &clk}, cmdA, periodicController{period: 2, train: 10})
	d.AddPlayer(2, staticView{self: 2, clock: &clk}, cmdB, periodicController{period: 5, train: 20})

	for i := 0; i < 10; i++ {
		clk++
		d.Tick(0)
	}
	// A: ticks 2,4,6,8,10 → 5 cmds. B: ticks 5,10 → 2 cmds.
	t.Logf("FSV player1 cmds=%d (want 5), player2 cmds=%d (want 2)", len(cmdA.cmds), len(cmdB.cmds))
	if len(cmdA.cmds) != 5 || len(cmdB.cmds) != 2 {
		t.Fatalf("p1=%d p2=%d want 5/2", len(cmdA.cmds), len(cmdB.cmds))
	}
	// Every command carries its own player and its own train type — no bleed.
	for _, c := range cmdA.cmds {
		if c.Player != 1 || c.A != 10 {
			t.Fatalf("player1 stream contaminated: %+v", c)
		}
	}
	for _, c := range cmdB.cmds {
		if c.Player != 2 || c.A != 20 {
			t.Fatalf("player2 stream contaminated: %+v", c)
		}
	}
	// The contexts and their capabilities are distinct instances — there is no
	// shared object an AI script could reach across the boundary.
	if d.Context(1) == d.Context(2) {
		t.Fatal("two players share one context")
	}
	if d.Context(1).View().Self() == d.Context(2).View().Self() {
		t.Fatal("two players share a view identity")
	}
	t.Logf("FSV contexts distinct: p1.Self=%d p2.Self=%d; B counter at %d, A counter at %d (independent)",
		d.Context(1).View().Self(), d.Context(2).View().Self(),
		cmdB.cmds[len(cmdB.cmds)-1].A, cmdA.cmds[len(cmdA.cmds)-1].A)
}

// TestIsolationMapStateUntouchedFSV — the AI domain has zero side effect on a
// separate (map-script) scheduler. Run a map scheduler 10 ticks (a) alone, (b)
// alongside a live AI domain, (c) alongside a disabled AI domain — the map
// scheduler's serialized bytes must be byte-identical in all three. Proves
// R-EXEC-3 isolation and disable-safety (issue edges 2 & 3).
func TestIsolationMapStateUntouchedFSV(t *testing.T) {
	// A tiny map-script that re-arms itself every tick, so its scheduler state
	// is non-trivial and would change if anything perturbed it.
	const mapCont sched.ContID = 1
	buildMap := func() *sched.Scheduler {
		s := sched.New()
		s.Register(mapCont, func(sc *sched.Scheduler, st sched.State) {
			sc.After(1, mapCont, sched.State{st[0] + 1})
		})
		s.After(1, mapCont, sched.State{0})
		return s
	}
	run := func(withAI, disabled bool) []byte {
		mapS := buildMap()
		clk := uint32(0)
		var d *ai.Domain
		if withAI {
			d = ai.NewDomain()
			d.AddPlayer(1, staticView{self: 1, clock: &clk}, &recordingCommander{}, periodicController{period: 2, train: 9})
			if disabled {
				d.Disable(1)
			}
		}
		for i := 0; i < 10; i++ {
			clk++
			mapS.Step()
			if d != nil {
				d.Tick(0)
			}
		}
		return mapS.Save(nil)
	}
	alone := run(false, false)
	withAI := run(true, false)
	withDisabledAI := run(true, true)
	t.Logf("FSV map-scheduler save bytes: alone=%d, +AI=%d, +disabledAI=%d", len(alone), len(withAI), len(withDisabledAI))
	if !bytes.Equal(alone, withAI) {
		t.Fatalf("map scheduler state differs with AI domain present (isolation breach)")
	}
	if !bytes.Equal(alone, withDisabledAI) {
		t.Fatalf("map scheduler state differs with AI domain disabled (disable-safety breach)")
	}
	t.Logf("FSV map state byte-identical across all three runs — AI domain has no hook into map state")
}

// TestDomainSerializeMidWaitRoundTripFSV — an AI job suspended mid-wait survives
// a save: restore into a fresh domain (controllers re-installed) and the resumed
// job fires at the identical tick and emits the identical command. Re-saving the
// restored domain yields byte-identical bytes. Issue edge 1.
func TestDomainSerializeMidWaitRoundTripFSV(t *testing.T) {
	clk := uint32(0)
	// --- unbroken control run: 8 ticks, period 3 → cmds at 3,6 ---
	ctrlCmd := &recordingCommander{}
	ctrl := ai.NewDomain()
	ctrl.AddPlayer(1, staticView{self: 1, clock: &clk}, ctrlCmd, periodicController{period: 3, train: 5})
	for i := 0; i < 8; i++ {
		clk++
		ctrl.Tick(0)
	}

	// --- broken run: tick 4, save mid-wait, restore, tick to 8 ---
	clk = 0
	aCmd := &recordingCommander{}
	a := ai.NewDomain()
	a.AddPlayer(1, staticView{self: 1, clock: &clk}, aCmd, periodicController{period: 3, train: 5})
	for i := 0; i < 4; i++ {
		clk++
		a.Tick(0)
	}
	// SoT before save: pending sleeper exists (job parked, waiting for tick 6).
	t.Logf("FSV at save: AI tick=%d pending=%d cmds-so-far=%d", a.Context(1).Now(), a.Context(1).PendingSleepers(), len(aCmd.cmds))
	if a.Context(1).PendingSleepers() != 1 {
		t.Fatalf("expected 1 parked job at save, got %d", a.Context(1).PendingSleepers())
	}
	blob := a.Save(nil)

	// Fresh domain, same controller re-installed, then Load.
	bCmd := &recordingCommander{}
	b := ai.NewDomain()
	b.AddPlayer(1, staticView{self: 1, clock: &clk}, bCmd, periodicController{period: 3, train: 5})
	if err := b.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	t.Logf("FSV post-restore: AI tick=%d pending=%d (want tick=4, pending=1)", b.Context(1).Now(), b.Context(1).PendingSleepers())
	if b.Context(1).Now() != 4 || b.Context(1).PendingSleepers() != 1 {
		t.Fatalf("post-restore tick=%d pending=%d want 4/1", b.Context(1).Now(), b.Context(1).PendingSleepers())
	}
	// Re-save must be byte-identical (canonical round-trip).
	if !bytes.Equal(blob, b.Save(nil)) {
		t.Fatal("re-save of restored domain not byte-identical")
	}
	// Advance the restored domain ticks 5..8; the parked job must fire at tick 6.
	for i := 4; i < 8; i++ {
		clk++
		b.Tick(0)
	}
	// Control emitted at ticks 3,6; broken emitted 3 before save, then 6 after.
	t.Logf("FSV control cmds=%v", cmdTicks(ctrlCmd))
	t.Logf("FSV restored cmds (pre-save %v + post-restore %v)", cmdTicks(aCmd), cmdTicks(bCmd))
	gotAll := append(append([]int32{}, cmdTicks(aCmd)...), cmdTicks(bCmd)...)
	if !equalI32(gotAll, cmdTicks(ctrlCmd)) {
		t.Fatalf("restored fire-ticks %v != unbroken %v", gotAll, cmdTicks(ctrlCmd))
	}
	t.Logf("FSV mid-wait job resumed at identical tick across the save boundary")
}

// TestBudgetOverrunWatchdogFSV — a player whose decision tick resumes more
// continuations than its slice trips the watchdog with a loud diagnostic, but
// the tick still completes (every due continuation ran; a second, in-budget
// player is still ticked). Issue edge 4.
func TestBudgetOverrunWatchdogFSV(t *testing.T) {
	clk := uint32(0)
	var diag bytes.Buffer
	cmdB := &recordingCommander{}
	d := ai.NewDomain()
	d.SetDiagnostics(&diag)
	d.AddPlayer(7, staticView{self: 7, clock: &clk}, &recordingCommander{}, fanoutController{n: 50})
	d.AddPlayer(8, staticView{self: 8, clock: &clk}, cmdB, periodicController{period: 1, train: 3})

	t.Logf("FSV before tick: overrun=%v", d.Overrun())
	clk++
	total := d.Tick(10) // budget 10; player 7 will resume 50
	t.Logf("FSV after tick: total resumes=%d overrun=%v player=%d", total, d.Overrun(), d.OverrunPlayer())
	if !d.Overrun() || d.OverrunPlayer() != 7 {
		t.Fatalf("watchdog did not trip for player 7 (overrun=%v player=%d)", d.Overrun(), d.OverrunPlayer())
	}
	res, bud := d.OverrunDetail()
	t.Logf("FSV overrun detail: resumes=%d budget=%d; diagnostic=%q", res, bud, diag.String())
	if res != 50 || bud != 10 {
		t.Fatalf("overrun detail resumes=%d budget=%d want 50/10", res, bud)
	}
	if diag.Len() == 0 {
		t.Fatal("watchdog produced no diagnostic (must be loud)")
	}
	// Tick still COMPLETED: the in-budget player 8 was ticked and emitted.
	if len(cmdB.cmds) != 1 {
		t.Fatalf("player 8 not ticked after player 7 overran: %d cmds (sim tick must complete)", len(cmdB.cmds))
	}
	// All 50 of player 7's due continuations actually ran (no preempt/loss).
	if total < 50 {
		t.Fatalf("total resumes=%d, player 7's 50 due continuations did not all run", total)
	}
	t.Logf("FSV overrun reported, tick completed, in-budget player still served")
}

// TestLoadFailClosedFSV — a corrupt blob leaves the domain byte-identical to
// before the failed Load (atomic rollback, fail-closed). Edge: malformed input.
func TestLoadFailClosedFSV(t *testing.T) {
	clk := uint32(0)
	d := ai.NewDomain()
	d.AddPlayer(1, staticView{self: 1, clock: &clk}, &recordingCommander{}, periodicController{period: 2, train: 1})
	for i := 0; i < 3; i++ {
		clk++
		d.Tick(0)
	}
	before := d.Save(nil)

	// (a) bad magic.
	if err := d.Load([]byte("not a valid ai blob at all")); err == nil {
		t.Fatal("Load accepted a bad-magic blob")
	}
	// (b) truncated valid blob.
	good := d.Save(nil)
	if err := d.Load(good[:len(good)-5]); err == nil {
		t.Fatal("Load accepted a truncated blob")
	}
	// (c) wrong player set: corrupt the player id in an otherwise-valid blob.
	corrupt := append([]byte{}, good...)
	corrupt[8+2+4] ^= 0xFF // flip the first player-id byte
	if err := d.Load(corrupt); err == nil {
		t.Fatal("Load accepted a blob referencing an unknown player")
	}
	after := d.Save(nil)
	t.Logf("FSV domain save bytes before=%d after-failed-loads=%d", len(before), len(after))
	if !bytes.Equal(before, after) {
		t.Fatal("failed Load mutated the domain (not fail-closed)")
	}
	t.Logf("FSV three rejected loads (bad magic / truncated / unknown player); domain unchanged")
}

// TestDomainTickZeroAllocFSV — steady-state ticking allocates nothing.
func TestDomainTickZeroAllocFSV(t *testing.T) {
	clk := uint32(0)
	d := ai.NewDomain()
	for p := 0; p < 4; p++ {
		d.AddPlayer(p, staticView{self: p, clock: &clk}, &recordingCommander{}, periodicController{period: 3, train: int32(p)})
	}
	for i := 0; i < 5; i++ { // warm
		clk++
		d.Tick(100)
	}
	allocs := testing.AllocsPerRun(500, func() {
		clk++
		d.Tick(100)
	})
	t.Logf("FSV 4-player domain Tick allocs/op=%v", allocs)
	if allocs != 0 {
		t.Fatalf("Tick allocates %v/op at steady state, want 0", allocs)
	}
}

// --- helpers ---

func cmdTicks(r *recordingCommander) []int32 {
	out := make([]int32, len(r.cmds))
	for i, c := range r.cmds {
		out[i] = c.B
	}
	return out
}

func equalI32(a, b []int32) bool {
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
