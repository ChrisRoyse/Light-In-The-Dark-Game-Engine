// Package input owns client-side gesture state. It never mutates sim
// state directly; gestures resolve to presentation selection or to
// serialized command records at the caller boundary.
package input

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

const (
	DefaultSelectionCap = 12
	MaxSelection        = sim.MaxCommandUnits
)

type SelectClass uint8

const (
	SelectNone SelectClass = iota
	SelectUnit
	SelectBuilding
)

type Rect struct {
	MinX float32 `json:"minX"`
	MinY float32 `json:"minY"`
	MaxX float32 `json:"maxX"`
	MaxY float32 `json:"maxY"`
}

type Selectable struct {
	ID          sim.EntityID `json:"id"`
	TypeID      uint16       `json:"typeID"`
	Class       SelectClass  `json:"class"`
	OwnerPlayer uint8        `json:"ownerPlayer"`
	LowPriority bool         `json:"lowPriority"`
	Screen      Rect         `json:"screen"`
}

type Config struct {
	LocalPlayer    uint8
	SelectionCap   int
	ClickTolerance float32
}

type Modifiers struct {
	Shift  bool
	Ctrl   bool
	Double bool
}

type Selection struct {
	IDs                   [MaxSelection]sim.EntityID `json:"-"`
	Count                 uint8                      `json:"count"`
	ActiveSubgroup        uint8                      `json:"activeSubgroup"`
	ActiveSubgroupTypeID  uint16                     `json:"activeSubgroupTypeID"`
	CommandRecordsEmitted uint16                     `json:"commandRecordsEmitted"`
}

type Result struct {
	Selection
	Changed             bool         `json:"changed"`
	Hit                 sim.EntityID `json:"hit,omitempty"`
	Candidates          uint16       `json:"candidates"`
	NormalPriority      uint16       `json:"normalPriority"`
	BuildingsConsidered uint16       `json:"buildingsConsidered"`
}

type Resolver struct {
	cfg            Config
	ids            [MaxSelection]sim.EntityID
	count          uint8
	activeSubgroup uint8
	activeTypeID   uint16
}

func DefaultConfig(localPlayer uint8) Config {
	return Config{LocalPlayer: localPlayer, SelectionCap: DefaultSelectionCap, ClickTolerance: 4}
}

func NewResolver(cfg Config) Resolver {
	if cfg.ClickTolerance < 0 {
		cfg.ClickTolerance = 0
	}
	return Resolver{cfg: cfg}
}

func NormalizeRect(r Rect) Rect {
	if r.MinX > r.MaxX {
		r.MinX, r.MaxX = r.MaxX, r.MinX
	}
	if r.MinY > r.MaxY {
		r.MinY, r.MaxY = r.MaxY, r.MinY
	}
	return r
}

func (r Rect) Intersects(o Rect) bool {
	r = NormalizeRect(r)
	o = NormalizeRect(o)
	return r.MinX <= o.MaxX && r.MaxX >= o.MinX && r.MinY <= o.MaxY && r.MaxY >= o.MinY
}

func (r Rect) Contains(x, y, tolerance float32) bool {
	r = NormalizeRect(r)
	return x >= r.MinX-tolerance && x <= r.MaxX+tolerance && y >= r.MinY-tolerance && y <= r.MaxY+tolerance
}

func (r Rect) Center() (float32, float32) {
	r = NormalizeRect(r)
	return (r.MinX + r.MaxX) * 0.5, (r.MinY + r.MaxY) * 0.5
}

func (r *Resolver) Selection() Selection {
	var out Selection
	out.Count = r.count
	out.ActiveSubgroup = r.activeSubgroup
	out.ActiveSubgroupTypeID = r.activeTypeID
	copy(out.IDs[:], r.ids[:])
	return out
}

func (r *Resolver) SetSelection(ids []sim.EntityID, items []Selectable) Result {
	next := [MaxSelection]sim.EntityID{}
	n := 0
	capacity := r.selectionCap()
	for _, id := range ids {
		if id == 0 || containsID(next[:], n, id) || n >= capacity {
			continue
		}
		next[n] = id
		n++
	}
	changed := r.apply(next, n, items, true)
	return r.result(0, changed, 0, 0, 0)
}

