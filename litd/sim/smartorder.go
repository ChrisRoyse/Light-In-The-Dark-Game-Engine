package sim

// Smart-order resolution (input.md §4, combat-and-orders.md §2.2):
// right-click → exactly one concrete opcode per capability class in
// the selection, resolved by the data-driven table BEFORE the command
// record is encoded. The raw click never enters sim state — the sim's
// input vocabulary stays the closed §8 opcode set, and replays carry
// the resolved order.
//
// Resolution is a pure function over read-only sim state. A target
// that died between click and ingest resolves to NOTHING (ok=false):
// the deterministic no-op — no record enters the stream, no state
// changes. Classification the sim cannot make yet (resource nodes,
// ground items, buildings — their stores land later) enters through
// ResolveSmartClass with an explicit target class.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// SmartCommand is one resolved order: the concrete opcode plus the
// selection subset (one capability class — or several classes that
// resolved to the same opcode) it applies to.
type SmartCommand struct {
	Opcode uint8
	Units  []EntityID
	Target EntityID
	Point  fixed.Vec2
}

// BindSmartTable installs the loaded resolution table and the
// TypeID → capability-class mapping (column index into the table).
// Fails closed on out-of-range class indices.
func (w *World) BindSmartTable(t *data.SmartTable, classByTypeID []uint8) bool {
	if t == nil {
		return false
	}
	for _, c := range classByTypeID {
		if int(c) >= len(t.UnitClasses) {
			return false
		}
	}
	w.smart = t
	w.unitClassByType = classByTypeID
	return true
}

// unitClassOf returns a unit's capability-class column. Units without
// a UnitType row (or an unmapped TypeID) are class 0 — the table's
// first column is the baseline class by convention.
func (w *World) unitClassOf(id EntityID) uint8 {
	r := w.UnitTypes.Row(id)
	if r == -1 {
		return 0
	}
	tid := w.UnitTypes.TypeID[r]
	if int(tid) >= len(w.unitClassByType) {
		return 0
	}
	return w.unitClassByType[tid]
}

// ClassifyTarget classifies what the sim can see today: no target →
// ground point; dead/stale target → invalid (ok=false, the no-op);
// owned entity → ally/enemy by team. Resource nodes, items, and
// building contexts classify through ResolveSmartClass until their
// stores exist.
func (w *World) ClassifyTarget(team uint8, target EntityID) (uint8, bool) {
	if target == 0 {
		return data.TCGroundPoint, true
	}
	if !w.Ents.Alive(target) {
		return 0, false
	}
	or := w.Owners.Row(target)
	if or == -1 {
		return data.TCGroundPoint, true // unowned scenery: treat as ground
	}
	if w.Owners.Team[or] == team {
		return data.TCAlly, true
	}
	return data.TCEnemy, true
}

// ResolveSmart classifies the target and resolves the selection.
// ok=false means the click resolves to nothing (no table bound, dead
// target, empty selection) — the caller stages no records.
func (w *World) ResolveSmart(team uint8, sel []EntityID, target EntityID, point fixed.Vec2) ([]SmartCommand, bool) {
	tc, ok := w.ClassifyTarget(team, target)
	if !ok {
		return nil, false
	}
	return w.ResolveSmartClass(tc, sel, target, point)
}

// ResolveSmartClass resolves a selection against an explicit target
// class: one SmartCommand per distinct resolved opcode (split orders,
// input.md §4 — harvesters harvest, the rest move), units in
// selection order, command order by first appearance. Dead units in
// the selection are skipped (they cannot carry orders).
func (w *World) ResolveSmartClass(tc uint8, sel []EntityID, target EntityID, point fixed.Vec2) ([]SmartCommand, bool) {
	if w.smart == nil || tc >= data.TargetClassCount || len(sel) == 0 {
		return nil, false
	}
	row := w.smart.Rules[tc]
	var out []SmartCommand
	for _, id := range sel {
		if !w.Ents.Alive(id) {
			continue
		}
		op := row[w.unitClassOf(id)]
		found := -1
		for i := range out {
			if out[i].Opcode == op {
				found = i
				break
			}
		}
		if found == -1 {
			out = append(out, SmartCommand{Opcode: op, Target: target, Point: point})
			found = len(out) - 1
		}
		out[found].Units = append(out[found].Units, id)
	}
	if len(out) == 0 {
		return nil, false
	}
	return out, true
}
