package sim

import (
	"fmt"
	"io"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	FogHidden uint8 = iota
	FogExplored
	FogVisible
)

const (
	VisibilityInvisible uint8 = 1 << iota
	VisibilityTrueSight
)

const (
	FogCellPathingSize        int32 = 4
	FogGridSize               int32 = path.GridSize / FogCellPathingSize
	FogCellCount              int32 = FogGridSize * FogGridSize
	fogPackedStateBytes             = int(FogCellCount / 4)
	fogCycleBytes                   = int(FogCellCount / 8)
	DefaultVisibilityInterval       = DefaultAcquireInterval
)

// LastSeenBuilding is the per-player fog ghost record for a structure.
type LastSeenBuilding struct {
	Entity EntityID
	TypeID uint16
	Owner  uint8
	Pos    fixed.Vec2
	Used   bool
}

// VisibilityGrid is the authoritative per-player fog state. Fog states
// are packed at two bits per fog cell; the cycle buffer is an accumulator
// for phase-spread source stamping and is saved because it affects the
// next finalized grid.
type VisibilityGrid struct {
	state         []byte
	cycle         []byte
	entityFlags   []uint8
	lastSeen      []LastSeenBuilding
	lastSeenCount [MaxPlayers]int32
	every         uint16
}

func newVisibilityGrid(entityCap, lastSeenCap int) *VisibilityGrid {
	if lastSeenCap < 1 {
		lastSeenCap = 1
	}
	return &VisibilityGrid{
		state:       make([]byte, MaxPlayers*fogPackedStateBytes),
		cycle:       make([]byte, MaxPlayers*fogCycleBytes),
		entityFlags: make([]uint8, entityCap),
		lastSeen:    make([]LastSeenBuilding, MaxPlayers*lastSeenCap),
		every:       DefaultVisibilityInterval,
	}
}

func (v *VisibilityGrid) PreallocatedBytes() int {
	return len(v.state) + len(v.cycle) + len(v.entityFlags) + len(v.lastSeen)*32
}

func (v *VisibilityGrid) LastSeenCap() int {
	if v == nil {
		return 0
	}
	return len(v.lastSeen) / MaxPlayers
}

func (v *VisibilityGrid) SetInterval(n uint16) bool {
	if v == nil || n < 1 {
		return false
	}
	v.every = n
	return true
}

func (v *VisibilityGrid) Interval() uint16 {
	if v == nil || v.every == 0 {
		return DefaultVisibilityInterval
	}
	return v.every
}

func fogCellIndex(x, y int32) int32 { return y*FogGridSize + x }

func fogCellInBounds(x, y int32) bool {
	return x >= 0 && x < FogGridSize && y >= 0 && y < FogGridSize
}

func clampFogCell(v int32) int32 {
	if v < 0 {
		return 0
	}
	if v >= FogGridSize {
		return FogGridSize - 1
	}
	return v
}

func (v *VisibilityGrid) stateOffset(player uint8, cell int32) (int, uint) {
	i := int(player)*fogPackedStateBytes + int(cell>>2)
	return i, uint((cell & 3) * 2)
}

func (v *VisibilityGrid) cycleOffset(player uint8, cell int32) (int, uint) {
	i := int(player)*fogCycleBytes + int(cell>>3)
	return i, uint(cell & 7)
}

func (v *VisibilityGrid) StateCell(player uint8, x, y int32) uint8 {
	if v == nil || player >= MaxPlayers || !fogCellInBounds(x, y) {
		return FogHidden
	}
	i, shift := v.stateOffset(player, fogCellIndex(x, y))
	return (v.state[i] >> shift) & 0x03
}

func (v *VisibilityGrid) setStateCell(player uint8, cell int32, state uint8) {
	i, shift := v.stateOffset(player, cell)
	v.state[i] = (v.state[i] &^ (0x03 << shift)) | ((state & 0x03) << shift)
}

func (v *VisibilityGrid) markCycle(player uint8, cell int32) {
	i, shift := v.cycleOffset(player, cell)
	v.cycle[i] |= 1 << shift
}

func (v *VisibilityGrid) cycleMarked(player uint8, cell int32) bool {
	i, shift := v.cycleOffset(player, cell)
	return v.cycle[i]&(1<<shift) != 0
}

func (v *VisibilityGrid) clearCycle() {
	for i := range v.cycle {
		v.cycle[i] = 0
	}
}

