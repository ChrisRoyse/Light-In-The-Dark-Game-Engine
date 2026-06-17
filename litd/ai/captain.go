package ai

import (
	"encoding/binary"
	"errors"
	"fmt"
)

// Captain-logic natives (#280; common.ai captain family — 20 functions;
// jass-mapping/ai-natives.md captain family; execution-model.md §6). WC3's AI
// gives each computer player exactly two captains — an ATTACK captain and a
// DEFENSE captain — each a strike group the AI controls as one unit: it fills
// up at home, marches to a goal, engages, and retreats home on losses. This is
// the wave-leading control layer over the #278 wave model.
//
// As with the rest of the AI cluster, the AI names WHAT (attack here, go home),
// and the sim chooses WHO/WHERE (R-EXEC-3): the captain recruits members and
// issues movement through a sim-free CaptainControl that a sim adapter
// satisfies; the captain never supplies entity ids it invented or mutates sim
// state directly. Member orders are typed commands handed to the sim's order
// system — there is no AI-side pathfinding.
//
// Determinism: every transition happens inside Tick(now) and is tick-quantized;
// member selection is entity-id order (the control returns ascending ids);
// position math is integer centroid + squared-distance (float distance inside
// the determinism boundary would be a platform-rounding hazard). A captain is
// flat data (roster + points + scalars), so it serializes directly and resumes
// to the identical arrival tick.
//
// Tombstones (hazard #2, jass-mapping/ai-natives.md): NONE. The WC3 natives that
// "misbehave in the AI context" are the string/callback natives; every captain
// native is a numeric/state query or a movement command, so the AIView/control
// isolation boundary makes those context bugs unrepresentable rather than
// requiring a tombstone. CaptainReadinessMa is implemented honestly — mana
// exists only on heroes in this sim, so it averages the mana fraction of
// mana-capable members and returns a not-applicable sentinel (-1) when none
// have mana, never a fabricated 0 or 100.

// CaptainSlot identifies one of a player's two captains.
type CaptainSlot uint8

const (
	SlotAttack  CaptainSlot = 0 // the offensive strike captain
	SlotDefense CaptainSlot = 1 // the base-defense captain
	numSlots                = 2
)

// CaptainState is the captain's lifecycle state. Transitions are deterministic
// and only change inside Tick.
type CaptainState uint8

const (
	CapHome       CaptainState = iota // at home, roster below full, gathering
	CapFull                           // at home, roster at full strength, ready to launch
	CapMarching                       // moving toward the attack goal
	CapEngaged                        // at/near the goal with enemies present
	CapRetreating                     // returning home (losses or GoHome order)
)

// String renders a state for trace logs.
func (s CaptainState) String() string {
	switch s {
	case CapHome:
		return "Home"
	case CapFull:
		return "Full"
	case CapMarching:
		return "Marching"
	case CapEngaged:
		return "Engaged"
	case CapRetreating:
		return "Retreating"
	default:
		return "?"
	}
}

// TargetMode records what kind of target the captain is bound to (the
// CaptainVsUnits / CaptainVsPlayer classification). v1 attack goals are points;
// the mode is recorded and queryable but the goal point drives movement.
type TargetMode uint8

const (
	TargetNone   TargetMode = iota // attack-move to a point (default)
	TargetUnits                    // bound to a specific unit-set (CaptainVsUnits)
	TargetPlayer                   // bound to a player's forces (CaptainVsPlayer)
)

// readinessMaNA is the not-applicable sentinel for mana readiness: returned when
// no member has a mana pool (honest "no data", never a fabricated value).
const readinessMaNA int32 = -1

