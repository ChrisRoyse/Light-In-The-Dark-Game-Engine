package sim

// Ability op interpreter — PRD2 06 (epic #549, #595). Executes a compiled
// AbilitySpec.OnCast by instantiating the five PRD2 primitives — projectile
// entity, mover (05), unit group (02), effect arena, custom event (04), KV
// (03), timer (01) — with NO per-ability engine code (R-ABL-1). Introduces
// no new sim state beyond the spec (read-only data) and the primitive
// instances it creates, so determinism / zero-alloc / serialization are all
// inherited from the primitives (R-ABL-6).
//
// after/loop/times defer their nested ops via the cooperative scheduler. The
// deferred block is addressed by a stable index into the AbilityBook (data,
// not a Go closure) and the reschedule period lives in the book too, so a
// parked DoT/channel round-trips save/load once registerAbilityDispatch
// re-binds contAbilityResume over the restored world — exactly like the
// trigger action-runner (trigger_dispatch.go).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// contAbilityResume is the sim-reserved ContID for deferred ability blocks.
// It sits in the engine-reserved range above the trigger continuations
// (1<<31, 1<<31+1) so the shared scheduler never collides them. State packs
// (blockIndex, caster, target, remaining); the period is data in the book.
const contAbilityResume sched.ContID = 1<<31 + 8

// abilityBlock is a deferred op-block: the nested ops plus the reschedule
// period in ticks (0 = one-shot). Addressed from an after/loop/times op's
// Block field.
type abilityBlock struct {
	ops    []AbilityOp
	period uint32
}

// AbilityBook holds compiled specs and their flattened deferred blocks.
// Read-only after setup (R-ABL-6): a cast only instantiates primitives. The
// host registers every spec at world setup (and again on a fresh world
// before LoadState) in a stable order, so block indices are reproducible and
// parked timers resume correctly on reload.
type AbilityBook struct {
	specs  []AbilitySpec
	byID   map[string]uint16 // setup-only id->index+1 (0=absent); keyed gets only, never ranged
	blocks []abilityBlock    // deferred blocks; index 0 is the reserved "none"
}

// NewAbilityBook returns an empty book with the reserved block-0 slot.
func NewAbilityBook() *AbilityBook {
	return &AbilityBook{byID: make(map[string]uint16), blocks: make([]abilityBlock, 1)}
}

// RegisterSpec stores a compiled spec, indexes its deferred op-blocks, and
// returns the spec index. Setup-only — never called in the tick.
func (bk *AbilityBook) RegisterSpec(spec AbilitySpec) uint16 {
	idx := uint16(len(bk.specs))
	bk.indexBlocks(spec.OnCast)
	bk.specs = append(bk.specs, spec)
	if spec.ID != "" {
		bk.byID[spec.ID] = idx + 1
	}
	return idx
}

// Lookup resolves an ability id to its spec index. Setup/order-issue time;
// a keyed map get is deterministic (the map is never ranged).
func (bk *AbilityBook) Lookup(id string) (uint16, bool) {
	v, ok := bk.byID[id]
	if !ok {
		return 0, false
	}
	return v - 1, true
}

// Spec returns a pointer to a registered spec (read-only) or nil.
func (bk *AbilityBook) Spec(idx uint16) *AbilitySpec {
	if int(idx) >= len(bk.specs) {
		return nil
	}
	return &bk.specs[idx]
}

// SpecCount / BlockCount expose the registry size (FSV / fingerprint).
func (bk *AbilityBook) SpecCount() int  { return len(bk.specs) }
func (bk *AbilityBook) BlockCount() int { return len(bk.blocks) - 1 }

