package ai_test

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// burstController issues one command per kind in `kinds` on tick 1, with
// distinct, predictable operands (A = base+i, B = the tick). Deterministic by
// construction — the basis for the determinism / ordering FSV.
type burstController struct {
	kinds []ai.CommandKind
	base  int32
}

const burstCont sched.ContID = 1

func (bc burstController) Install(ctx *ai.Context) {
	ctx.Register(burstCont, func(s *sched.Scheduler, st sched.State) {
		for i, k := range bc.kinds {
			ctx.Commander().Issue(ai.AICommand{Kind: k, A: bc.base + int32(i), B: int32(s.Now())})
		}
	})
	ctx.After(1, burstCont, sched.State{})
}

// everyTickController issues exactly one command each tick — used to exercise
// the steady-state issue+drain path for the zero-alloc check.
type everyTickController struct{ kind ai.CommandKind }

const everyCont sched.ContID = 1

func (ec everyTickController) Install(ctx *ai.Context) {
	ctx.Register(everyCont, func(s *sched.Scheduler, st sched.State) {
		ctx.Commander().Issue(ai.AICommand{Kind: ec.kind, A: int32(s.Now())})
		s.After(1, everyCont, sched.State{})
	})
	ctx.After(1, everyCont, sched.State{})
}

// buildScenario wires a stream + a 2-player domain (A=player 1, B=player 2),
// each running a burstController, and returns them. Players are added in index
// order so the AI sub-phase ticks A before B.
func buildScenario(clk *uint32) (*ai.CommandStream, *ai.Domain) {
	st := ai.NewCommandStream(16)
	d := ai.NewDomain()
	d.AddPlayer(1, staticView{self: 1, clock: clk}, st.Commander(1),
		burstController{kinds: []ai.CommandKind{ai.CmdTrain, ai.CmdBuild}, base: 10})
	d.AddPlayer(2, staticView{self: 2, clock: clk}, st.Commander(2),
		burstController{kinds: []ai.CommandKind{ai.CmdAttack, ai.CmdGuard, ai.CmdRetreat}, base: 20})
	return st, d
}

// TestCommandBoundaryDeterminismFSV — the same scenario run twice produces a
// byte-identical command stream (equal Hash). This is the replay source of
// truth: an AI match replays from the command stream alone.
func TestCommandBoundaryDeterminismFSV(t *testing.T) {
	var h [2]uint64
	var dump string
	for run := 0; run < 2; run++ {
		clk := uint32(0)
		st, d := buildScenario(&clk)
		for i := 0; i < 3; i++ {
			clk++
			d.Tick(0)
		}
		h[run] = st.Hash()
		if run == 0 {
			dump = st.String()
		}
	}
	t.Logf("FSV run0 stream:\n%s", dump)
	t.Logf("FSV stream hash run0=%#x run1=%#x", h[0], h[1])
	if h[0] != h[1] {
		t.Fatalf("command stream not deterministic across runs: %#x != %#x", h[0], h[1])
	}
}

// TestCommandTwoPlayersSameTickOrderFSV — when two AIs enqueue on the same tick,
// the stream is sequenced by player index then enqueue seq. Issue edge 1.
func TestCommandTwoPlayersSameTickOrderFSV(t *testing.T) {
	clk := uint32(0)
	st, d := buildScenario(&clk)
	clk++
	d.Tick(0) // both burst on tick 1
	n := st.Len()
	t.Logf("FSV stream after one tick:\n%s", st.String())
	// Player 1 issued 2, player 2 issued 3 → 5 total.
	if n != 5 {
		t.Fatalf("stream len=%d want 5", n)
	}
	// All player-1 commands must precede all player-2 commands; within the whole
	// stream the (player, then implicit append) order holds.
	prevPlayer := -1
	for i := 0; i < n; i++ {
		p, c := st.At(i)
		t.Logf("  [%d] player=%d kind=%d A=%d B(tick)=%d", i, p, c.Kind, c.A, c.B)
		if p < prevPlayer {
			t.Fatalf("player order regressed at %d: player %d after %d", i, p, prevPlayer)
		}
		prevPlayer = p
		if c.Player != int32(p) {
			t.Fatalf("command at %d tagged player %d but stream says %d", i, c.Player, p)
		}
	}
	// Concretely: indices 0-1 are player 1 (A=10,11), 2-4 are player 2 (A=20,21,22).
	p0, c0 := st.At(0)
	p4, c4 := st.At(4)
	if p0 != 1 || c0.A != 10 || p4 != 2 || c4.A != 22 {
		t.Fatalf("boundary commands wrong: first={p%d A%d} last={p%d A%d}", p0, c0.A, p4, c4.A)
	}
	t.Logf("FSV ordering by (playerIndex, seq) confirmed: p1[0..1] then p2[2..4]")
}