// CaptainControl is the sim-authoritative surface a captain drives, satisfied by
// a sim adapter at the integration boundary (ints / integer world units here;
// EntityID / fixed-point in the sim). It exposes recruitment, per-member state,
// the two typed movement orders, and an enemy-proximity query — nothing else.
type CaptainControl interface {
	// Recruit fills (player, slot) toward want total members, incrementally
	// (keeps those already enlisted), appending NEWLY recruited unit ids to dst
	// in ascending entity-id order and returning the grown slice. The sim picks
	// idle units; an already-committed unit is never double-drafted.
	Recruit(player int, slot CaptainSlot, want int, dst []int32) []int32
	// UnitState reports a member's position (integer world units), liveness, and
	// hp/mana as percentages 0..100. A dead/absent unit returns alive == false.
	// manaPct is readinessMaNA (-1) for a unit with no mana pool.
	UnitState(id int32) (x, y int32, alive bool, hpPct, manaPct int32)
	// OrderMoveTo issues a plain move order toward (x,y).
	OrderMoveTo(id, x, y int32)
	// OrderAttackTo issues an attack-move order toward (x,y).
	OrderAttackTo(id, x, y int32)
	// EnemyNear reports whether any enemy of player is within radius of (x,y).
	EnemyNear(player int, x, y, radius int32) bool
}

// Captain is one strike group: a roster plus the staging/goal geometry and the
// scalars that drive its deterministic state machine. All fields are flat data,
// so a captain serializes directly.
type Captain struct {
	ctrl   CaptainControl
	player int
	slot   CaptainSlot

	state        CaptainState
	homeX, homeY int32   // staging point
	goalX, goalY int32   // current attack goal
	members      []int32 // entity ids, ascending; dead members pruned each Tick
	capacity     int     // target roster size (IsFull threshold)
	changes      bool    // SetCaptainChanges: roster may recruit/grow at home
	tmode        TargetMode
	target       int32 // unit-set tag or player num (per tmode)

	retreatPct   int32 // readiness (0..100) at/below which the captain retreats
	engageRadius int32 // world units: enemies within this of the centroid = engaged
	arriveRadius int32 // world units: centroid within this of a point = "at" it

	lastTick uint32

	scratch []int32 // reused recruit destination, keeps Tick allocation-free
}

// CaptainConfig parameterizes a new captain. Zero fields take documented
// defaults via NewCaptain.
type CaptainConfig struct {
	Capacity     int   // target roster size (default 1)
	RetreatPct   int32 // retreat threshold 0..100 (default 50)
	EngageRadius int32 // default 400 world units
	ArriveRadius int32 // default 64 world units
}

// NewCaptain binds a captain for (player, slot) staged at (homeX, homeY).
func NewCaptain(ctrl CaptainControl, player int, slot CaptainSlot, homeX, homeY int32, cfg CaptainConfig) *Captain {
	if cfg.Capacity <= 0 {
		cfg.Capacity = 1
	}
	if cfg.RetreatPct == 0 {
		cfg.RetreatPct = 50
	}
	if cfg.EngageRadius == 0 {
		cfg.EngageRadius = 400
	}
	if cfg.ArriveRadius == 0 {
		cfg.ArriveRadius = 64
	}
	return &Captain{
		ctrl:         ctrl,
		player:       player,
		slot:         slot,
		state:        CapHome,
		homeX:        homeX,
		homeY:        homeY,
		goalX:        homeX,
		goalY:        homeY,
		capacity:     cfg.Capacity,
		changes:      true,
		retreatPct:   cfg.RetreatPct,
		engageRadius: cfg.EngageRadius,
		arriveRadius: cfg.ArriveRadius,
	}
}

// ---- commander natives (mutating) ----

// SetChanges toggles whether the captain recruits to top up its roster at home
// (SetCaptainChanges). With changes off, a depleted captain does not refill.
func (c *Captain) SetChanges(allow bool) { c.changes = allow }

// SetHome sets the staging point (SetCaptainHome).
func (c *Captain) SetHome(x, y int32) { c.homeX, c.homeY = x, y }

// ResetLocs clears the attack goal back to home and drops target binding
// (ResetCaptainLocs).
func (c *Captain) ResetLocs() {
	c.goalX, c.goalY = c.homeX, c.homeY
	c.tmode, c.target = TargetNone, 0
}

// Teleport relocates the staging point to (x,y) (TeleportCaptain). When the
// captain is home/full it re-issues move orders so the group restages there;
// this sim has no instantaneous unit teleport in the control surface, so a
// teleport is realized as a relocation order — an honest behavioral mapping,
// not a bug.
func (c *Captain) Teleport(x, y int32) {
	c.homeX, c.homeY = x, y
	if c.state == CapHome || c.state == CapFull {
		c.goalX, c.goalY = x, y
		c.issueMove(x, y)
	}
}