func (v *VisibilityGrid) finalizeCycle() {
	for p := uint8(0); p < MaxPlayers; p++ {
		for cell := int32(0); cell < FogCellCount; cell++ {
			if v.cycleMarked(p, cell) {
				v.setStateCell(p, cell, FogVisible)
				continue
			}
			x, y := cell%FogGridSize, cell/FogGridSize
			if v.StateCell(p, x, y) == FogVisible {
				v.setStateCell(p, cell, FogExplored)
			}
		}
	}
	v.clearCycle()
}

func worldToPathCell(pos fixed.Vec2) (x, y int32, ok bool) {
	x = int32(pos.X.Floor() >> 5)
	y = int32(pos.Y.Floor() >> 5)
	return x, y, path.InBounds(x, y)
}

func worldToFogCell(pos fixed.Vec2) (x, y int32, ok bool) {
	px, py, ok := worldToPathCell(pos)
	if !ok {
		return 0, 0, false
	}
	return px / FogCellPathingSize, py / FogCellPathingSize, true
}

func fogCellCenter(cell int32) fixed.Vec2 {
	x := cell % FogGridSize
	y := cell / FogGridSize
	px := x*FogCellPathingSize + FogCellPathingSize/2
	py := y*FogCellPathingSize + FogCellPathingSize/2
	return CellCenter(py*path.GridSize + px)
}

func distSqLE(a, b fixed.Vec2, r fixed.F64) bool {
	hi, lo := fixed.DistSq(a, b)
	rHi, rLo := fixed.RadiusSq(r)
	return hi < rHi || (hi == rHi && lo <= rLo)
}

func (w *World) unitSightRadius(typeID uint16) fixed.F64 {
	if int(typeID) >= len(w.unitDefs) {
		return 0
	}
	def := &w.unitDefs[typeID]
	if w.IsNight() && def.SightNight > 0 {
		return def.SightNight
	}
	return def.SightDay
}

func (w *World) visibilitySystem() {
	v := w.Visibility
	if v == nil {
		return
	}
	every := uint32(v.Interval())
	w.visibilityStampDue(every)
	if w.tick%every == 0 {
		v.finalizeCycle()
		w.updateLastSeenBuildings()
	}
}

func (w *World) visibilityStampDue(every uint32) {
	t := w.Transforms
	for r := int32(0); r < t.count; r++ {
		id := t.Entity[r]
		if (w.tick+id.Index())%every != 0 {
			continue
		}
		w.stampEntityVision(id, t.Pos[r])
	}
}

// RecomputeVisibility rebuilds the visible set from all current sources
// and finalizes immediately. Map setup, tests, and save-load FSV use it
// to establish a tick-zero source of truth.
func (w *World) RecomputeVisibility() {
	if w.Visibility == nil {
		return
	}
	w.Visibility.clearCycle()
	t := w.Transforms
	for r := int32(0); r < t.count; r++ {
		w.stampEntityVision(t.Entity[r], t.Pos[r])
	}
	w.Visibility.finalizeCycle()
	w.updateLastSeenBuildings()
}

func (w *World) stampEntityVision(id EntityID, pos fixed.Vec2) {
	v := w.Visibility
	if v == nil {
		return
	}
	or := w.Owners.Row(id)
	ut := w.UnitTypes.Row(id)
	if or == -1 || ut == -1 {
		return
	}
	player := w.Owners.Player[or]
	if player >= MaxPlayers {
		return
	}
	radius := w.unitSightRadius(w.UnitTypes.TypeID[ut])
	if radius <= 0 {
		return
	}
	w.stampVision(player, id, pos, radius)
}

func (w *World) stampVision(player uint8, source EntityID, pos fixed.Vec2, radius fixed.F64) {
	v := w.Visibility
	px, py, ok := worldToPathCell(pos)
	if !ok {
		return
	}
	rCells := int32((radius.Floor() + 31) >> 5)
	minX := clampFogCell((px - rCells) / FogCellPathingSize)
	maxX := clampFogCell((px + rCells) / FogCellPathingSize)
	minY := clampFogCell((py - rCells) / FogCellPathingSize)
	maxY := clampFogCell((py + rCells) / FogCellPathingSize)
	sourceLevel := uint8(0)
	air := false
	if cr := w.Collisions.Row(source); cr != -1 {
		air = w.Collisions.PathFlags[cr]&PathAir != 0
	}
	if w.Grid != nil {
		sourceLevel = w.Grid.CliffLevel(px, py)
	}
	for fy := minY; fy <= maxY; fy++ {
		for fx := minX; fx <= maxX; fx++ {
			cell := fogCellIndex(fx, fy)
			center := fogCellCenter(cell)
			if !distSqLE(pos, center, radius) {
				continue
			}
			if !air && w.Grid != nil {
				cx := fx*FogCellPathingSize + FogCellPathingSize/2
				cy := fy*FogCellPathingSize + FogCellPathingSize/2
				if w.Grid.CliffLevel(cx, cy) > sourceLevel {
					continue
				}
			}
			v.markCycle(player, cell)
		}
	}
}

