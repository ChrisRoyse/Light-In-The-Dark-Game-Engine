package ai_test

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// ---------------------------------------------------------------------------
// Part A — controlled CaptainControl. A scripted world where unit positions,
// deaths, damage, and the enemy presence are set by the test, so the captain's
// state-transition trace is exact and deterministic. SoT = the captain's own
// state read after each Tick (state, centroid, readiness, group size) plus the
// save-blob bytes for the round-trip edge.
// ---------------------------------------------------------------------------

type capUnit struct {
	x, y           int32
	alive          bool
	hp, mana       int32 // percentages; mana < 0 == no mana pool
	enlisted       bool  // recruited at least once (dead-but-enlisted is never re-drafted)
	tx, ty         int32 // standing order target
	ordered        bool
}

type fakeCapCtrl struct {
	units   []*capUnit // index+1 == unit id (ascending)
	enemyX  int32
	enemyY  int32
	enemyOn bool
}

func newFakeCapCtrl(n int, hp, mana int32) *fakeCapCtrl {
	f := &fakeCapCtrl{}
	for i := 0; i < n; i++ {
		f.units = append(f.units, &capUnit{alive: true, hp: hp, mana: mana})
	}
	return f
}

func (f *fakeCapCtrl) unit(id int32) *capUnit {
	if id < 1 || int(id) > len(f.units) {
		return nil
	}
	return f.units[id-1]
}

// Recruit fills toward want: counts living enlisted members, then enlists the
// next idle (alive, never-enlisted) ids in ascending order to top up. Dead
// enlisted units are skipped (no drafting corpses).
func (f *fakeCapCtrl) Recruit(player int, slot ai.CaptainSlot, want int, dst []int32) []int32 {
	have := 0
	for _, u := range f.units {
		if u.enlisted && u.alive {
			have++
		}
	}
	for i, u := range f.units {
		if have >= want {
			break
		}
		if u.enlisted || !u.alive {
			continue
		}
		u.enlisted = true
		dst = append(dst, int32(i+1))
		have++
	}
	return dst
}

func (f *fakeCapCtrl) UnitState(id int32) (x, y int32, alive bool, hpPct, manaPct int32) {
	u := f.unit(id)
	if u == nil || !u.alive {
		return 0, 0, false, 0, 0
	}
	return u.x, u.y, true, u.hp, u.mana
}

func (f *fakeCapCtrl) OrderMoveTo(id, x, y int32) {
	if u := f.unit(id); u != nil {
		u.tx, u.ty, u.ordered = x, y, true
	}
}
func (f *fakeCapCtrl) OrderAttackTo(id, x, y int32) {
	if u := f.unit(id); u != nil {
		u.tx, u.ty, u.ordered = x, y, true
	}
}
func (f *fakeCapCtrl) EnemyNear(player int, x, y, radius int32) bool {
	if !f.enemyOn {
		return false
	}
	dx := int64(x - f.enemyX)
	dy := int64(y - f.enemyY)
	return dx*dx+dy*dy <= int64(radius)*int64(radius)
}

// advance steps every ordered living unit toward its standing target by step
// (per axis; axis-aligned test paths make this exact).
func (f *fakeCapCtrl) advance(step int32) {
	for _, u := range f.units {
		if !u.alive || !u.ordered {
			continue
		}
		u.x = stepToward(u.x, u.tx, step)
		u.y = stepToward(u.y, u.ty, step)
	}
}

func stepToward(cur, dst, step int32) int32 {
	switch {
	case cur < dst:
		if dst-cur <= step {
			return dst
		}
		return cur + step
	case cur > dst:
		if cur-dst <= step {
			return dst
		}
		return cur - step
	default:
		return cur
	}
}

func (f *fakeCapCtrl) clone() *fakeCapCtrl {
	c := &fakeCapCtrl{enemyX: f.enemyX, enemyY: f.enemyY, enemyOn: f.enemyOn}
	for _, u := range f.units {
		cp := *u
		c.units = append(c.units, &cp)
	}
	return c
}