// ClearTargets drops the captain's target binding (ClearCaptainTargets).
func (c *Captain) ClearTargets() { c.tmode, c.target = TargetNone, 0 }

// Attack orders the captain to attack-move to (x,y): goal set, state → Marching,
// orders issued (CaptainAttack). A no-op for an empty captain (nothing to send).
func (c *Captain) Attack(x, y int32) {
	if c.IsEmpty() {
		return
	}
	c.goalX, c.goalY = x, y
	c.state = CapMarching
	c.issueAttack(x, y)
}

// AttackVsUnits / AttackVsPlayer set the target classification alongside the
// attack goal so the VsUnits/VsPlayer predicates read true. The point still
// drives movement (v1 goals are points).
func (c *Captain) AttackVsUnits(tag, x, y int32) {
	c.tmode, c.target = TargetUnits, tag
	c.Attack(x, y)
}
func (c *Captain) AttackVsPlayer(player, x, y int32) {
	c.tmode, c.target = TargetPlayer, player
	c.Attack(x, y)
}

// GoHome orders the captain back to its staging point: state → Retreating,
// move-home orders issued (CaptainGoHome). Overrides any in-flight attack.
func (c *Captain) GoHome() {
	if c.IsEmpty() {
		c.state = CapHome
		return
	}
	c.state = CapRetreating
	c.goalX, c.goalY = c.homeX, c.homeY
	c.issueMove(c.homeX, c.homeY)
}

// ---- the deterministic state machine ----

// Tick advances the captain one quantized step: prune the dead, recruit at home,
// then run the state transition. All movement re-issues are idempotent (the sim
// treats a repeated identical order as a no-op), keeping the step deterministic.
func (c *Captain) Tick(now uint32) {
	c.lastTick = now
	c.prune()

	// Recruit only while staging at home and allowed to change composition.
	if c.changes && (c.state == CapHome || c.state == CapFull) && len(c.members) < c.capacity {
		c.scratch = c.ctrl.Recruit(c.player, c.slot, c.capacity, c.scratch[:0])
		c.members = append(c.members, c.scratch...)
		sortAscInt32(c.members)
	}

	switch c.state {
	case CapHome:
		if c.IsFull() {
			c.state = CapFull
		}

	case CapFull:
		// Ready; waits for an Attack order. Drop back if readiness slips.
		if !c.IsFull() {
			c.state = CapHome
		}

	case CapMarching:
		if c.IsEmpty() {
			c.state = CapHome
			return
		}
		if c.Readiness() <= c.retreatPct {
			c.GoHome()
			return
		}
		cx, cy, ok := c.centroid()
		if ok && near(cx, cy, c.goalX, c.goalY, c.arriveRadius) {
			c.state = CapEngaged
		} else {
			c.issueAttack(c.goalX, c.goalY) // keep marching
		}

	case CapEngaged:
		if c.IsEmpty() {
			c.state = CapHome // wave wiped while engaged → reform at home
			return
		}
		if c.Readiness() <= c.retreatPct {
			c.GoHome()
			return
		}
		cx, cy, ok := c.centroid()
		if ok && !c.ctrl.EnemyNear(c.player, cx, cy, c.engageRadius) {
			c.GoHome() // goal cleared of enemies → return home
		} else {
			c.issueAttack(c.goalX, c.goalY)
		}

	case CapRetreating:
		if c.IsEmpty() {
			c.state = CapHome
			return
		}
		cx, cy, ok := c.centroid()
		if ok && near(cx, cy, c.homeX, c.homeY, c.arriveRadius) {
			c.state = CapHome
		} else {
			c.issueMove(c.homeX, c.homeY)
		}
	}
}

// prune drops dead/absent members in place, preserving ascending order.
func (c *Captain) prune() {
	out := c.members[:0]
	for _, id := range c.members {
		if _, _, alive, _, _ := c.ctrl.UnitState(id); alive {
			out = append(out, id)
		}
	}
	c.members = out
}

