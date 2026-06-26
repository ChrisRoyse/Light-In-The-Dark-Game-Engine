package sim

// #306 patrol / follow / hold behavior tests. SoT = tick-stamped
// order/attack-state traces, position dumps, and path-request
// (re-target) counts.

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// behWorld: damage matrix bound, deterministic acquisition cadence 1.
func behWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	w.SetAcquireInterval(1)
	return w
}

func posOf(w *World, id EntityID) fixed.Vec2 {
	return w.Transforms.Pos[w.Transforms.Row(id)]
}

func setPos(w *World, id EntityID, p fixed.Vec2) {
	w.Transforms.Pos[w.Transforms.Row(id)] = p
	w.bucketReconcile()
}

// Edge 1: a patroller pings A↔B; an enemy off the far endpoint is
// engaged and chased to the leash edge. SoT = the patrol flags and the
// unit position. At the leash break the target is dropped; the unit
// walks back onto the segment (verified by segDistFarther==false at the
// returning→0 transition) and resumes the ping-pong.
func TestPatrolLeashAndResume(t *testing.T) {
	w := behWorld(t)
	w.SetPatrolLeash(300)
	a := atkUnit(t, w, 0, pt2(1000, 1000), 8*fixed.One)
	arm(t, w, a, 0, 0)
	w.Combats.AcquisitionRange[w.Combats.Row(a)] = 600 * fixed.One
	// patrol from (1000,1000) to (2000,1000)
	if !w.IssueOrder(a, Order{Kind: OrderPatrol, Point: pt2(2000, 1000)}, false) {
		t.Fatal("issue patrol")
	}
	w.Step()
	pr := w.Patrol.Row(a)
	if pr == -1 || w.Patrol.A[pr] != pt2(1000, 1000) || w.Patrol.B[pr] != pt2(2000, 1000) {
		t.Fatalf("patrol endpoints wrong: %+v", w.Patrol)
	}
	t.Logf("t%d patrol A=%v B=%v flags=%03b pos=(%d,%d)", w.Tick(),
		w.Patrol.A[pr], w.Patrol.B[pr], w.Patrol.Flags[pr], posOf(w, a).X.Floor(), posOf(w, a).Y.Floor())

	// enemy off the segment near B — engages, chases off-segment, leashes.
	enemy := atkUnit(t, w, 1, pt2(1900, 1450), 0)
	leashed := false
	var leashPos fixed.Vec2
	for i := 0; i < 220 && !leashed; i++ {
		w.Step()
		if w.Patrol.Flags[pr]&patrolReturning != 0 {
			leashed = true
			leashPos = posOf(w, a)
			leashDist := w.segDistFarther(leashPos, w.Patrol.A[pr], w.Patrol.B[pr], w.patrolLeashDist())
			t.Logf("t%d LEASHED pos=(%d,%d) past-leash=%v target=%d", w.Tick(),
				leashPos.X.Floor(), leashPos.Y.Floor(), leashDist, w.Combats.Target[w.Combats.Row(a)])
		}
	}
	if !leashed {
		t.Fatal("patroller never leashed off the enemy")
	}
	// chase target cleared at the leash break
	if w.Combats.Target[w.Combats.Row(a)] != 0 {
		t.Fatalf("still holding the leashed target after the break")
	}
	// remove the enemy's pull so the return is observable without re-engage.
	setPos(w, enemy, pt2(9000, 9000))

	// watch the returning flag fall 1→0 — that is the resume tick.
	returned := false
	for i := 0; i < 220 && !returned; i++ {
		prev := w.Patrol.Flags[pr] & patrolReturning
		w.Step()
		if prev != 0 && w.Patrol.Flags[pr]&patrolReturning == 0 {
			returned = true
			off := w.segDistFarther(posOf(w, a), w.Patrol.A[pr], w.Patrol.B[pr], fixed.FromInt(48))
			t.Logf("t%d RESUMED pos=(%d,%d) off-segment>48=%v", w.Tick(),
				posOf(w, a).X.Floor(), posOf(w, a).Y.Floor(), off)
			if off {
				t.Fatalf("resumed off the segment: pos=(%d,%d)", posOf(w, a).X.Floor(), posOf(w, a).Y.Floor())
			}
		}
	}
	if !returned {
		t.Fatal("patroller never returned to its segment")
	}
	// ping-pong is alive again: it eventually reaches an endpoint and flips.
	flipped := false
	startLeg := w.Patrol.Flags[pr] & patrolLegToA
	for i := 0; i < 600 && !flipped; i++ {
		w.Step()
		if w.Patrol.Flags[pr]&patrolLegToA != startLeg {
			flipped = true
		}
	}
	t.Logf("t%d ping-pong leg flipped=%v pos=(%d,%d)", w.Tick(), flipped,
		posOf(w, a).X.Floor(), posOf(w, a).Y.Floor())
	if !flipped {
		t.Fatal("patrol did not resume ping-pong after returning")
	}
}