func (r *Resolver) Click(items []Selectable, x, y float32, mods Modifiers) Result {
	hit, ok := r.pickClick(items, x, y)
	if !ok {
		if mods.Shift {
			return r.result(0, false, 0, 0, 0)
		}
		changed := r.apply([MaxSelection]sim.EntityID{}, 0, items, true)
		return r.result(0, changed, 0, 0, 0)
	}
	if mods.Shift {
		changed := r.toggle(hit.ID, items)
		return r.result(hit.ID, changed, 1, 0, 0)
	}
	if mods.Ctrl || mods.Double {
		return r.selectType(items, hit)
	}
	var next [MaxSelection]sim.EntityID
	next[0] = hit.ID
	changed := r.apply(next, 1, items, true)
	return r.result(hit.ID, changed, 1, 0, 0)
}

func (r *Resolver) Drag(items []Selectable, rect Rect, mods Modifiers) Result {
	rect = NormalizeRect(rect)
	class, candidates, normal, buildings := r.dragClass(items, rect)
	if candidates == 0 {
		if mods.Shift {
			return r.result(0, false, 0, 0, 0)
		}
		changed := r.apply([MaxSelection]sim.EntityID{}, 0, items, true)
		return r.result(0, changed, 0, 0, buildings)
	}

	next := [MaxSelection]sim.EntityID{}
	n := 0
	capacity := r.selectionCap()
	if mods.Shift {
		for i := 0; i < int(r.count) && n < capacity; i++ {
			next[n] = r.ids[i]
			n++
		}
	}
	cx, cy := rect.Center()
	for n < capacity {
		best := -1
		var bestDist float32
		var bestID sim.EntityID
		for i := range items {
			it := items[i]
			if !r.dragCandidate(it, rect, class, normal > 0) || containsID(next[:], n, it.ID) {
				continue
			}
			ix, iy := it.Screen.Center()
			d := distSq(ix, iy, cx, cy)
			if best == -1 || d < bestDist || (d == bestDist && it.ID < bestID) {
				best = i
				bestDist = d
				bestID = it.ID
			}
		}
		if best == -1 {
			break
		}
		next[n] = items[best].ID
		n++
	}
	changed := r.apply(next, n, items, true)
	return r.result(0, changed, candidates, normal, buildings)
}

func (r *Resolver) Tab(items []Selectable) Result {
	if r.count == 0 {
		return r.result(0, false, 0, 0, 0)
	}
	var types [MaxSelection]uint16
	typeCount := 0
	for i := 0; i < int(r.count); i++ {
		if typ, ok := typeOf(items, r.ids[i]); ok && !containsType(types[:], typeCount, typ) {
			types[typeCount] = typ
			typeCount++
		}
	}
	if typeCount <= 1 {
		return r.result(0, false, 0, 0, 0)
	}
	r.activeSubgroup = uint8((int(r.activeSubgroup) + 1) % typeCount)
	r.activeTypeID = types[r.activeSubgroup]
	return r.result(0, true, 0, 0, 0)
}

func (r *Resolver) selectType(items []Selectable, hit Selectable) Result {
	if hit.OwnerPlayer != r.cfg.LocalPlayer || hit.Class == SelectNone {
		var next [MaxSelection]sim.EntityID
		next[0] = hit.ID
		changed := r.apply(next, 1, items, true)
		return r.result(hit.ID, changed, 1, 0, 0)
	}
	next := [MaxSelection]sim.EntityID{}
	n := 0
	capacity := r.selectionCap()
	hx, hy := hit.Screen.Center()
	for n < capacity {
		best := -1
		var bestDist float32
		var bestID sim.EntityID
		for i := range items {
			it := items[i]
			if it.ID == 0 || it.OwnerPlayer != r.cfg.LocalPlayer || it.Class != hit.Class || it.TypeID != hit.TypeID || containsID(next[:], n, it.ID) {
				continue
			}
			ix, iy := it.Screen.Center()
			d := distSq(ix, iy, hx, hy)
			if best == -1 || d < bestDist || (d == bestDist && it.ID < bestID) {
				best = i
				bestDist = d
				bestID = it.ID
			}
		}
		if best == -1 {
			break
		}
		next[n] = items[best].ID
		n++
	}
	changed := r.apply(next, n, items, true)
	return r.result(hit.ID, changed, uint16(n), 0, 0)
}