func (w *World) FogStateAt(player uint8, x, y int32) uint8 {
	if w.Visibility == nil {
		return FogHidden
	}
	return w.Visibility.StateCell(player, x, y)
}

func (w *World) FogStateAtWorld(player uint8, pos fixed.Vec2) uint8 {
	x, y, ok := worldToFogCell(pos)
	if !ok || w.Visibility == nil {
		return FogHidden
	}
	return w.Visibility.StateCell(player, x, y)
}

func (w *World) IsVisibleToPlayer(player uint8, pos fixed.Vec2) bool {
	return w.FogStateAtWorld(player, pos) == FogVisible
}

func (w *World) SetVisibilityFlags(id EntityID, flags uint8) bool {
	if w.Visibility == nil || !w.Ents.Alive(id) {
		return false
	}
	idx := id.Index()
	if idx >= uint32(len(w.Visibility.entityFlags)) {
		return false
	}
	w.Visibility.entityFlags[idx] = flags & (VisibilityInvisible | VisibilityTrueSight)
	return true
}

func (w *World) VisibilityFlags(id EntityID) uint8 {
	if w.Visibility == nil {
		return 0
	}
	idx := id.Index()
	if idx >= uint32(len(w.Visibility.entityFlags)) {
		return 0
	}
	return w.Visibility.entityFlags[idx]
}

func (w *World) CanSeeEntity(player uint8, id EntityID) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	or := w.Owners.Row(id)
	if or != -1 && w.Owners.Player[or] == player {
		return true
	}
	tr := w.Transforms.Row(id)
	if tr == -1 || !w.IsVisibleToPlayer(player, w.Transforms.Pos[tr]) {
		return false
	}
	if w.VisibilityFlags(id)&VisibilityInvisible == 0 {
		return true
	}
	return w.playerHasTrueSightAt(player, w.Transforms.Pos[tr])
}

func (w *World) playerHasTrueSightAt(player uint8, pos fixed.Vec2) bool {
	t := w.Transforms
	for r := int32(0); r < t.count; r++ {
		id := t.Entity[r]
		if w.VisibilityFlags(id)&VisibilityTrueSight == 0 {
			continue
		}
		or := w.Owners.Row(id)
		ut := w.UnitTypes.Row(id)
		if or == -1 || ut == -1 || w.Owners.Player[or] != player {
			continue
		}
		radius := w.unitSightRadius(w.UnitTypes.TypeID[ut])
		if radius <= 0 {
			continue
		}
		if distSqLE(t.Pos[r], pos, radius) && w.losAllows(t.Entity[r], t.Pos[r], pos) {
			return true
		}
	}
	return false
}

func (w *World) losAllows(source EntityID, from, to fixed.Vec2) bool {
	if w.Grid == nil {
		return true
	}
	fx, fy, ok := worldToPathCell(from)
	if !ok {
		return false
	}
	tx, ty, ok := worldToPathCell(to)
	if !ok {
		return false
	}
	if cr := w.Collisions.Row(source); cr != -1 && w.Collisions.PathFlags[cr]&PathAir != 0 {
		return true
	}
	return w.Grid.CliffLevel(tx, ty) <= w.Grid.CliffLevel(fx, fy)
}

func (w *World) visibilityGatesGameplay() bool {
	return w.Visibility != nil && w.Grid != nil && len(w.unitDefs) > 0
}

func (w *World) updateLastSeenBuildings() {
	v := w.Visibility
	if v == nil {
		return
	}
	for player := uint8(0); player < MaxPlayers; player++ {
		w.clearRescoutedMissingBuildings(player)
	}
	b := w.Build
	for r := int32(0); r < b.count; r++ {
		id := b.Entity[r]
		ut := w.UnitTypes.Row(id)
		or := w.Owners.Row(id)
		tr := w.Transforms.Row(id)
		if ut == -1 || or == -1 || tr == -1 {
			continue
		}
		owner := w.Owners.Player[or]
		for player := uint8(0); player < MaxPlayers; player++ {
			if player == owner || !w.IsVisibleToPlayer(player, w.Transforms.Pos[tr]) {
				continue
			}
			v.upsertLastSeen(player, LastSeenBuilding{
				Entity: id,
				TypeID: w.UnitTypes.TypeID[ut],
				Owner:  owner,
				Pos:    w.Transforms.Pos[tr],
				Used:   true,
			})
		}
	}
}