func capCfg() ai.CaptainConfig {
	return ai.CaptainConfig{Capacity: 3, RetreatPct: 50, EngageRadius: 400, ArriveRadius: 64}
}

func traceLine(t uint32, c *ai.Captain) string {
	return fmt.Sprintf("t%-2d %-10s size=%d ready=%d hp=%d ma=%d home=%v full=%v atGoal=%v combat=%v retreat=%v members=%v",
		t, c.State(), c.GroupSize(), c.Readiness(), c.ReadinessHP(), c.ReadinessMa(),
		c.IsHome(), c.IsFull(), c.AtGoal(), c.InCombat(), c.Retreating(), c.Members())
}

// runLifecycle drives the full form→attack→retreat-on-losses→go-home→reform
// scenario and returns the per-tick state trace. killAt names the tick (after
// which) two members die, simulating combat losses.
func runLifecycle(t *testing.T, log bool) []string {
	f := newFakeCapCtrl(5, 100, -1) // 5 idle units available, no mana, full hp
	f.enemyX, f.enemyY, f.enemyOn = 1000, 0, true
	c := ai.NewCaptain(f, 1, ai.SlotAttack, 0, 0, capCfg())

	const step = int32(250)
	var trace []string
	for now := uint32(0); now <= 11; now++ {
		switch now {
		case 1:
			c.Attack(1000, 0) // order the attack at tick 1
		case 6:
			// combat losses: kill members 2 and 3 (two of the three enlisted)
			f.units[1].alive = false
			f.units[2].alive = false
		}
		c.Tick(now)
		f.advance(step)
		line := traceLine(now, c)
		trace = append(trace, line)
		if log {
			t.Log(line)
		}
	}
	return trace
}

// TestCaptainLifecycleTraceFSV — the headline scenario. Form a group, lead the
// attack, retreat on losses, reform at home; the state-transition trace is
// identical across two runs (determinism) and matches the hand-computed
// known-output schedule (KIKO).
func TestCaptainLifecycleTraceFSV(t *testing.T) {
	t.Log("=== run 1 ===")
	a := runLifecycle(t, true)
	t.Log("=== run 2 ===")
	b := runLifecycle(t, false)

	if len(a) != len(b) {
		t.Fatalf("trace length differs: %d vs %d", len(a), len(b))
	}
	for i := range a {
		if a[i] != b[i] {
			t.Fatalf("trace diverges at line %d:\n run1: %s\n run2: %s", i, a[i], b[i])
		}
	}
	t.Logf("determinism: %d/%d trace lines identical across 2 runs", len(a), len(b))

	// KIKO assertions on the known schedule (capacity 3, step 250, goal x=1000,
	// arrive 64, engage 400, retreat<=50%, losses after t6).
	type want struct {
		tick  uint32
		state ai.CaptainState
		size  int
		ready int32
	}
	// derive the (tick,state,size,readiness) tuple from each trace line's prefix
	// by re-running with structured capture instead of string parsing:
	f := newFakeCapCtrl(5, 100, -1)
	f.enemyX, f.enemyY, f.enemyOn = 1000, 0, true
	c := ai.NewCaptain(f, 1, ai.SlotAttack, 0, 0, capCfg())
	const step = int32(250)
	got := map[uint32]want{}
	for now := uint32(0); now <= 11; now++ {
		switch now {
		case 1:
			c.Attack(1000, 0)
		case 6:
			f.units[1].alive = false
			f.units[2].alive = false
		}
		c.Tick(now)
		f.advance(step)
		got[now] = want{now, c.State(), c.GroupSize(), c.Readiness()}
	}
	expect := []want{
		{0, ai.CapFull, 3, 100},       // recruited & full at home
		{1, ai.CapMarching, 3, 100},   // attack ordered, marching out
		{4, ai.CapMarching, 3, 100},   // still marching (x=1000 reached only after advance)
		{5, ai.CapEngaged, 3, 100},    // centroid at goal, enemy present → engaged
		{6, ai.CapRetreating, 1, 33},  // 2 killed → readiness 33 <= 50 → retreat
		{10, ai.CapHome, 1, 33},       // survivor back at home (recruit lags one tick)
		{11, ai.CapFull, 3, 100},      // reformed to full strength
	}
	for _, e := range expect {
		g := got[e.tick]
		if g.state != e.state || g.size != e.size || g.ready != e.ready {
			t.Errorf("t%d: got state=%v size=%d ready=%d; want state=%v size=%d ready=%d",
				e.tick, g.state, g.size, g.ready, e.state, e.size, e.ready)
		} else {
			t.Logf("t%d KIKO OK: state=%v size=%d ready=%d", e.tick, g.state, g.size, g.ready)
		}
	}

	// Conservation: at every tick, group size never exceeds capacity and
	// readiness is in [0,100].
	for now := uint32(0); now <= 11; now++ {
		g := got[now]
		if g.size > 3 || g.size < 0 {
			t.Errorf("t%d: size %d out of [0,3]", now, g.size)
		}
		if g.ready < 0 || g.ready > 100 {
			t.Errorf("t%d: readiness %d out of [0,100]", now, g.ready)
		}
	}
}