// indexBlocks recursively assigns Block indices to after/loop/times ops and
// records their children + reschedule period. The period encodes the op
// kind: after = one-shot (0); times repeats every Arg ticks; loop repeats
// every Count ticks.
func (bk *AbilityBook) indexBlocks(ops []AbilityOp) {
	for i := range ops {
		op := &ops[i]
		switch op.Kind {
		case OpAfter:
			op.Block = uint16(len(bk.blocks))
			bk.blocks = append(bk.blocks, abilityBlock{ops: op.Children, period: 0})
		case OpTimes:
			op.Block = uint16(len(bk.blocks))
			bk.blocks = append(bk.blocks, abilityBlock{ops: op.Children, period: posTicks(op.Arg)})
		case OpLoop:
			op.Block = uint16(len(bk.blocks))
			bk.blocks = append(bk.blocks, abilityBlock{ops: op.Children, period: posTicks(int64(op.Count))})
		}
		if len(op.Children) > 0 {
			bk.indexBlocks(op.Children)
		}
	}
}

// ---- runtime ----

// abilityCtx is the per-cast execution context. Value type, lives on the
// stack — a for_each iteration copies it so each member runs with its own
// target/point without heap traffic.
type abilityCtx struct {
	caster EntityID
	target EntityID
	point  fixed.Vec2
	dir    fixed.Vec2
	proj   EntityID // last spawned projectile (attach_mover targets it)
	group  GroupID  // active group for fill_group / for_each_in_group
	lastKV int64    // last get_kv value (if predicate fallback)
}

// CastAbility executes spec[specIndex].OnCast for caster against an optional
// target entity (0 = point cast) at point. Returns false on a bad index.
// This is the op-interpreter entry point (#595); the cast state machine
// (precast/cooldown/mana) is wired separately.
func (w *World) CastAbility(specIndex uint16, caster, target EntityID, point fixed.Vec2) bool {
	bk := w.AbilityDefs
	if bk == nil || int(specIndex) >= len(bk.specs) {
		return false
	}
	spec := &bk.specs[specIndex]
	ctx := w.newAbilityCtx(caster, target, point)
	w.runAbilityOps(spec.OnCast, &ctx)
	w.freeAbilityCtx(&ctx)
	return true
}

// freeAbilityCtx releases the per-cast scratch group so a cast leaves no
// persistent state behind (R-ABL-6) — fill_group's group is transient
// targeting scratch, not an author-owned handle. Auto-freeing keeps the
// group pool from leaking one slot per AoE cast.
func (w *World) freeAbilityCtx(ctx *abilityCtx) {
	if ctx.group != 0 && w.Groups != nil {
		w.Groups.DestroyGroup(ctx.group)
		ctx.group = 0
	}
}

// newAbilityCtx builds a context: a target unit defaults the point to its
// position; direction is caster→point.
func (w *World) newAbilityCtx(caster, target EntityID, point fixed.Vec2) abilityCtx {
	ctx := abilityCtx{caster: caster, target: target, point: point}
	if target != 0 {
		if r := w.Transforms.Row(target); r != -1 {
			ctx.point = w.Transforms.Pos[r]
		}
	}
	if cr := w.Transforms.Row(caster); cr != -1 {
		ctx.dir = ctx.point.Sub(w.Transforms.Pos[cr])
	}
	return ctx
}

func (w *World) runAbilityOps(ops []AbilityOp, ctx *abilityCtx) {
	for i := range ops {
		w.runAbilityOp(&ops[i], ctx)
	}
}

func (w *World) runAbilityOp(op *AbilityOp, ctx *abilityCtx) {
	switch op.Kind {
	case OpSpawnProjectile:
		pos := ctx.point
		if cr := w.Transforms.Row(ctx.caster); cr != -1 {
			pos = w.Transforms.Pos[cr]
		}
		facing := fixed.Atan2(ctx.dir.Y, ctx.dir.X)
		if e, ok := w.CreateUnit(pos, facing); ok {
			ctx.proj = e
		}
	case OpAttachMover:
		w.abilityAttachMover(op, ctx)
	case OpFillGroup:
		w.abilityFillGroup(op, ctx)
	case OpRunEffects:
		w.ExecuteEffects(op.EffectList, EffectCtx{Source: ctx.caster, Target: ctx.target, Point: ctx.point})
	case OpEmitEvent:
		w.Emit(Event{Kind: op.EventKind, Src: ctx.caster, Dst: ctx.target, Arg: op.Arg})
	case OpSetKV:
		if w.KV != nil {
			w.KV.KVSet(EntityKVOwner(ctx.caster), op.KeyID, KVInt, op.Arg, 0)
		}
	case OpGetKV:
		ctx.lastKV = 0
		if w.KV != nil {
			if _, v, _, ok := w.KV.KVGet(EntityKVOwner(ctx.caster), op.KeyID); ok {
				ctx.lastKV = v
			}
		}
	case OpAfter, OpTimes, OpLoop:
		w.abilitySchedule(op, ctx)
	case OpForEachInGroup:
		w.abilityForEach(op, ctx)
	case OpIf:
		if w.abilityPredicate(op, ctx) {
			w.runAbilityOps(op.Children, ctx)
		}
	}
}

