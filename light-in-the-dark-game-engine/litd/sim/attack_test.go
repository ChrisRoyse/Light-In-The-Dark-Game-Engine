package sim

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// atkMatrix: neutral 1000‰ row so damage numbers stay hand-checkable.
var atkMatrix = [][]int32{{1000}}

// atkUnit spawns an owned, damageable, order-capable unit.
func atkUnit(t *testing.T, w *World, team uint8, pos fixed.Vec2, speed fixed.F64) EntityID {
	t.Helper()
	id, ok := w.CreateUnit(pos, 0)
	if !ok || !w.Owners.Add(w.Ents, id, team, team, team) ||
		!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) ||
		!w.Combats.Add(w.Ents, id) ||
		!w.Orders.Add(w.Ents, id) {
		t.Fatal("atk unit setup failed")
	}
	if speed > 0 && !w.Movements.Add(w.Ents, w.Transforms, id, speed, 65535) {
		t.Fatal("movement add failed")
	}
	return id
}

// arm fills weapon slot s: dmg 10 flat, range 100, cooldown 27,
// damage-point 10, backswing 10 — the §6 worked-example timings.
func arm(t *testing.T, w *World, id EntityID, slot int, flags uint8) {
	t.Helper()
	cr := w.Combats.Row(id)
	c := w.Combats
	c.DmgBase[cr][slot] = 10
	c.AttackType[cr][slot] = 0
	c.Cooldown[cr][slot] = 27
	c.DamagePt[cr][slot] = 10
	c.Backswing[cr][slot] = 10
	c.Range[cr][slot] = 100 * fixed.One
	c.WFlags[cr][slot] = flags
}

// traceWorld builds attacker (team 0) + victim (team 1) at distance
// 50 (inside range 100), wires the transition trace.
func traceWorld(t *testing.T, flags uint8) (*World, EntityID, EntityID, *[]string) {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	a := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	v := atkUnit(t, w, 1, fixed.Vec2{X: 1050 * fixed.One, Y: 1000 * fixed.One}, 0)
	arm(t, w, a, 0, flags)
	trace := &[]string{}
	w.OnAttackTransition = func(tick uint32, id EntityID, slot int, from, to uint8) {
		*trace = append(*trace, fmt.Sprintf("t%d e%d w%d %s→%s", tick, id, slot, AtkStateName(from), AtkStateName(to)))
	}
	cr := w.Combats.Row(a)
	w.Combats.Target[cr] = v // engaged directly; acquisition has its own tests
	return w, a, v, trace
}

func logTrace(t *testing.T, trace *[]string) {
	t.Helper()
	for _, line := range *trace {
		t.Logf("%s", line)
	}
}

// Happy cycle: WINDUP entered at T=1, FIRE at exactly T+10 (damage
// point 0.5 s → 10 ticks), backswing 10 ticks, next WINDUP at
// windup-start+27 (cooldown 1.35 s → 27 ticks).
func TestAttackCycleTimings(t *testing.T) {
	w, _, v, trace := traceWorld(t, 0)
	hr := w.Healths.Row(v)
	for i := 0; i < 40; i++ {
		w.Step()
	}
	logTrace(t, trace)
	want := []string{
		"t1 e1 w0 idle→windup",       // in range, ready: commit at tick 1
		"t11 e1 w0 windup→backswing", // FIRE edge at exactly 1+10
		"t21 e1 w0 backswing→cooldown",
		"t28 e1 w0 cooldown→windup", // ReadyAt = 1+27
		"t38 e1 w0 windup→backswing",
	}
	if len(*trace) != len(want) {
		t.Fatalf("trace len %d, want %d", len(*trace), len(want))
	}
	for i := range want {
		if (*trace)[i] != want[i] {
			t.Fatalf("trace[%d] = %q, want %q", i, (*trace)[i], want[i])
		}
	}
	// two FIREs landed: 100 − 2×10 (coeff 1000‰, armor 0)
	if got := w.Healths.Life[hr]; got != 80*fixed.One {
		t.Fatalf("victim life = %d, want 80 after two hits", got)
	}
}