// TestCaptainSpawnIsHomeFSV — edge 1: an empty, freshly-spawned captain reads
// home (and empty / not full / not at goal). Print state before any Tick.
func TestCaptainSpawnIsHomeFSV(t *testing.T) {
	f := newFakeCapCtrl(3, 100, -1)
	c := ai.NewCaptain(f, 1, ai.SlotAttack, 500, 500, capCfg())

	t.Logf("BEFORE any tick: state=%v size=%d home=%v empty=%v full=%v atGoal=%v",
		c.State(), c.GroupSize(), c.IsHome(), c.IsEmpty(), c.IsFull(), c.AtGoal())

	if !c.IsHome() {
		t.Fatalf("fresh captain must read home; state=%v", c.State())
	}
	if !c.IsEmpty() {
		t.Fatalf("fresh captain must be empty; size=%d", c.GroupSize())
	}
	if c.IsFull() {
		t.Fatalf("empty captain must not be full")
	}
	if c.AtGoal() {
		t.Fatalf("empty captain must not be at goal")
	}
	if c.State() != ai.CapHome {
		t.Fatalf("fresh captain state = %v, want Home", c.State())
	}
	t.Log("edge1 OK: empty fresh captain reads Home/empty/not-full/not-at-goal")
}

// TestCaptainWipedWhileEngagedFSV — edge 2: the whole group dies while engaged;
// the captain transitions home to reform per documented semantics. Print
// before/after.
func TestCaptainWipedWhileEngagedFSV(t *testing.T) {
	f := newFakeCapCtrl(6, 100, -1)
	f.enemyX, f.enemyY, f.enemyOn = 0, 0, true // enemy at home so engaged immediately
	c := ai.NewCaptain(f, 1, ai.SlotAttack, 0, 0, capCfg())

	c.Tick(0)            // recruit 3 → Full at home
	c.Attack(0, 0)       // attack at home point → centroid already at goal
	c.Tick(1)            // Marching → at goal → Engaged
	if c.State() != ai.CapEngaged {
		t.Fatalf("setup: want Engaged, got %v", c.State())
	}
	t.Logf("BEFORE wipe: state=%v size=%d empty=%v", c.State(), c.GroupSize(), c.IsEmpty())

	// wipe the whole group
	for _, u := range f.units {
		u.alive = false
	}
	c.Tick(2) // prune → empty while Engaged → Home
	t.Logf("AFTER wipe : state=%v size=%d empty=%v home=%v", c.State(), c.GroupSize(), c.IsEmpty(), c.IsHome())
	if c.State() != ai.CapHome {
		t.Fatalf("wiped captain must go Home to reform; got %v", c.State())
	}
	if !c.IsEmpty() {
		t.Fatalf("wiped captain must be empty; size=%d", c.GroupSize())
	}

	// reform: revive the spare units so recruit can refill (the 3 spares 4,5,6)
	for i := 3; i < 6; i++ {
		f.units[i].alive = true
	}
	c.Tick(3) // Home → recruit 3 spares → Full
	t.Logf("AFTER reform: state=%v size=%d full=%v members=%v", c.State(), c.GroupSize(), c.IsFull(), c.Members())
	if c.State() != ai.CapFull || c.GroupSize() != 3 {
		t.Fatalf("reform failed: state=%v size=%d", c.State(), c.GroupSize())
	}
	t.Log("edge2 OK: wipe-while-engaged → Home → reform to Full")
}