// abilityAttachMover instantiates a mover (05) carrying the op's payload.
// Target defaults to the last spawned projectile, else the caster. Homing/
// orbit anchor on the cast target; point movers aim at the cast point.
func (w *World) abilityAttachMover(op *AbilityOp, ctx *abilityCtx) {
	if w.Movers == nil {
		return
	}
	target := ctx.proj
	if target == 0 {
		target = ctx.caster
	}
	spec := MoverSpec{
		Kind: op.MoverKind, Target: target, Owner: ctx.caster,
		Dir: ctx.dir, Speed: op.Speed, RangeLeft: op.Range, Radius: op.Radius,
		HitMask: op.HitMask, Pierce: op.Pierce, Payload: op.EffectList,
		AngVel: op.AngVel, TurnRate: op.TurnRate, Height: op.Height,
		Decay: op.Decay, DoneMode: op.DoneMode, OnDone: op.OnDone,
	}
	// A mover carrying a spawned projectile owns that body's lifetime: consume
	// it on completion so projectiles don't leak unit slots (R-ABL-6). A mover
	// attached to the caster/target unit never consumes it.
	if ctx.proj != 0 && target == ctx.proj {
		spec.Flags |= MoverConsume
	}
	switch op.MoverKind {
	case MoverHoming, MoverOrbitUnit:
		spec.Anchor = ctx.target
	case MoverPoint:
		spec.Goal = ctx.point
	}
	// Spline movers carry their control points; file them into the shared
	// waypoint arena at cast time (#622).
	if len(op.Waypoints) > 0 {
		if start, length, ok := w.Movers.AddWaypoints(op.Waypoints); ok {
			spec.WpStart = start
			spec.WpLen = length
		}
	}
	w.Movers.Create(spec)
}

// abilityFillGroup populates the context group by radius around the cast
// point, filtered by the op's hit mask (02). Reuses one group per cast.
func (w *World) abilityFillGroup(op *AbilityOp, ctx *abilityCtx) {
	if w.Groups == nil {
		return
	}
	if ctx.group == 0 {
		ctx.group = w.Groups.CreateGroup()
		if ctx.group == 0 {
			return
		}
	} else {
		w.Groups.GroupClear(ctx.group)
	}
	m := w.maskFromHit(op.HitMask, ctx.caster)
	if op.Count > 0 {
		m.Max = op.Count
	}
	w.GroupFillRadius(ctx.group, ctx.point, op.Radius, m)
}

// abilityForEach runs the nested ops once per group member, each with the
// member as target. Iterates the membership span directly (no closure) so
// the loop is zero-alloc (R-ABL-6).
func (w *World) abilityForEach(op *AbilityOp, ctx *abilityCtx) {
	g := w.Groups
	if g == nil || ctx.group == 0 {
		return
	}
	row, ok := g.resolve(ctx.group)
	if !ok {
		return
	}
	start := g.Start[row]
	for i := int32(0); i < g.Len[row]; i++ {
		e := g.Members[start+i]
		child := *ctx
		child.target = e
		if r := w.Transforms.Row(e); r != -1 {
			child.point = w.Transforms.Pos[r]
		}
		w.runAbilityOps(op.Children, &child)
	}
}