// centroid returns the integer mean position of living members; ok=false when
// the roster is empty (no defined position).
func (c *Captain) centroid() (x, y int32, ok bool) {
	var sx, sy, n int64
	for _, id := range c.members {
		ux, uy, alive, _, _ := c.ctrl.UnitState(id)
		if !alive {
			continue
		}
		sx += int64(ux)
		sy += int64(uy)
		n++
	}
	if n == 0 {
		return 0, 0, false
	}
	return int32(sx / n), int32(sy / n), true
}

func (c *Captain) issueMove(x, y int32) {
	for _, id := range c.members {
		c.ctrl.OrderMoveTo(id, x, y)
	}
}
func (c *Captain) issueAttack(x, y int32) {
	for _, id := range c.members {
		c.ctrl.OrderAttackTo(id, x, y)
	}
}

// ---- view natives (query) ----

// State returns the current lifecycle state (for traces / FSV).
func (c *Captain) State() CaptainState { return c.state }

// InCombat reports whether the captain is engaged (CaptainInCombat).
func (c *Captain) InCombat() bool { return c.state == CapEngaged }

// Retreating reports whether the captain is returning home (CaptainRetreating).
func (c *Captain) Retreating() bool { return c.state == CapRetreating }

// AtGoal reports whether the captain's centroid has reached its attack goal
// (CaptainAtGoal). An empty captain is not at any goal.
func (c *Captain) AtGoal() bool {
	cx, cy, ok := c.centroid()
	return ok && near(cx, cy, c.goalX, c.goalY, c.arriveRadius)
}

// IsHome reports whether the captain is home (CaptainIsHome): staging (Home or
// Full) AND either empty or with its centroid at the staging point. An empty,
// freshly-spawned captain reads home.
func (c *Captain) IsHome() bool {
	if c.state != CapHome && c.state != CapFull {
		return false
	}
	cx, cy, ok := c.centroid()
	if !ok {
		return true // empty group at base
	}
	return near(cx, cy, c.homeX, c.homeY, c.arriveRadius)
}

// IsFull reports whether the roster has reached target strength (CaptainIsFull).
func (c *Captain) IsFull() bool { return len(c.members) >= c.capacity }

// IsEmpty reports whether the roster has no members (CaptainIsEmpty).
func (c *Captain) IsEmpty() bool { return len(c.members) == 0 }

// GroupSize returns the living member count (CaptainGroupSize).
func (c *Captain) GroupSize() int { return len(c.members) }

// VsUnits / VsPlayer report the captain's target classification
// (CaptainVsUnits / CaptainVsPlayer).
func (c *Captain) VsUnits() bool  { return c.tmode == TargetUnits }
func (c *Captain) VsPlayer() bool { return c.tmode == TargetPlayer }

// Readiness returns the captain's fighting strength as a percentage of full
// (CaptainReadiness): the sum of members' hp fractions over the target
// capacity, clamped to 0..100. Losses and damage both pull it down.
func (c *Captain) Readiness() int32 {
	if c.capacity <= 0 {
		return 0
	}
	var sum int64
	for _, id := range c.members {
		_, _, alive, hp, _ := c.ctrl.UnitState(id)
		if alive {
			sum += int64(hp)
		}
	}
	r := sum / int64(c.capacity)
	if r > 100 {
		r = 100
	}
	return int32(r)
}

// ReadinessHP returns the average hp percentage of living members
// (CaptainReadinessHP), 0 for an empty captain.
func (c *Captain) ReadinessHP() int32 {
	var sum, n int64
	for _, id := range c.members {
		_, _, alive, hp, _ := c.ctrl.UnitState(id)
		if alive {
			sum += int64(hp)
			n++
		}
	}
	if n == 0 {
		return 0
	}
	return int32(sum / n)
}

// ReadinessMa returns the average mana percentage of living mana-capable members
// (CaptainReadinessMa), or readinessMaNA (-1) when no member has a mana pool —
// honest "not applicable", never a fabricated value.
func (c *Captain) ReadinessMa() int32 {
	var sum, n int64
	for _, id := range c.members {
		_, _, alive, _, mp := c.ctrl.UnitState(id)
		if alive && mp >= 0 {
			sum += int64(mp)
			n++
		}
	}
	if n == 0 {
		return readinessMaNA
	}
	return int32(sum / n)
}

// Members returns a copy of the roster (caller may not mutate captain state).
func (c *Captain) Members() []int32 { return append([]int32(nil), c.members...) }