// TestCaptainGoHomeMidAttackFSV — edge 3: GoHome ordered mid-march overrides the
// attack; members get a move-home order. Print the order-target override trace.
func TestCaptainGoHomeMidAttackFSV(t *testing.T) {
	f := newFakeCapCtrl(3, 100, -1)
	c := ai.NewCaptain(f, 1, ai.SlotAttack, 0, 0, capCfg())

	c.Tick(0)          // recruit 3 → Full
	c.Attack(1000, 0)  // march toward (1000,0)
	c.Tick(1)
	f.advance(250)     // x → 250
	mem := c.Members()
	t.Logf("MID-MARCH t1: state=%v cx≈250 order-target of m%d = (%d,%d)",
		c.State(), mem[0], f.unit(mem[0]).tx, f.unit(mem[0]).ty)
	if c.State() != ai.CapMarching {
		t.Fatalf("setup: want Marching, got %v", c.State())
	}
	if f.unit(mem[0]).tx != 1000 {
		t.Fatalf("mid-march order target x = %d, want 1000", f.unit(mem[0]).tx)
	}

	c.GoHome() // override
	t.Logf("AFTER GoHome: state=%v retreat=%v order-target of m%d = (%d,%d)",
		c.State(), c.Retreating(), mem[0], f.unit(mem[0]).tx, f.unit(mem[0]).ty)
	if c.State() != ai.CapRetreating || !c.Retreating() {
		t.Fatalf("GoHome must set Retreating; got %v", c.State())
	}
	if f.unit(mem[0]).tx != 0 || f.unit(mem[0]).ty != 0 {
		t.Fatalf("GoHome must re-order members to home (0,0); got (%d,%d)",
			f.unit(mem[0]).tx, f.unit(mem[0]).ty)
	}
	t.Log("edge3 OK: GoHome mid-march overrode attack → Retreating, members re-ordered home")
}