// abilitySchedule parks a deferred block on the scheduler. after runs the
// block once after Count ticks; times runs it Count times every Arg ticks;
// loop runs it Arg times every Count ticks. iters==0 or no block → no-op.
func (w *World) abilitySchedule(op *AbilityOp, ctx *abilityCtx) {
	if op.Block == 0 {
		return
	}
	var firstDelay, iters uint32
	switch op.Kind {
	case OpAfter:
		firstDelay, iters = posTicks(int64(op.Count)), 1
	case OpTimes:
		firstDelay, iters = posTicks(op.Arg), uint32(maxI32(op.Count))
	case OpLoop:
		firstDelay, iters = posTicks(int64(op.Count)), uint32(maxI32(int32(op.Arg)))
	}
	if iters == 0 {
		return
	}
	if firstDelay == 0 {
		firstDelay = 1
	}
	w.Sched.After(firstDelay, contAbilityResume, packAbilityResume(op.Block, ctx.caster, ctx.target, iters))
}

// abilityPredicate evaluates an if op: true when the KV value at the op's
// key (or the last get_kv) equals the op's Arg. A missing key is false
// (fail-closed branch).
func (w *World) abilityPredicate(op *AbilityOp, ctx *abilityCtx) bool {
	val := ctx.lastKV
	if op.KeyID != 0 {
		if w.KV == nil {
			return false
		}
		_, v, _, ok := w.KV.KVGet(EntityKVOwner(ctx.caster), op.KeyID)
		if !ok {
			return false
		}
		val = v
	}
	return val == op.Arg
}

// registerAbilityDispatch binds contAbilityResume on the world's scheduler.
// Called once at construction (and again on a fresh world before LoadState),
// so a parked DoT/channel relinks to the same stable ContID over the live
// world — the AbilityBook (re-registered identically at setup) supplies the
// block ops and period.
func (w *World) registerAbilityDispatch() {
	w.Sched.Register(contAbilityResume, func(s *sched.Scheduler, st sched.State) {
		block, caster, target, remaining := unpackAbilityResume(st)
		bk := w.AbilityDefs
		if bk == nil || int(block) >= len(bk.blocks) {
			return
		}
		b := bk.blocks[block]
		ctx := w.newAbilityCtx(caster, target, fixed.Vec2{})
		w.runAbilityOps(b.ops, &ctx)
		w.freeAbilityCtx(&ctx)
		if remaining > 1 && b.period > 0 {
			w.Sched.After(b.period, contAbilityResume, packAbilityResume(block, caster, target, remaining-1))
		}
	})
}

// packAbilityResume / unpackAbilityResume encode a parked block into the
// scheduler's value-typed State (no pointers, so it serializes directly).
func packAbilityResume(block uint16, caster, target EntityID, remaining uint32) sched.State {
	return sched.State{int64(block), int64(uint64(caster)), int64(uint64(target)), int64(uint64(remaining))}
}

func unpackAbilityResume(st sched.State) (block uint16, caster, target EntityID, remaining uint32) {
	return uint16(st[0]), EntityID(uint64(st[1])), EntityID(uint64(st[2])), uint32(uint64(st[3]))
}

// maskFromHit translates a missile-style hit mask into a unit-query mask
// relative to the caster's player. 0 (or no relation bit) means enemy-only.
func (w *World) maskFromHit(hit uint16, caster EntityID) QueryMask {
	var m QueryMask
	if or := w.Owners.Row(caster); or != -1 {
		m.OfPlayer = w.Owners.Player[or]
	}
	if hit&MissileHitAlly != 0 {
		m.Ally = true
	}
	if hit&MissileHitEnemy != 0 || hit&MissileHitRelationMask == 0 {
		m.Enemy = true
	}
	// class filters: presence of class bits restricts to those classes.
	if cls := hit & MissileHitClassMask; cls != 0 {
		if cls&MissileHitAir == 0 {
			m.ExcludeFlying = true
		}
		if cls&MissileHitStructure != 0 && cls&MissileHitGround == 0 && cls&MissileHitAir == 0 {
			m.StructuresOnly = true
		}
	}
	return m
}

// posTicks clamps a signed tick count to a non-negative uint32.
func posTicks(t int64) uint32 {
	if t <= 0 {
		return 0
	}
	return uint32(t)
}

// maxI32 clamps a negative int32 to 0.
func maxI32(v int32) int32 {
	if v < 0 {
		return 0
	}
	return v
}