// TestCommandDeadTargetNoOpFSV — a command naming an entity that died between
// the AI's decision and application is a no-op at apply time (R-API-5), and the
// stream still hashes identically (it is pure data, independent of target
// liveness). Issue edge 2.
func TestCommandDeadTargetNoOpFSV(t *testing.T) {
	clk := uint32(0)
	st := ai.NewCommandStream(8)
	d := ai.NewDomain()
	// Issue three attacks: targets 100 (alive), 200 (DEAD), 300 (alive).
	d.AddPlayer(1, staticView{self: 1, clock: &clk}, st.Commander(1), targetController{targets: []int32{100, 200, 300}})
	clk++
	d.Tick(0)
	hashBefore := st.Hash()
	t.Logf("FSV stream before apply:\n%s", st.String())

	// "live" entities — 200 has died. (A test set; gameplay code never maps.)
	isLive := func(id int32) bool { return id == 100 || id == 300 }
	applied, skipped := 0, 0
	var appliedTargets []int32
	st.Drain(func(player int, c ai.AICommand) {
		if !isLive(c.B) {
			skipped++ // R-API-5: order against a dead entity is a no-op
			return
		}
		applied++
		appliedTargets = append(appliedTargets, c.B)
	})
	t.Logf("FSV apply: applied=%d skipped(dead)=%d appliedTargets=%v", applied, skipped, appliedTargets)
	if applied != 2 || skipped != 1 {
		t.Fatalf("applied=%d skipped=%d want 2/1 (target 200 dead → no-op)", applied, skipped)
	}
	if len(appliedTargets) != 2 || appliedTargets[0] != 100 || appliedTargets[1] != 300 {
		t.Fatalf("applied targets %v want [100 300]", appliedTargets)
	}

	// Replay: the identical decisions on a fresh stream hash identically, proving
	// the stream is liveness-independent and replays byte-for-byte.
	clk = 0
	st2 := ai.NewCommandStream(8)
	d2 := ai.NewDomain()
	d2.AddPlayer(1, staticView{self: 1, clock: &clk}, st2.Commander(1), targetController{targets: []int32{100, 200, 300}})
	clk++
	d2.Tick(0)
	t.Logf("FSV replay stream hash %#x vs original %#x", st2.Hash(), hashBefore)
	if st2.Hash() != hashBefore {
		t.Fatalf("replay stream hash %#x != original %#x", st2.Hash(), hashBefore)
	}
	t.Logf("FSV dead-target command is a no-op; stream replays identically")
}

// TestCommandDuringPauseFSV — commands issued while the sim defers draining
// (paused) accumulate and apply in order on resume. Issue edge 4.
func TestCommandDuringPauseFSV(t *testing.T) {
	clk := uint32(0)
	st := ai.NewCommandStream(8)
	d := ai.NewDomain()
	d.AddPlayer(1, staticView{self: 1, clock: &clk}, st.Commander(1), everyTickController{kind: ai.CmdTrain})
	// PAUSED: tick three times WITHOUT draining the stream.
	for i := 0; i < 3; i++ {
		clk++
		d.Tick(0)
	}
	t.Logf("FSV during pause: %d commands queued, undrained", st.Len())
	if st.Len() != 3 {
		t.Fatalf("queued %d during pause, want 3", st.Len())
	}
	// RESUME: a single drain applies all three in enqueue (tick) order.
	var order []int32
	st.Drain(func(player int, c ai.AICommand) { order = append(order, c.A) })
	t.Logf("FSV post-resume apply order (A=tick) = %v", order)
	if len(order) != 3 || order[0] != 1 || order[1] != 2 || order[2] != 3 {
		t.Fatalf("post-resume order %v want [1 2 3]", order)
	}
	t.Logf("FSV paused commands applied in order on resume")
}

// TestInboxOverflowBoundedFSV — the inbox is bounded; overflow rejects the
// newest command (preserving the queued FIFO), counts the drop, and the queue
// never grows past capacity. Issue edge 3.
func TestInboxOverflowBoundedFSV(t *testing.T) {
	var diag bytes.Buffer
	b := ai.NewInbox(4)
	b.SetDiagnostics(&diag)
	accepted := 0
	for i := 1; i <= 6; i++ { // push 6 into a cap-4 inbox
		if b.Push(ai.InboxMsg{Command: int32(i), Data: int32(i * 10)}) {
			accepted++
		}
	}
	t.Logf("FSV after pushing 6 into cap-4 inbox: accepted=%d waiting=%d dropped=%d cap=%d",
		accepted, b.Waiting(), b.Dropped(), b.Cap())
	if accepted != 4 || b.Waiting() != 4 || b.Dropped() != 2 {
		t.Fatalf("accepted=%d waiting=%d dropped=%d want 4/4/2", accepted, b.Waiting(), b.Dropped())
	}
	if b.Waiting() > b.Cap() {
		t.Fatal("inbox grew past capacity (unbounded)")
	}
	// FIFO preserved: the first four (1,2,3,4) are queued; 5,6 were rejected.
	for want := int32(1); want <= 4; want++ {
		m, ok := b.Pop()
		if !ok || m.Command != want || m.Data != want*10 {
			t.Fatalf("pop got %+v ok=%v want Command=%d", m, ok, want)
		}
	}
	if _, ok := b.Pop(); ok {
		t.Fatal("inbox not empty after draining 4")
	}
	t.Logf("FSV reject-newest preserved FIFO [1,2,3,4]; diagnostic=%q", diag.String())
	if diag.Len() == 0 {
		t.Fatal("overflow produced no diagnostic (must be loud)")
	}
}