// TestCaptainSaveMidMarchFSV — edge 4: save mid-march, restore into a fresh
// captain on an identical world, and verify it reaches the goal (Engaged) on the
// identical tick as an uninterrupted reference run. SoT = arrival tick + the
// save blob round-trip.
func TestCaptainSaveMidMarchFSV(t *testing.T) {
	const step = int32(200)
	arrival := func(c *ai.Captain, f *fakeCapCtrl, start uint32, saveAt int, t *testing.T) uint32 {
		var blob []byte
		var restored *ai.Captain
		var rf *fakeCapCtrl
		for now := start; now <= start+30; now++ {
			cur, curF := c, f
			if restored != nil {
				cur, curF = restored, rf
			}
			cur.Tick(now)
			curF.advance(step)
			if cur.State() == ai.CapEngaged {
				return now
			}
			if saveAt >= 0 && int(now) == saveAt && restored == nil {
				// save the captain + clone the world, then continue on the clone
				blob = c.Save(nil)
				rf = f.clone()
				restored = ai.NewCaptain(rf, 9, ai.SlotDefense, -1, -1, ai.CaptainConfig{Capacity: 1})
				if err := restored.Load(blob); err != nil {
					t.Fatalf("Load: %v", err)
				}
				t.Logf("t%d: saved mid-march (%d bytes), continuing on restored captain", now, len(blob))
			}
		}
		t.Fatal("never reached goal")
		return 0
	}

	// Reference: uninterrupted march.
	fRef := newFakeCapCtrl(3, 100, -1)
	fRef.enemyX, fRef.enemyY, fRef.enemyOn = 2000, 0, true
	cRef := ai.NewCaptain(fRef, 1, ai.SlotAttack, 0, 0, ai.CaptainConfig{Capacity: 3, ArriveRadius: 64})
	cRef.Tick(0) // recruit → Full
	cRef.Attack(2000, 0)
	refArrival := arrival(cRef, fRef, 1, -1, t)
	t.Logf("reference arrival tick = %d", refArrival)

	// Saved/restored: identical world, but saved mid-march at tick 4.
	fSav := newFakeCapCtrl(3, 100, -1)
	fSav.enemyX, fSav.enemyY, fSav.enemyOn = 2000, 0, true
	cSav := ai.NewCaptain(fSav, 1, ai.SlotAttack, 0, 0, ai.CaptainConfig{Capacity: 3, ArriveRadius: 64})
	cSav.Tick(0)
	cSav.Attack(2000, 0)
	savArrival := arrival(cSav, fSav, 1, 4, t)
	t.Logf("saved/restored arrival tick = %d", savArrival)

	if refArrival != savArrival {
		t.Fatalf("arrival tick differs: reference %d vs saved/restored %d", refArrival, savArrival)
	}
	t.Logf("edge4 OK: save-mid-march round-trip reaches goal on identical tick %d", refArrival)
}

// TestCaptainReadinessManaFSV — ReadinessMa is honest: it averages only
// mana-capable members and returns the not-applicable sentinel (-1) when none
// have a mana pool. No fabricated value.
func TestCaptainReadinessManaFSV(t *testing.T) {
	// Two units: one with 80% mana, one with none (-1).
	f := newFakeCapCtrl(2, 100, -1)
	f.units[0].mana = 80 // first unit is a caster at 80% mana
	c := ai.NewCaptain(f, 1, ai.SlotAttack, 0, 0, ai.CaptainConfig{Capacity: 2})
	c.Tick(0)
	t.Logf("mixed roster: ReadinessMa=%d (avg of mana-capable only)", c.ReadinessMa())
	if c.ReadinessMa() != 80 {
		t.Fatalf("ReadinessMa with one 80%% caster + one manaless = %d, want 80", c.ReadinessMa())
	}

	// All manaless → not applicable.
	f2 := newFakeCapCtrl(3, 100, -1)
	c2 := ai.NewCaptain(f2, 1, ai.SlotAttack, 0, 0, ai.CaptainConfig{Capacity: 3})
	c2.Tick(0)
	t.Logf("manaless roster: ReadinessMa=%d (not-applicable sentinel)", c2.ReadinessMa())
	if c2.ReadinessMa() != -1 {
		t.Fatalf("ReadinessMa for a manaless group = %d, want -1 (NA)", c2.ReadinessMa())
	}
	t.Log("mana OK: ReadinessMa averages mana-capable members, -1 when none — no fabrication")
}