// Edge 2: a follow target that drifts in small sub-threshold steps must
// NOT re-path every tick (the thrash hysteresis prevents). SoT = the
// follower's Movements.Target — it only changes when the order issues a
// fresh path. With a 96-unit repath gate, ~5 units/tick of drift crosses
// the gate roughly once per 20 ticks, so a 30-tick walk costs ≤2
// re-targets instead of 30.
func TestFollowRepathHysteresis(t *testing.T) {
	w := behWorld(t)
	w.SetFollowRepath(96)
	follower := atkUnit(t, w, 0, pt2(1000, 1000), 4*fixed.One)
	target := atkUnit(t, w, 0, pt2(1100, 1000), 0)
	if !w.IssueOrder(follower, Order{Kind: OrderFollow, Target: target}, false) {
		t.Fatal("issue follow")
	}
	mr := w.Movements.Row(follower)
	retargets := 0
	prev := fixed.Vec2{}
	tick := func() {
		w.Step()
		cur := w.Movements.Target[mr]
		if cur != prev {
			retargets++
			prev = cur
		}
	}
	tick() // fresh: 1st path
	base := retargets
	// drift the target in 5-unit sub-threshold steps for 30 ticks.
	tx := int32(1100)
	for i := 0; i < 30; i++ {
		tx += 5
		setPos(w, target, pt2(tx, 1000))
		tick()
	}
	driftRepaths := retargets - base
	t.Logf("re-targets over 30 sub-threshold drift ticks (5u each): %d (anchor moved %d units)", driftRepaths, tx-1100)
	if base != 1 {
		t.Fatalf("fresh follow did not path exactly once: %d", base)
	}
	if driftRepaths > 2 {
		t.Fatalf("hysteresis broke: %d re-paths over 30 sub-threshold drift ticks (want ≤2)", driftRepaths)
	}
	// a single big teleport past the gate must produce at least one re-path.
	pre := retargets
	setPos(w, target, pt2(5000, 5000))
	tick()
	t.Logf("re-targets after teleport: +%d", retargets-pre)
	if retargets-pre < 1 {
		t.Fatal("teleport past the gate did not re-path")
	}
	// target death stops the follow (WC3). Death resolves at cleanup
	// (phase 7), so the follow observes a gone target on the NEXT tick.
	var done []Event
	w.RegisterHandler(hA, func(_ *World, e Event) { done = append(done, e) })
	w.Subscribe(EvOrderDone, hA)
	w.KillUnit(target)
	w.Step() // cleanup destroys the target this tick
	w.Step() // driveFollow sees it gone → completeOrder
	t.Logf("after target death: order kind=%d (want Stop=%d), done events=%d, target alive=%v",
		w.Orders.Kind[w.Orders.Row(follower)], OrderStop, len(done), w.Ents.Alive(target))
	if w.Ents.Alive(target) {
		t.Fatal("target not destroyed at cleanup")
	}
	if w.Orders.Kind[w.Orders.Row(follower)] != OrderStop {
		t.Fatal("follow did not stop on target death")
	}
	if len(done) == 0 {
		t.Fatal("no EvOrderDone emitted when follow target died")
	}
}