func (w *World) clearRescoutedMissingBuildings(player uint8) {
	v := w.Visibility
	capPer := v.LastSeenCap()
	base := int(player) * capPer
	for i := int32(0); i < v.lastSeenCount[player]; {
		rec := &v.lastSeen[base+int(i)]
		if !w.IsVisibleToPlayer(player, rec.Pos) || w.Build.Row(rec.Entity) != -1 {
			i++
			continue
		}
		v.removeLastSeenAt(player, i)
	}
}

func (v *VisibilityGrid) upsertLastSeen(player uint8, rec LastSeenBuilding) bool {
	capPer := v.LastSeenCap()
	base := int(player) * capPer
	for i := int32(0); i < v.lastSeenCount[player]; i++ {
		if v.lastSeen[base+int(i)].Entity == rec.Entity {
			v.lastSeen[base+int(i)] = rec
			return true
		}
	}
	if int(v.lastSeenCount[player]) >= capPer {
		return false
	}
	v.lastSeen[base+int(v.lastSeenCount[player])] = rec
	v.lastSeenCount[player]++
	return true
}

func (v *VisibilityGrid) removeLastSeenAt(player uint8, row int32) {
	capPer := v.LastSeenCap()
	base := int(player) * capPer
	last := v.lastSeenCount[player] - 1
	if row != last {
		v.lastSeen[base+int(row)] = v.lastSeen[base+int(last)]
	}
	v.lastSeen[base+int(last)] = LastSeenBuilding{}
	v.lastSeenCount[player]--
}

func (v *VisibilityGrid) LastSeen(player uint8, entity EntityID) (LastSeenBuilding, bool) {
	if v == nil || player >= MaxPlayers {
		return LastSeenBuilding{}, false
	}
	capPer := v.LastSeenCap()
	base := int(player) * capPer
	for i := int32(0); i < v.lastSeenCount[player]; i++ {
		rec := v.lastSeen[base+int(i)]
		if rec.Entity == entity && rec.Used {
			return rec, true
		}
	}
	return LastSeenBuilding{}, false
}

func (v *VisibilityGrid) HashInto(h *statehash.Hasher) {
	if v == nil {
		h.WriteBool(false)
		return
	}
	h.WriteBool(true)
	h.WriteU16(v.Interval())
	h.WriteBytes(v.state)
	h.WriteBytes(v.cycle)
	h.WriteU32(uint32(len(v.entityFlags)))
	for i := range v.entityFlags {
		h.WriteU8(v.entityFlags[i])
	}
	h.WriteU32(uint32(v.LastSeenCap()))
	for player := 0; player < MaxPlayers; player++ {
		h.WriteU32(uint32(v.lastSeenCount[player]))
		base := player * v.LastSeenCap()
		for i := int32(0); i < v.lastSeenCount[player]; i++ {
			rec := &v.lastSeen[base+int(i)]
			h.WriteBool(rec.Used)
			h.WriteU32(uint32(rec.Entity))
			h.WriteU16(rec.TypeID)
			h.WriteU8(rec.Owner)
			h.WriteI64(int64(rec.Pos.X))
			h.WriteI64(int64(rec.Pos.Y))
		}
	}
}

func (w *World) saveVisibility(s *saveWriter) {
	v := w.Visibility
	if v == nil {
		s.boolean(false)
		return
	}
	s.boolean(true)
	s.u16(v.Interval())
	s.u32(uint32(len(v.state)))
	s.write(v.state)
	s.u32(uint32(len(v.cycle)))
	s.write(v.cycle)
	s.u32(uint32(len(v.entityFlags)))
	s.write(v.entityFlags)
	s.u32(uint32(v.LastSeenCap()))
	for player := 0; player < MaxPlayers; player++ {
		s.u32(uint32(v.lastSeenCount[player]))
		base := player * v.LastSeenCap()
		for i := int32(0); i < v.lastSeenCount[player]; i++ {
			rec := &v.lastSeen[base+int(i)]
			s.boolean(rec.Used)
			s.ent(rec.Entity)
			s.u16(rec.TypeID)
			s.u8(rec.Owner)
			s.vec2(rec.Pos)
		}
	}
}