// Edge 1: target dies one tick before FIRE → cancel, NO packet, NO
// cooldown consumed — re-engage on a fresh target starts a windup
// immediately.
func TestAttackWindupCancelOnDeath(t *testing.T) {
	w, a, v, trace := traceWorld(t, 0)
	fired := 0
	w.RegisterHandler(1, func(w *World, e Event) { fired++ })
	w.Subscribe(EvUnitDamaged, 1)
	for i := 0; i < 10; i++ { // windup at t1, FIRE would be t11
		w.Step()
	}
	w.KillUnit(v) // dies tick 11 via external kill, before phase-5 FIRE? KillUnit marks now;
	// mark during phase 5 of tick 11 instead: kill BEFORE stepping so phase 5 sees it dead
	w.Step() // tick 11: target in killed buffer → removed end of 11; attack sees alive until phase 7
	w.Step() // tick 12: target gone → windup cancels
	logTrace(t, trace)
	if fired != 1 {
		// the t11 FIRE legitimately lands (target dies the same tick,
		// damage already in flight); the SECOND windup must not start
		t.Fatalf("EvUnitDamaged fired %d times", fired)
	}
	// fresh target appears: cooldown was NOT consumed by the cancel —
	// but the t11 FIRE consumed it; assert state returned to idle
	cr := w.Combats.Row(a)
	if w.Combats.AtkState[cr][0] != AtkIdle || w.Combats.Target[cr] != 0 {
		t.Fatalf("state=%s target=%d, want idle/0 after target death",
			AtkStateName(w.Combats.AtkState[cr][0]), w.Combats.Target[cr])
	}
}

// Edge 1b: cancel BEFORE fire (move order mid-windup), cooldown not
// consumed → re-issuing attack starts windup the very next tick.
func TestAttackWindupCancelNoCooldown(t *testing.T) {
	w, a, v, trace := traceWorld(t, 0)
	w.Step() // t1: windup begins
	w.IssueOrder(a, Order{Kind: OrderMove, Point: fixed.Vec2{X: 2000 * fixed.One, Y: 1000 * fixed.One}}, false)
	w.Step() // t2: order head=Move → windup cancels, no packet
	if n := len(w.dmgBuf); n != 0 {
		t.Fatalf("packets after cancel = %d, want 0", n)
	}
	w.IssueOrder(a, Order{Kind: OrderAttack, Target: v}, false)
	w.Step() // t3: ready (cooldown untouched) → windup immediately
	logTrace(t, trace)
	want := []string{"t1 e1 w0 idle→windup", "t2 e1 w0 windup→idle", "t3 e1 w0 idle→windup"}
	for i := range want {
		if (*trace)[i] != want[i] {
			t.Fatalf("trace[%d] = %q, want %q", i, (*trace)[i], want[i])
		}
	}
}

// Edge 3: the flag variant — a canceled windup consumes the full
// attack period; re-engagement waits on the clock.
func TestAttackWindupCancelConsumesCooldown(t *testing.T) {
	w, a, v, trace := traceWorld(t, WeaponCancelConsumesCooldown)
	w.Step() // t1: windup
	w.IssueOrder(a, Order{Kind: OrderMove, Point: fixed.Vec2{X: 2000 * fixed.One, Y: 1000 * fixed.One}}, false)
	w.Step() // t2: cancel, ReadyAt = 1+27 = 28
	cr := w.Combats.Row(a)
	if got := w.Combats.ReadyAt[cr][0]; got != 28 {
		t.Fatalf("ReadyAt = %d, want 28 (windup start 1 + cooldown 27)", got)
	}
	w.IssueOrder(a, Order{Kind: OrderAttack, Target: v}, false)
	for w.Tick() < 30 {
		w.Step()
	}
	logTrace(t, trace)
	// re-engage waits in cooldown until t28
	want := []string{"t1 e1 w0 idle→windup", "t2 e1 w0 windup→idle", "t3 e1 w0 idle→cooldown", "t28 e1 w0 cooldown→windup"}
	for i := range want {
		if (*trace)[i] != want[i] {
			t.Fatalf("trace[%d] = %q, want %q", i, (*trace)[i], want[i])
		}
	}
}

// Edge 2: a move order during BACKSWING interrupts instantly.
func TestAttackBackswingInterrupt(t *testing.T) {
	w, a, _, trace := traceWorld(t, 0)
	for w.Tick() < 12 { // FIRE at t11 → backswing t11..21
		w.Step()
	}
	cr := w.Combats.Row(a)
	if w.Combats.AtkState[cr][0] != AtkBackswing {
		t.Fatalf("setup: state=%s, want backswing", AtkStateName(w.Combats.AtkState[cr][0]))
	}
	w.IssueOrder(a, Order{Kind: OrderMove, Point: fixed.Vec2{X: 2000 * fixed.One, Y: 1000 * fixed.One}}, false)
	w.Step() // t13: interrupt is instant
	logTrace(t, trace)
	if w.Combats.AtkState[cr][0] != AtkIdle {
		t.Fatalf("backswing not interrupted: %s", AtkStateName(w.Combats.AtkState[cr][0]))
	}
	last := (*trace)[len(*trace)-1]
	if last != "t13 e1 w0 backswing→idle" {
		t.Fatalf("last transition %q, want t13 backswing→idle", last)
	}
}