// ---- helpers ----

// near reports whether (ax,ay) is within r of (bx,by) by integer squared
// distance — no float inside the determinism boundary.
func near(ax, ay, bx, by, r int32) bool {
	dx := int64(ax - bx)
	dy := int64(ay - by)
	rr := int64(r) * int64(r)
	return dx*dx+dy*dy <= rr
}

// sortAscInt32 is an insertion sort (rosters are tiny) keeping members in
// ascending entity-id order after a recruit append, so membership is canonical.
func sortAscInt32(a []int32) {
	for i := 1; i < len(a); i++ {
		v := a[i]
		j := i - 1
		for j >= 0 && a[j] > v {
			a[j+1] = a[j]
			j--
		}
		a[j+1] = v
	}
}

// ---- CaptainCorps: the two-slot holder (CreateCaptains) ----

// CaptainCorps is one player's pair of captains (attack + defense), the
// CreateCaptains unit.
type CaptainCorps struct {
	player   int
	captains [numSlots]*Captain
}

// CreateCaptains builds both captains for player, each staged at home with cfg.
func CreateCaptains(ctrl CaptainControl, player int, homeX, homeY int32, cfg CaptainConfig) *CaptainCorps {
	return &CaptainCorps{
		player: player,
		captains: [numSlots]*Captain{
			NewCaptain(ctrl, player, SlotAttack, homeX, homeY, cfg),
			NewCaptain(ctrl, player, SlotDefense, homeX, homeY, cfg),
		},
	}
}

// Captain returns the captain in slot (nil for an out-of-range slot).
func (cc *CaptainCorps) Captain(slot CaptainSlot) *Captain {
	if int(slot) >= numSlots {
		return nil
	}
	return cc.captains[slot]
}

// Tick advances both captains in slot order (deterministic).
func (cc *CaptainCorps) Tick(now uint32) {
	for _, c := range cc.captains {
		c.Tick(now)
	}
}

// SetChanges applies the composition toggle to both captains.
func (cc *CaptainCorps) SetChanges(allow bool) {
	for _, c := range cc.captains {
		c.SetChanges(allow)
	}
}

// ---- serialization ----

var captainMagic = [8]byte{'L', 'I', 'T', 'D', 'C', 'A', 'P', 'T'}

const captainVersion uint16 = 1

var (
	errCaptainMagic   = errors.New("ai: bad captain save magic")
	errCaptainVersion = errors.New("ai: unsupported captain save version")
)

// Save serializes the captain's full running state so it resumes mid-march and
// reaches the identical arrival tick.
func (c *Captain) Save(dst []byte) []byte {
	dst = append(dst, captainMagic[:]...)
	dst = binary.LittleEndian.AppendUint16(dst, captainVersion)
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.player))
	dst = append(dst, byte(c.slot), byte(c.state), byte(c.tmode), boolByte(c.changes))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.homeX))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.homeY))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.goalX))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.goalY))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.capacity))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.target))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.retreatPct))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.engageRadius))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(c.arriveRadius))
	dst = binary.LittleEndian.AppendUint32(dst, c.lastTick)
	dst = binary.LittleEndian.AppendUint32(dst, uint32(len(c.members)))
	for _, id := range c.members {
		dst = binary.LittleEndian.AppendUint32(dst, uint32(id))
	}
	return dst
}

const captainHeader = 8 + 2 + 4 + 4 + 4*10 + 4 // magic,ver,player,4 flag-bytes,10 u32 scalars,memberCount