// Edge 3: hold-position never repositions; it fires only on a target
// that walks into range.
func TestHoldNoChaseFiresInRange(t *testing.T) {
	w := behWorld(t)
	holder := atkUnit(t, w, 0, pt2(1000, 1000), 8*fixed.One)
	arm(t, w, holder, 0, 0) // range 100
	w.Combats.AcquisitionRange[w.Combats.Row(holder)] = 600 * fixed.One
	enemy := atkUnit(t, w, 1, pt2(1200, 1000), 0) // 200 away: out of weapon range, in acq range
	if !w.IssueOrder(holder, Order{Kind: OrderHold}, false) {
		t.Fatal("issue hold")
	}
	var fired bool
	w.OnAttackTransition = func(_ uint32, id EntityID, _ int, _, to uint8) {
		if id == holder && to == AtkBackswing {
			fired = true
		}
	}
	start := posOf(w, holder)
	for i := 0; i < 15; i++ {
		w.Step()
	}
	t.Logf("t%d hold pos=(%d,%d) start=(%d,%d) fired=%v (enemy at range 200, weapon range 100)",
		w.Tick(), posOf(w, holder).X.Floor(), posOf(w, holder).Y.Floor(),
		start.X.Floor(), start.Y.Floor(), fired)
	if posOf(w, holder) != start {
		t.Fatal("hold unit repositioned toward an out-of-range target")
	}
	if fired {
		t.Fatal("hold unit fired at an out-of-range target")
	}
	// enemy steps into weapon range — now it fires, still no move
	setPos(w, enemy, pt2(1080, 1000)) // 80 away: inside range 100
	for i := 0; i < 15; i++ {
		w.Step()
	}
	t.Logf("t%d after enemy enters range: hold pos=(%d,%d) fired=%v", w.Tick(),
		posOf(w, holder).X.Floor(), posOf(w, holder).Y.Floor(), fired)
	if posOf(w, holder) != start {
		t.Fatal("hold unit moved when the target entered range")
	}
	if !fired {
		t.Fatal("hold unit never fired at an in-range target")
	}
}

// Edge 4: right-click an ally resolves to the follow opcode; the
// resolved order (not the raw click) is what enters the queue and the
// replay stream.
func TestSmartAllyResolvesFollow(t *testing.T) {
	w, table := smartWorld(t)
	_ = table
	fighter := smartUnit(t, w, 0, 0, 0)
	ally := smartUnit(t, w, 0, 0, 1) // same team
	cmds, ok := w.ResolveSmartClass(data.TCAlly, []EntityID{fighter}, ally, fixed.Vec2{})
	if !ok || len(cmds) != 1 || cmds[0].Opcode != OpFollow {
		t.Fatalf("ally smart resolve: ok=%v cmds=%+v", ok, cmds)
	}
	t.Logf("right-click ally → opcode %d (OpFollow=%d) target=%d", cmds[0].Opcode, OpFollow, cmds[0].Target.Index())
	// the resolved opcode round-trips through the command wire format
	rec := CommandRecord{Version: CommandVersion, Opcode: OpFollow, UnitCount: 1}
	rec.Units[0] = fighter
	rec.Target = ally
	buf, ok := AppendEncode(nil, &rec)
	if !ok {
		t.Fatal("encode")
	}
	var dec CommandRecord
	n, ok := DecodeCommand(buf, &dec)
	t.Logf("wire: %d bytes, decoded opcode=%d target=%d", n, dec.Opcode, dec.Target.Index())
	if !ok || dec.Opcode != OpFollow || dec.Target != ally {
		t.Fatalf("wire round-trip: ok=%v opcode=%d", ok, dec.Opcode)
	}
}

// Edge 5: right-click a ground item resolves to get-item; with a full
// inventory the take fails deterministically at adjacency (the order
// completes failed, the item stays grounded).
func TestPatrolPickupFullInventoryDeterministic(t *testing.T) {
	// (uint8 order-kind after the order resolves, item grounded?, fail event?)
	build := func(t *testing.T) (uint8, EntityID, bool, bool) {
		w := itemWorld(t)
		u := itemUnit(t, w, pt2(500, 500))
		for i := 0; i < InventorySlots; i++ { // fill all six slots
			it, ok := w.SpawnItem(itClaws, pt2(490+int32(i), 490))
			if !ok || w.PickupItem(u, it) != ItemOK {
				t.Fatalf("seed slot %d failed", i)
			}
		}
		ground, ok := w.SpawnItem(itStone, pt2(520, 500)) // adjacent (dist 20 < reach 128)
		if !ok {
			t.Fatal("spawn ground item")
		}
		var done []Event
		w.RegisterHandler(hA, func(_ *World, e Event) { done = append(done, e) })
		w.Subscribe(EvOrderDone, hA)
		if !w.IssueOrder(u, Order{Kind: OrderPickup, Target: ground}, false) {
			t.Fatal("issue pickup")
		}
		w.Step()
		gr := w.Items.Row(ground)
		grounded := gr != -1 && w.Items.Carrier[gr] == 0 && w.Transforms.Row(ground) != -1
		failEvt := len(done) == 1 && done[0].Arg == 0
		t.Logf("order=%d done=%d grounded=%v carrier=%d", w.Orders.Kind[w.Orders.Row(u)], len(done), grounded, w.Items.Carrier[gr])
		return w.Orders.Kind[w.Orders.Row(u)], ground, grounded, failEvt
	}
	k1, g1, grounded1, fail1 := build(t)
	k2, g2, grounded2, fail2 := build(t)
	if !grounded1 || !grounded2 {
		t.Fatal("full-inventory pickup consumed the ground item instead of leaving it")
	}
	if !fail1 || !fail2 {
		t.Fatal("full-inventory pickup did not emit a single failure EvOrderDone")
	}
	if k1 != OrderStop || k2 != OrderStop {
		t.Fatalf("failed pickup did not fall through to Stop: k1=%d k2=%d", k1, k2)
	}
	if k1 != k2 || g1.Index() != g2.Index() {
		t.Fatalf("nondeterministic outcome: k1=%d k2=%d", k1, k2)
	}
}