// Edge 4: two weapons on one unit cycle independently — different
// damage points and cooldowns interleave.
func TestAttackTwoWeaponsIndependent(t *testing.T) {
	w, a, v, trace := traceWorld(t, 0)
	cr := w.Combats.Row(a)
	// slot 1: faster weapon — dp 4, cooldown 13, backswing 2
	c := w.Combats
	c.DmgBase[cr][1] = 5
	c.Cooldown[cr][1] = 13
	c.DamagePt[cr][1] = 4
	c.Backswing[cr][1] = 2
	c.Range[cr][1] = 100 * fixed.One
	_ = v
	for i := 0; i < 30; i++ {
		w.Step()
	}
	logTrace(t, trace)
	// spot-check the interleave: w1 fires at t5 (1+4) and re-windups
	// at t14 (1+13); w0 fires at t11
	mustContain := []string{
		"t5 e1 w1 windup→backswing",
		"t11 e1 w0 windup→backswing",
		"t14 e1 w1 cooldown→windup",
		"t28 e1 w0 cooldown→windup",
	}
	got := map[string]bool{}
	for _, l := range *trace {
		got[l] = true
	}
	for _, m := range mustContain {
		if !got[m] {
			t.Fatalf("trace missing %q", m)
		}
	}
}

// Chase: a mobile attacker outside attack range closes in (movement
// driven by the cycle), halts on range entry, then fires.
func TestAttackChaseClosesToRange(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	a := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 20*fixed.One)
	v := atkUnit(t, w, 1, fixed.Vec2{X: 1400 * fixed.One, Y: 1000 * fixed.One}, 0) // 400 away, range 100
	arm(t, w, a, 0, 0)
	var trace []string
	w.OnAttackTransition = func(tick uint32, id EntityID, slot int, from, to uint8) {
		trace = append(trace, fmt.Sprintf("t%d %s→%s", tick, AtkStateName(from), AtkStateName(to)))
	}
	w.Combats.Target[w.Combats.Row(a)] = v
	fired := false
	w.RegisterHandler(1, func(w *World, e Event) { fired = true })
	w.Subscribe(EvUnitDamaged, 1)
	for i := 0; i < 60 && !fired; i++ {
		w.Step()
	}
	for _, l := range trace {
		t.Logf("%s", l)
	}
	if !fired {
		t.Fatal("chase never closed to a FIRE in 60 ticks")
	}
	if trace[0] != "t1 idle→chase" {
		t.Fatalf("first transition %q, want t1 idle→chase", trace[0])
	}
	// movement halted on range entry
	if st := w.Movements.State[w.Movements.Row(a)]; st != MoveIdle {
		t.Fatalf("feet still moving after range entry: state %d", st)
	}
}

// R-GC-1: a steady-state attack cycle allocates nothing.
func TestAttackCycleAllocs(t *testing.T) {
	w, _, _, _ := traceWorld(t, 0)
	w.OnAttackTransition = nil
	allocs := testing.AllocsPerRun(300, func() { w.Step() })
	if allocs != 0 {
		t.Fatalf("Step with active attack cycle allocates %v/run, want 0 (R-GC-1)", allocs)
	}
}

// An attacker's FIRE edge stages a non-hashing RenderUnitAttack presentation cue
// (#313) carrying its unit-type id — once per attack cycle (cooldown-gated), not
// per damage tick. X+X=Y: a single attacker (type id 3) firing once in the first
// cycle → exactly one RenderUnitAttack{Ent=attacker, Data=3}, at the FIRE tick.
func TestSnapshotUnitAttackRenderEventFSV(t *testing.T) {
	w, a, _, _ := traceWorld(t, 0)
	const typeID = uint16(3)
	w.UnitTypes.Add(w.Ents, a, typeID)

	var found []RenderEvent
	var fireTick uint32
	for i := 0; i < 12; i++ { // FIRE edge at tick 11; next cycle is tick 38
		w.Step()
		for _, e := range w.Snaps.Curr().Events {
			if e.Kind == RenderUnitAttack {
				found = append(found, e)
				fireTick = w.Snaps.Curr().Tick
			}
		}
	}
	if len(found) != 1 {
		t.Fatalf("RenderUnitAttack cues in 12 ticks = %d, want exactly 1 (one FIRE edge)", len(found))
	}
	if found[0].Ent != a || found[0].Data != typeID {
		t.Fatalf("attack cue Ent=%d Data=%d, want attacker=%d typeID=%d", found[0].Ent.Index(), found[0].Data, a.Index(), typeID)
	}
	t.Logf("FSV #313 sim: attacker FIRE edge → 1 RenderUnitAttack{Data=typeID=%d} at tick %d", typeID, fireTick)
}