// TestCaptainCorpsSaveFailClosedFSV — a corrupt save leaves the corps unchanged.
func TestCaptainCorpsSaveFailClosedFSV(t *testing.T) {
	f := newFakeCapCtrl(8, 100, -1)
	cc := ai.CreateCaptains(f, 1, 0, 0, capCfg())
	cc.Tick(0) // both captains recruit
	atkBefore := cc.Captain(ai.SlotAttack).GroupSize()
	defBefore := cc.Captain(ai.SlotDefense).GroupSize()

	good := cc.Save(nil)
	t.Logf("good corps blob = %d bytes; attack size=%d defense size=%d", len(good), atkBefore, defBefore)

	for _, bad := range [][]byte{
		nil,
		[]byte("XXXX"),
		append([]byte("LITDCPCO"), 0xff), // good magic, truncated
		good[:len(good)-4],               // truncated tail
	} {
		if err := cc.Load(bad); err == nil {
			t.Fatalf("Load(%q...) returned nil; want fail-closed error", bad[:min(len(bad), 8)])
		}
	}
	if cc.Captain(ai.SlotAttack).GroupSize() != atkBefore || cc.Captain(ai.SlotDefense).GroupSize() != defBefore {
		t.Fatalf("fail-closed violated: sizes changed to %d/%d",
			cc.Captain(ai.SlotAttack).GroupSize(), cc.Captain(ai.SlotDefense).GroupSize())
	}

	// A good blob round-trips.
	if err := cc.Load(good); err != nil {
		t.Fatalf("Load(good): %v", err)
	}
	if cc.Captain(ai.SlotAttack).GroupSize() != atkBefore {
		t.Fatalf("round-trip changed attack size: %d", cc.Captain(ai.SlotAttack).GroupSize())
	}
	t.Log("fail-closed OK: corrupt blobs rejected, corps unchanged; good blob round-trips")
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

// ---------------------------------------------------------------------------
// Part B — real sim. Proves the CaptainControl maps honestly to the sim: a
// captain recruits real idle units (ascending entity id), and an Attack order
// actually moves them toward the goal (SoT = sim Transforms.Pos). Deterministic
// across two runs.
// ---------------------------------------------------------------------------

type captainAdapter struct {
	w        *sim.World
	enlisted map[int32]ai.CaptainSlot
}

func newCaptainAdapter(w *sim.World) *captainAdapter {
	return &captainAdapter{w: w, enlisted: map[int32]ai.CaptainSlot{}}
}

func (a *captainAdapter) Recruit(player int, slot ai.CaptainSlot, want int, dst []int32) []int32 {
	have := 0
	for id, s := range a.enlisted {
		if s == slot && a.w.Ents.Alive(sim.EntityID(uint32(id))) {
			have++
		}
	}
	var ids []sim.EntityID
	ids = a.w.AppendAllUnits(ids) // ascending entity id
	for _, id := range ids {
		if have >= want {
			break
		}
		i32 := int32(uint32(id))
		if _, ok := a.enlisted[i32]; ok {
			continue
		}
		or := a.w.Owners.Row(id)
		if or == -1 || int(a.w.Owners.Player[or]) != player {
			continue
		}
		a.enlisted[i32] = slot
		dst = append(dst, i32)
		have++
	}
	return dst
}

func (a *captainAdapter) UnitState(id int32) (x, y int32, alive bool, hpPct, manaPct int32) {
	e := sim.EntityID(uint32(id))
	if !a.w.Ents.Alive(e) {
		return 0, 0, false, 0, 0
	}
	tr := a.w.Transforms.Row(e)
	if tr == -1 {
		return 0, 0, false, 0, 0
	}
	p := a.w.Transforms.Pos[tr]
	x, y = int32(p.X.Floor()), int32(p.Y.Floor())
	hpPct = 100
	if hr := a.w.Healths.Row(e); hr != -1 {
		max := a.w.Healths.MaxLife[hr]
		if max > 0 {
			hpPct = int32((int64(a.w.Healths.Life[hr]) * 100) / int64(max))
		}
	}
	manaPct = -1 // honest NA unless the unit has a mana pool
	if ar := a.w.Abilities.Row(e); ar != -1 {
		if mx := a.w.Abilities.MaxMana[ar]; mx > 0 {
			manaPct = int32((int64(a.w.Abilities.Mana[ar]) * 100) / int64(mx))
		}
	}
	return x, y, true, hpPct, manaPct
}

func (a *captainAdapter) OrderMoveTo(id, x, y int32) {
	a.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: ptWU(x, y)}, false)
}
func (a *captainAdapter) OrderAttackTo(id, x, y int32) {
	// Attack-move realized as move-to-target; on arrival the idle stance acquires
	// (same faithful realization as the wave adapter — pursuit is #150/#380).
	a.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: ptWU(x, y)}, false)
}
func (a *captainAdapter) EnemyNear(player int, x, y, radius int32) bool {
	var ids []sim.EntityID
	ids = a.w.AppendAllUnits(ids)
	rr := int64(radius) * int64(radius)
	for _, id := range ids {
		or := a.w.Owners.Row(id)
		if or == -1 || int(a.w.Owners.Player[or]) == player {
			continue
		}
		tr := a.w.Transforms.Row(id)
		if tr == -1 {
			continue
		}
		p := a.w.Transforms.Pos[tr]
		dx := int64(int32(p.X.Floor()) - x)
		dy := int64(int32(p.Y.Floor()) - y)
		if dx*dx+dy*dy <= rr {
			return true
		}
	}
	return false
}