func (r *Resolver) pickClick(items []Selectable, x, y float32) (Selectable, bool) {
	best := -1
	var bestDist float32
	var bestID sim.EntityID
	for i := range items {
		it := items[i]
		if it.ID == 0 || !it.Screen.Contains(x, y, r.cfg.ClickTolerance) {
			continue
		}
		cx, cy := it.Screen.Center()
		d := distSq(cx, cy, x, y)
		if best == -1 || d < bestDist || (d == bestDist && it.ID < bestID) {
			best = i
			bestDist = d
			bestID = it.ID
		}
	}
	if best == -1 {
		return Selectable{}, false
	}
	return items[best], true
}

func (r *Resolver) dragClass(items []Selectable, rect Rect) (SelectClass, uint16, uint16, uint16) {
	var units, buildings, normal uint16
	for i := range items {
		it := items[i]
		if it.ID == 0 || it.OwnerPlayer != r.cfg.LocalPlayer || !it.Screen.Intersects(rect) {
			continue
		}
		switch it.Class {
		case SelectUnit:
			units++
			if !it.LowPriority {
				normal++
			}
		case SelectBuilding:
			buildings++
		}
	}
	if units > 0 {
		return SelectUnit, units, normal, buildings
	}
	if buildings > 0 {
		return SelectBuilding, buildings, 0, buildings
	}
	return SelectNone, 0, 0, buildings
}

func (r *Resolver) dragCandidate(it Selectable, rect Rect, class SelectClass, excludeLow bool) bool {
	if it.ID == 0 || it.OwnerPlayer != r.cfg.LocalPlayer || it.Class != class || !it.Screen.Intersects(rect) {
		return false
	}
	if class == SelectUnit && excludeLow && it.LowPriority {
		return false
	}
	return true
}

func (r *Resolver) toggle(id sim.EntityID, items []Selectable) bool {
	next := [MaxSelection]sim.EntityID{}
	n := 0
	removed := false
	for i := 0; i < int(r.count); i++ {
		if r.ids[i] == id {
			removed = true
			continue
		}
		next[n] = r.ids[i]
		n++
	}
	if !removed && n < r.selectionCap() {
		next[n] = id
		n++
	}
	return r.apply(next, n, items, true)
}

func (r *Resolver) apply(next [MaxSelection]sim.EntityID, n int, items []Selectable, resetSubgroup bool) bool {
	if n < 0 {
		n = 0
	}
	if n > MaxSelection {
		n = MaxSelection
	}
	changed := int(r.count) != n
	if !changed {
		for i := 0; i < n; i++ {
			if r.ids[i] != next[i] {
				changed = true
				break
			}
		}
	}
	r.ids = next
	r.count = uint8(n)
	if resetSubgroup {
		r.activeSubgroup = 0
		r.activeTypeID = 0
		if n > 0 {
			r.activeTypeID, _ = typeOf(items, r.ids[0])
		}
	}
	return changed
}

func (r *Resolver) result(hit sim.EntityID, changed bool, candidates, normal, buildings uint16) Result {
	out := Result{
		Selection:           r.Selection(),
		Changed:             changed,
		Hit:                 hit,
		Candidates:          candidates,
		NormalPriority:      normal,
		BuildingsConsidered: buildings,
	}
	return out
}

func (r *Resolver) selectionCap() int {
	switch {
	case r.cfg.SelectionCap < 0:
		return DefaultSelectionCap
	case r.cfg.SelectionCap == 0:
		return MaxSelection
	case r.cfg.SelectionCap > MaxSelection:
		return MaxSelection
	default:
		return r.cfg.SelectionCap
	}
}

func containsID(ids []sim.EntityID, n int, id sim.EntityID) bool {
	for i := 0; i < n; i++ {
		if ids[i] == id {
			return true
		}
	}
	return false
}

func containsType(types []uint16, n int, typ uint16) bool {
	for i := 0; i < n; i++ {
		if types[i] == typ {
			return true
		}
	}
	return false
}

func typeOf(items []Selectable, id sim.EntityID) (uint16, bool) {
	for i := range items {
		if items[i].ID == id {
			return items[i].TypeID, true
		}
	}
	return 0, false
}

func distSq(ax, ay, bx, by float32) float32 {
	dx := ax - bx
	dy := ay - by
	return dx*dx + dy*dy
}