func readVisibility(r *saveReader, w *World, d *decodedSave) error {
	r.what = "visibility present"
	d.visPresent = r.boolean()
	if r.err != nil {
		return r.err
	}
	if !d.visPresent {
		return nil
	}
	r.what = "visibility interval"
	d.visEvery = r.u16()
	if d.visEvery == 0 {
		return fmt.Errorf("sim: save: visibility interval is zero")
	}
	stateLen := r.u32()
	if int(stateLen) != len(w.Visibility.state) {
		return fmt.Errorf("sim: save: visibility state length %d, want %d", stateLen, len(w.Visibility.state))
	}
	d.visState = make([]byte, int(stateLen))
	if _, err := io.ReadFull(r.r, d.visState); err != nil {
		return fmt.Errorf("sim: save: truncated while reading visibility state")
	}
	cycleLen := r.u32()
	if int(cycleLen) != len(w.Visibility.cycle) {
		return fmt.Errorf("sim: save: visibility cycle length %d, want %d", cycleLen, len(w.Visibility.cycle))
	}
	d.visCycle = make([]byte, int(cycleLen))
	if _, err := io.ReadFull(r.r, d.visCycle); err != nil {
		return fmt.Errorf("sim: save: truncated while reading visibility cycle")
	}
	flagLen := r.u32()
	if int(flagLen) != len(w.Visibility.entityFlags) {
		return fmt.Errorf("sim: save: visibility flag length %d, want %d", flagLen, len(w.Visibility.entityFlags))
	}
	d.visFlags = make([]byte, int(flagLen))
	if _, err := io.ReadFull(r.r, d.visFlags); err != nil {
		return fmt.Errorf("sim: save: truncated while reading visibility entity flags")
	}
	capPer := r.u32()
	if int(capPer) != w.Visibility.LastSeenCap() {
		return fmt.Errorf("sim: save: visibility last-seen cap %d, want %d", capPer, w.Visibility.LastSeenCap())
	}
	d.visLastSeen = make([]LastSeenBuilding, MaxPlayers*int(capPer))
	for player := 0; player < MaxPlayers; player++ {
		count := r.u32()
		if count > capPer {
			return fmt.Errorf("sim: save: visibility player %d last-seen count %d exceeds cap %d", player, count, capPer)
		}
		d.visLastSeenCount[player] = int32(count)
		base := player * int(capPer)
		for i := uint32(0); i < count; i++ {
			rec := &d.visLastSeen[base+int(i)]
			rec.Used = r.boolean()
			rec.Entity = r.ent()
			rec.TypeID = r.u16()
			rec.Owner = r.u8()
			rec.Pos = r.vec2()
		}
	}
	return r.err
}

func validateVisibilitySave(d *decodedSave, w *World, entAlive func(EntityID) bool) error {
	if !d.visPresent {
		return nil
	}
	for i, flags := range d.visFlags {
		if flags&^(VisibilityInvisible|VisibilityTrueSight) != 0 {
			return fmt.Errorf("sim: save: visibility flags for entity index %d contain unknown bits %02x", i, flags)
		}
	}
	for player := 0; player < MaxPlayers; player++ {
		base := player * w.Visibility.LastSeenCap()
		for i := int32(0); i < d.visLastSeenCount[player]; i++ {
			rec := &d.visLastSeen[base+int(i)]
			if !rec.Used {
				return fmt.Errorf("sim: save: visibility player %d last-seen row %d is unused in live prefix", player, i)
			}
			if rec.Owner >= MaxPlayers {
				return fmt.Errorf("sim: save: visibility player %d last-seen row %d owner %d out of range", player, i, rec.Owner)
			}
			if rec.Entity != 0 && rec.Entity.Index() >= uint32(len(d.entSlots)) {
				return fmt.Errorf("sim: save: visibility player %d last-seen row %d entity %d out of range", player, i, rec.Entity)
			}
			if rec.Entity != 0 && entAlive(rec.Entity) {
				found := false
				for _, id := range d.bdE {
					if id == rec.Entity {
						found = true
						break
					}
				}
				if !found {
					return fmt.Errorf("sim: save: visibility last-seen live entity %d is not a building", rec.Entity)
				}
			}
		}
	}
	return nil
}

func applyVisibilitySave(d *decodedSave, w *World) {
	v := w.Visibility
	if v == nil || !d.visPresent {
		return
	}
	v.every = d.visEvery
	copy(v.state, d.visState)
	copy(v.cycle, d.visCycle)
	copy(v.entityFlags, d.visFlags)
	for player := 0; player < MaxPlayers; player++ {
		v.lastSeenCount[player] = 0
	}
	for i := range v.lastSeen {
		v.lastSeen[i] = LastSeenBuilding{}
	}
	copy(v.lastSeen, d.visLastSeen)
	v.lastSeenCount = d.visLastSeenCount
}