// Load restores captain state from blob. Fail-closed: parse into locals and
// validate the whole blob before committing, so a corrupt blob leaves the
// captain untouched. The control binding is preserved (not serialized).
func (c *Captain) Load(blob []byte) error {
	if len(blob) < captainHeader {
		return fmt.Errorf("ai: captain blob too short (%d bytes)", len(blob))
	}
	for i := range captainMagic {
		if blob[i] != captainMagic[i] {
			return errCaptainMagic
		}
	}
	off := 8
	if v := binary.LittleEndian.Uint16(blob[off:]); v != captainVersion {
		return fmt.Errorf("%w: %d (want %d)", errCaptainVersion, v, captainVersion)
	}
	off += 2
	player := int(int32(binary.LittleEndian.Uint32(blob[off:])))
	off += 4
	slot := CaptainSlot(blob[off])
	state := CaptainState(blob[off+1])
	tmode := TargetMode(blob[off+2])
	changes := blob[off+3] != 0
	off += 4
	if int(slot) >= numSlots || state > CapRetreating || tmode > TargetPlayer {
		return fmt.Errorf("ai: captain blob has invalid slot/state/tmode (%d/%d/%d)", slot, state, tmode)
	}
	rd := func() int32 { v := int32(binary.LittleEndian.Uint32(blob[off:])); off += 4; return v }
	homeX, homeY := rd(), rd()
	goalX, goalY := rd(), rd()
	capacity := int(rd())
	target := rd()
	retreatPct := rd()
	engageRadius := rd()
	arriveRadius := rd()
	lastTick := uint32(rd())
	n := int(uint32(rd()))
	if n < 0 || off+n*4 > len(blob) {
		return fmt.Errorf("ai: captain member list (%d) exceeds blob", n)
	}
	members := make([]int32, n)
	for i := 0; i < n; i++ {
		members[i] = int32(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
	}
	// validated — commit
	c.player, c.slot, c.state, c.tmode, c.changes = player, slot, state, tmode, changes
	c.homeX, c.homeY, c.goalX, c.goalY = homeX, homeY, goalX, goalY
	c.capacity, c.target = capacity, target
	c.retreatPct, c.engageRadius, c.arriveRadius = retreatPct, engageRadius, arriveRadius
	c.lastTick = lastTick
	c.members = members
	return nil
}

var corpsMagic = [8]byte{'L', 'I', 'T', 'D', 'C', 'P', 'C', 'O'}

const corpsVersion uint16 = 1

var (
	errCorpsMagic   = errors.New("ai: bad captain-corps save magic")
	errCorpsVersion = errors.New("ai: unsupported captain-corps save version")
)

// Save serializes both captains length-prefixed.
func (cc *CaptainCorps) Save(dst []byte) []byte {
	dst = append(dst, corpsMagic[:]...)
	dst = binary.LittleEndian.AppendUint16(dst, corpsVersion)
	dst = binary.LittleEndian.AppendUint32(dst, uint32(cc.player))
	for _, c := range cc.captains {
		blob := c.Save(nil)
		dst = binary.LittleEndian.AppendUint32(dst, uint32(len(blob)))
		dst = append(dst, blob...)
	}
	return dst
}

// Load restores both captains. Fail-closed: a sub-blob that fails to parse
// leaves the whole corps unchanged (parse into temporaries first).
func (cc *CaptainCorps) Load(blob []byte) error {
	const header = 8 + 2 + 4
	if len(blob) < header {
		return fmt.Errorf("ai: corps blob too short (%d bytes)", len(blob))
	}
	for i := range corpsMagic {
		if blob[i] != corpsMagic[i] {
			return errCorpsMagic
		}
	}
	off := 8
	if v := binary.LittleEndian.Uint16(blob[off:]); v != corpsVersion {
		return fmt.Errorf("%w: %d (want %d)", errCorpsVersion, v, corpsVersion)
	}
	off += 2
	player := int(int32(binary.LittleEndian.Uint32(blob[off:])))
	off += 4
	// Parse both sub-blobs into temporaries bound to the live controls.
	tmp := [numSlots]*Captain{}
	for i := 0; i < numSlots; i++ {
		if off+4 > len(blob) {
			return fmt.Errorf("ai: truncated corps captain %d length", i)
		}
		n := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		if n < 0 || off+n > len(blob) {
			return fmt.Errorf("ai: corps captain %d sub-blob (%d) exceeds remaining bytes", i, n)
		}
		cand := &Captain{ctrl: cc.captains[i].ctrl}
		if err := cand.Load(blob[off : off+n]); err != nil {
			return fmt.Errorf("ai: corps captain %d: %w", i, err)
		}
		cand.scratch = cc.captains[i].scratch
		tmp[i] = cand
		off += n
	}
	// validated — commit
	cc.player = player
	cc.captains = tmp
	return nil
}

func boolByte(b bool) byte {
	if b {
		return 1
	}
	return 0
}
