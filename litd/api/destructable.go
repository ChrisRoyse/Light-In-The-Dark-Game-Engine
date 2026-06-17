package litd

// Destructable surface (#229, destructables-and-doodads.md). A destructable is
// a killable, optionally pathing-blocking world widget — a tree, gate, or
// breakable rock. Life is the deterministic source of truth in the sim; killing
// a blocking destructable frees its pathing footprint the same tick. Percent
// life variants are tombstoned in the mapping — callers write
// d.SetLife(int(float64(d.MaxLife()) * pct)).

// DestructableOptions configures a CreateDestructable call. Type is a content
// id (the typed data-table key lands with the destructable data tables in v2);
// Life seeds both current and max life. Footprint is the square pathing
// footprint side in cells and is only stamped when BlocksPathing is set.
type DestructableOptions struct {
	Type          uint16
	Pos           Vec2
	Facing        Angle
	Life          int
	BlocksPathing bool
	Footprint     int
}

// CreateDestructable spawns a destructable. It returns the zero (invalid)
// handle on pool/entity exhaustion (R-API-5) — never a silent partial spawn.
// JASS: CreateDestructable, CreateDestructableLoc, CreateDestructableZ
func (g *Game) CreateDestructable(o DestructableOptions) Destructable {
	if g == nil || g.w == nil {
		return Destructable{}
	}
	foot := o.Footprint
	if foot < 0 {
		foot = 0
	}
	if foot > 255 {
		foot = 255
	}
	id := g.w.CreateDestructable(o.Type, vec(o.Pos), angleToBrad(o.Facing), int32(o.Life), o.BlocksPathing, uint8(foot))
	if id == 0 {
		g.reportInvalid("Game.CreateDestructable (pool exhausted)")
		return Destructable{}
	}
	return Destructable{id: id, g: g}
}

// Life returns the destructable's current life, or 0 for an invalid handle.
// JASS: GetDestructableLife
func (d Destructable) Life() int {
	if !d.Valid() {
		d.g.reportInvalid("Destructable.Life")
		return 0
	}
	return int(d.g.w.DestructableLife(d.id))
}

// MaxLife returns the destructable's maximum life, or 0 for an invalid handle.
// JASS: GetDestructableMaxLife
func (d Destructable) MaxLife() int {
	if !d.Valid() {
		d.g.reportInvalid("Destructable.MaxLife")
		return 0
	}
	return int(d.g.w.DestructableMaxLife(d.id))
}

// SetLife sets current life, clamped to [0, MaxLife]. Setting life to 0 does
// not kill — death (and the pathing release) is the explicit Kill verb.
// No-op on an invalid handle.
// JASS: SetDestructableLife
func (d Destructable) SetLife(v int) {
	if !d.Valid() {
		d.g.reportInvalid("Destructable.SetLife")
		return
	}
	d.g.w.SetDestructableLife(d.id, int32(v))
}

// Kill kills the destructable: life drops to 0 and, if it was blocking, its
// pathing footprint is freed the same tick. No-op on an invalid or already-dead
// handle.
// JASS: KillDestructable, RemoveDestructable
func (d Destructable) Kill() {
	if !d.Valid() {
		d.g.reportInvalid("Destructable.Kill")
		return
	}
	d.g.w.KillDestructable(d.id)
}

// Resurrect revives a dead destructable to full life, re-stamping its pathing
// footprint. No-op on an invalid or still-living handle.
// JASS: DestructableRestoreLife
func (d Destructable) Resurrect() {
	if !d.Valid() {
		d.g.reportInvalid("Destructable.Resurrect")
		return
	}
	d.g.w.ResurrectDestructable(d.id)
}

// Dead reports whether the destructable has been killed.
// JASS: IsDestructableAliveBJ, IsDestructableDeadBJ
func (d Destructable) Dead() bool {
	return d.Valid() && d.g.w.DestructableDead(d.id)
}

// Invulnerable reports the invulnerable flag.
// JASS: IsDestructableInvulnerable, IsDestructableInvulnerableBJ
func (d Destructable) Invulnerable() bool {
	return d.Valid() && d.g.w.DestructableInvulnerable(d.id)
}

// SetInvulnerable sets the invulnerable flag. No-op on an invalid handle.
// JASS: SetDestructableInvulnerable, SetDestructableInvulnerableBJ
func (d Destructable) SetInvulnerable(b bool) {
	if !d.Valid() {
		d.g.reportInvalid("Destructable.SetInvulnerable")
		return
	}
	d.g.w.SetDestructableInvulnerable(d.id, b)
}

// PlayAnimation requests a render-side animation. Destructable animation is
// cosmetic (render-only) and carries no deterministic sim state, so this is a
// render-domain forward — included for surface completeness; it never affects
// the state hash. No-op on an invalid handle.
// JASS: QueueDestructableAnimation, QueueDestructableAnimationBJ, SetDestructableAnimation, SetDestructableAnimationBJ
func (d Destructable) PlayAnimation(name string) {
	if !d.Valid() {
		d.g.reportInvalid("Destructable.PlayAnimation")
		return
	}
	// Render-only: the render mirror consumes destructable animation state.
	_ = name
}