// TestCaptainRealSimMarchFSV — real sim. Spawn 3 idle soldiers for player 1; a
// captain recruits all three (ascending ids) and is ordered to attack a far
// point. After stepping the sim, the captain's centroid has measurably advanced
// toward the goal — proving the control's orders actually move sim units (SoT =
// Transforms.Pos). Identical across two runs.
func TestCaptainRealSimMarchFSV(t *testing.T) {
	run := func(log bool) (startCx, endCx int32, recruited []int32) {
		w := waveSimWorld(t)
		for i, p := range [][2]int32{{1000, 1000}, {1032, 1000}, {1000, 1032}} {
			if _, ok := w.SpawnFromTable(wSoldier, 1, 1, ptWU(p[0], p[1])); !ok {
				t.Fatalf("soldier %d spawn failed", i)
			}
		}
		ad := newCaptainAdapter(w)
		c := ai.NewCaptain(ad, 1, ai.SlotAttack, 1010, 1010, ai.CaptainConfig{Capacity: 3, ArriveRadius: 64})

		c.Tick(w.Tick()) // recruit the 3 idle soldiers → Full
		recruited = c.Members()
		// centroid before marching
		startCx = centroidX(w, recruited)
		if log {
			t.Logf("recruited=%v full=%v start centroid x=%d", recruited, c.IsFull(), startCx)
		}

		c.Attack(3000, 1010) // march far east
		for i := 0; i < 40; i++ {
			c.Tick(w.Tick())
			w.RecomputeVisibility()
			w.Step()
		}
		endCx = centroidX(w, recruited)
		if log {
			t.Logf("after 40 ticks: centroid x=%d state=%v", endCx, c.State())
		}
		return startCx, endCx, recruited
	}

	s1, e1, r1 := run(true)
	s2, e2, r2 := run(false)

	// determinism
	if s1 != s2 || e1 != e2 || fmt.Sprint(r1) != fmt.Sprint(r2) {
		t.Fatalf("non-deterministic: run1 (%d→%d, %v) run2 (%d→%d, %v)", s1, e1, r1, s2, e2, r2)
	}
	// recruited the 3 lowest entity ids, ascending
	if len(r1) != 3 || !(r1[0] < r1[1] && r1[1] < r1[2]) {
		t.Fatalf("recruit must yield 3 ascending ids; got %v", r1)
	}
	// SoT: the units actually advanced east toward the goal
	if e1 <= s1 {
		t.Fatalf("captain did not advance toward goal: centroid x %d → %d", s1, e1)
	}
	t.Logf("real-sim OK: recruited %v, centroid advanced east %d → %d (Transforms SoT), deterministic", r1, s1, e1)
}

func centroidX(w *sim.World, ids []int32) int32 {
	var sx, n int64
	for _, id := range ids {
		e := sim.EntityID(uint32(id))
		if !w.Ents.Alive(e) {
			continue
		}
		tr := w.Transforms.Row(e)
		if tr == -1 {
			continue
		}
		sx += int64(w.Transforms.Pos[tr].X.Floor())
		n++
	}
	if n == 0 {
		return 0
	}
	return int32(sx / n)
}