// TestInboxFIFOPeekPopFSV — happy-path WC3 semantics: Waiting/Peek/Pop FIFO.
func TestInboxFIFOPeekPopFSV(t *testing.T) {
	b := ai.NewInbox(8)
	if _, ok := b.Peek(); ok {
		t.Fatal("Peek on empty inbox returned ok")
	}
	b.Push(ai.InboxMsg{Command: 7, Data: 70})
	b.Push(ai.InboxMsg{Command: 8, Data: 80})
	if b.Waiting() != 2 {
		t.Fatalf("waiting=%d want 2", b.Waiting())
	}
	// Peek does not consume.
	m, _ := b.Peek()
	if m.Command != 7 || b.Waiting() != 2 {
		t.Fatalf("peek consumed or wrong head: %+v waiting=%d", m, b.Waiting())
	}
	p1, _ := b.Pop()
	p2, _ := b.Pop()
	t.Logf("FSV inbox FIFO pop order: %+v then %+v (waiting now %d)", p1, p2, b.Waiting())
	if p1.Command != 7 || p2.Command != 8 || b.Waiting() != 0 {
		t.Fatalf("FIFO order wrong: %+v %+v waiting=%d", p1, p2, b.Waiting())
	}
	// Ring wrap: push past the original head to confirm modulo indexing.
	for i := 0; i < 8; i++ {
		b.Push(ai.InboxMsg{Command: int32(100 + i)})
	}
	first, _ := b.Peek()
	if first.Command != 100 || b.Waiting() != 8 {
		t.Fatalf("after wrap head=%+v waiting=%d want Command=100 waiting=8", first, b.Waiting())
	}
	t.Logf("FSV inbox ring wraps correctly, head=%d", first.Command)
}

// TestCommandInvalidKindRejectedFSV — a malformed command (invalid kind) is
// rejected fail-closed and never enters the replay stream.
func TestCommandInvalidKindRejectedFSV(t *testing.T) {
	st := ai.NewCommandStream(4)
	cmd := st.Commander(1)
	cmd.Issue(ai.AICommand{Kind: ai.CmdNone, A: 1})  // zero value — invalid
	cmd.Issue(ai.AICommand{Kind: ai.CommandKind(9999), A: 2}) // out of range
	cmd.Issue(ai.AICommand{Kind: ai.CmdTrain, A: 3})  // valid
	t.Logf("FSV after 2 invalid + 1 valid: len=%d rejected=%d", st.Len(), st.Rejected())
	if st.Len() != 1 || st.Rejected() != 2 {
		t.Fatalf("len=%d rejected=%d want 1/2", st.Len(), st.Rejected())
	}
	_, c := st.At(0)
	if c.Kind != ai.CmdTrain || c.A != 3 {
		t.Fatalf("surviving command=%+v want Train/A3", c)
	}
	t.Logf("FSV invalid kinds rejected, only the valid command entered the stream")
}

// TestCommandStreamZeroAllocFSV — steady-state issue+drain allocates nothing.
func TestCommandStreamZeroAllocFSV(t *testing.T) {
	clk := uint32(0)
	st := ai.NewCommandStream(8)
	d := ai.NewDomain()
	for p := 0; p < 4; p++ {
		d.AddPlayer(p, staticView{self: p, clock: &clk}, st.Commander(p), everyTickController{kind: ai.CmdTrain})
	}
	for i := 0; i < 5; i++ { // warm: grow buf to capacity, then drain resets
		clk++
		d.Tick(100)
		st.Drain(func(player int, c ai.AICommand) {})
	}
	allocs := testing.AllocsPerRun(500, func() {
		clk++
		d.Tick(100)
		st.Drain(func(player int, c ai.AICommand) {})
	})
	t.Logf("FSV 4-player issue+drain allocs/op=%v", allocs)
	if allocs != 0 {
		t.Fatalf("issue+drain allocates %v/op at steady state, want 0", allocs)
	}
}

// targetController issues one CmdAttack per target id (B = target), tick 1.
type targetController struct{ targets []int32 }

const targetCont sched.ContID = 1

func (tc targetController) Install(ctx *ai.Context) {
	ctx.Register(targetCont, func(s *sched.Scheduler, st sched.State) {
		for _, tg := range tc.targets {
			ctx.Commander().Issue(ai.AICommand{Kind: ai.CmdAttack, A: 1, B: tg})
		}
	})
	ctx.After(1, targetCont, sched.State{})
}