// Determinism + save v7: a patrol mid-leg twins and round-trips.
func TestPatrolDeterminismAndSave(t *testing.T) {
	build := func() *World {
		w := NewWorld(Caps{Units: 16})
		w.BindDamageMatrix(atkMatrix)
		w.SetAcquireInterval(1)
		a := atkUnit(t, w, 0, pt2(1000, 1000), 8*fixed.One)
		arm(t, w, a, 0, 0)
		w.IssueOrder(a, Order{Kind: OrderPatrol, Point: pt2(1300, 1000)}, false)
		f := atkUnit(t, w, 0, pt2(2000, 2000), 4*fixed.One)
		tgt := atkUnit(t, w, 0, pt2(2100, 2000), 0)
		w.IssueOrder(f, Order{Kind: OrderFollow, Target: tgt}, false)
		for i := 0; i < 30; i++ {
			w.Step()
		}
		return w
	}
	a, b := build(), build()
	var sa, sb statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa)
	b.HashState(NewHashRegistry(), &sb)
	t.Logf("twin A=%016x B=%016x", sa.Top, sb.Top)
	if sa.Top != sb.Top {
		t.Fatal("twin divergence")
	}
	// patrol flags are state
	a.Patrol.Flags[0] ^= patrolLegToA
	var sa2 statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("patrol flag mutation invisible to the hash")
	}
	a.Patrol.Flags[0] ^= patrolLegToA

	var buf bytes.Buffer
	if err := a.SaveState(&buf, 9); err != nil {
		t.Fatal(err)
	}
	w2 := NewWorld(Caps{Units: 16})
	w2.BindDamageMatrix(atkMatrix)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 9); err != nil {
		t.Fatal(err)
	}
	var sl statehash.Snapshot
	w2.HashState(NewHashRegistry(), &sl)
	t.Logf("loaded=%016x (orig %016x) patrolRows=%d", sl.Top, sa.Top, w2.Patrol.Count())
	if sl.Top != sa.Top {
		t.Fatal("load diverged")
	}
	for i := 0; i < 30; i++ { // resume identically
		a.Step()
		w2.Step()
	}
	a.HashState(NewHashRegistry(), &sa)
	w2.HashState(NewHashRegistry(), &sl)
	if sa.Top != sl.Top {
		t.Fatal("resume diverged")
	}
}

func TestPatrolFollowTickAllocs(t *testing.T) {
	w := behWorld(t)
	a := atkUnit(t, w, 0, pt2(1000, 1000), 8*fixed.One)
	arm(t, w, a, 0, 0)
	w.IssueOrder(a, Order{Kind: OrderPatrol, Point: pt2(1300, 1000)}, false)
	f := atkUnit(t, w, 0, pt2(2000, 2000), 4*fixed.One)
	tgt := atkUnit(t, w, 0, pt2(2100, 2000), 0)
	w.IssueOrder(f, Order{Kind: OrderFollow, Target: tgt}, false)
	w.Step()
	allocs := testing.AllocsPerRun(50, func() { w.Step() })
	t.Logf("allocs/op driving patrol+follow: %v", allocs)
	if allocs != 0 {
		t.Fatalf("patrol/follow tick allocates: %v", allocs)
	}
}

var _ = fmt.Sprintf
