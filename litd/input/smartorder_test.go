package input

import (
	"os"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func inputSmartWorld(t *testing.T) (*sim.World, sim.EntityID, sim.EntityID, sim.EntityID) {
	t.Helper()
	tables, err := data.Load(os.DirFS("../../data"))
	if err != nil {
		t.Fatalf("starter data must load: %v", err)
	}
	w := sim.NewWorld(sim.Caps{})
	if !w.BindSmartTable(tables.Smart, []uint8{0, 1}) {
		t.Fatal("BindSmartTable failed")
	}
	fighter := inputSmartUnit(t, w, 0, 0, 0)
	worker := inputSmartUnit(t, w, 0, 0, 1)
	target, ok := w.CreateUnit(fixed.Vec2{X: 700 * fixed.One, Y: 700 * fixed.One}, 0)
	if !ok {
		t.Fatal("target setup failed")
	}
	return w, fighter, worker, target
}

func inputSmartUnit(t *testing.T, w *sim.World, player, team uint8, typeID uint16) sim.EntityID {
	t.Helper()
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.One, Y: fixed.One}, 0)
	if !ok || !w.Owners.Add(w.Ents, id, player, team, player) || !w.UnitTypes.Add(w.Ents, id, typeID) {
		t.Fatal("smart-order unit setup failed")
	}
	return id
}

func inputSelection(ids ...sim.EntityID) Selection {
	var s Selection
	s.Count = uint8(len(ids))
	for i := range ids {
		s.IDs[i] = ids[i]
	}
	return s
}

func TestSmartOrderTable(t *testing.T) {
	w, fighter, worker, target := inputSmartWorld(t)
	pt := fixed.Vec2{X: 900 * fixed.One, Y: 600 * fixed.One}
	cases := []struct {
		name      string
		class     uint8
		target    sim.EntityID
		wantOps   []uint8
		wantUnits [][]sim.EntityID
	}{
		{"ground", data.TCGroundPoint, 0, []uint8{sim.OpMove}, [][]sim.EntityID{{fighter, worker}}},
		{"enemy", data.TCEnemy, target, []uint8{sim.OpAttack}, [][]sim.EntityID{{fighter, worker}}},
		{"ally", data.TCAlly, target, []uint8{sim.OpFollow}, [][]sim.EntityID{{fighter, worker}}},
		{"transport", data.TCTransport, target, []uint8{sim.OpBoard}, [][]sim.EntityID{{fighter, worker}}},
		{"own-building", data.TCOwnBuilding, target, []uint8{sim.OpRally}, [][]sim.EntityID{{fighter, worker}}},
		{"damaged-own", data.TCDamagedOwn, target, []uint8{sim.OpMove, sim.OpRepair}, [][]sim.EntityID{{fighter}, {worker}}},
		{"construction", data.TCConstruction, target, []uint8{sim.OpMove, sim.OpResumeConstruction}, [][]sim.EntityID{{fighter}, {worker}}},
		{"resource", data.TCResource, target, []uint8{sim.OpMove, sim.OpHarvest}, [][]sim.EntityID{{fighter}, {worker}}},
		{"item", data.TCItem, target, []uint8{sim.OpGetItem}, [][]sim.EntityID{{fighter, worker}}},
	}
	for _, tc := range cases {
		var out SmartOrderResult
		buf := make([]byte, 0, 256)
		req := SmartOrderRequest{
			Player:    0,
			Team:      0,
			Seq:       11,
			Selection: inputSelection(fighter, worker),
			Target:    SmartTarget{Entity: tc.target, Point: pt, Class: tc.class, ClassSet: true},
		}
		encoded, ok := ResolveRightClick(w, req, buf, &out)
		t.Logf("FSV row %-13s BEFORE selected=[%d,%d] target=%d AFTER records=%s bytes=%d feedback=%s",
			tc.name, fighter, worker, tc.target, inputCommandDump(out), len(encoded), out.Feedback.String())
		if !ok || out.Count != uint8(len(tc.wantOps)) || out.Feedback != SmartFeedbackNone {
			t.Fatalf("%s: resolution failed count=%d feedback=%s ok=%v", tc.name, out.Count, out.Feedback.String(), ok)
		}
		for i, wantOp := range tc.wantOps {
			rec := out.Records[i]
			if rec.Opcode != wantOp || !sameRecordUnits(rec, tc.wantUnits[i]) {
				t.Fatalf("%s record %d = %s, want op=%d units=%v", tc.name, i, inputRecordDump(rec), wantOp, tc.wantUnits[i])
			}
			if rec.Flags != 0 || rec.Player != 0 || rec.Seq != 11+uint16(i) {
				t.Fatalf("%s record metadata wrong: %s", tc.name, inputRecordDump(rec))
			}
		}
	}
}

func TestSmartRightClickHarvestSplitAndDecodeFSV(t *testing.T) {
	w, fighter, worker, mine := inputSmartWorld(t)
	pt := fixed.Vec2{X: 800 * fixed.One, Y: 800 * fixed.One}
	var out SmartOrderResult
	encoded, ok := ResolveRightClick(w, SmartOrderRequest{
		Player:    0,
		Team:      0,
		Seq:       3,
		Selection: inputSelection(fighter, worker),
		Target:    SmartTarget{Entity: mine, Point: pt, Class: data.TCResource, ClassSet: true},
		Queued:    true,
	}, make([]byte, 0, 128), &out)
	if !ok || out.Count != 2 {
		t.Fatalf("resource split failed: %s feedback=%s", inputCommandDump(out), out.Feedback.String())
	}
	var a, b sim.CommandRecord
	n, ok := sim.DecodeCommand(encoded, &a)
	if !ok {
		t.Fatalf("first encoded record did not decode: %x", encoded)
	}
	_, ok = sim.DecodeCommand(encoded[n:], &b)
	if !ok {
		t.Fatalf("second encoded record did not decode: %x", encoded[n:])
	}
	t.Logf("FSV harvest split BEFORE selected=[fighter %d, worker %d] target=%d AFTER decoded=(%s) (%s)",
		fighter, worker, mine, inputRecordDump(a), inputRecordDump(b))
	if a.Opcode != sim.OpMove || b.Opcode != sim.OpHarvest || a.Flags&sim.CmdFlagQueued == 0 || b.Flags&sim.CmdFlagQueued == 0 {
		t.Fatalf("harvest split op/queued flags wrong: %s | %s", inputRecordDump(a), inputRecordDump(b))
	}
	if !sameRecordUnits(a, []sim.EntityID{fighter}) || !sameRecordUnits(b, []sim.EntityID{worker}) || b.Target != mine {
		t.Fatalf("harvest split records wrong: %s | %s", inputRecordDump(a), inputRecordDump(b))
	}
}

func TestSmartRightClickClientFilteredEdgesFSV(t *testing.T) {
	w, fighter, _, target := inputSmartWorld(t)
	selection := inputSelection(fighter)
	base := []byte{0xaa, 0xbb}

	var hidden SmartOrderResult
	hiddenBytes, hiddenOK := ResolveRightClick(w, SmartOrderRequest{
		Player: 0, Team: 0, Selection: selection,
		Target: SmartTarget{Entity: target, Hidden: true, Class: data.TCEnemy, ClassSet: true},
	}, append([]byte(nil), base...), &hidden)
	t.Logf("FSV hidden target BEFORE bytes=%x targetAlive=%v AFTER bytes=%x records=%d feedback=%s",
		base, w.Ents.Alive(target), hiddenBytes, hidden.Count, hidden.Feedback.String())
	if hiddenOK || hidden.Count != 0 || hidden.Feedback != SmartFeedbackHiddenTarget || len(hiddenBytes) != len(base) {
		t.Fatalf("hidden target must be client-filtered with no records: ok=%v %+v bytes=%x", hiddenOK, hidden, hiddenBytes)
	}

	w.DestroyUnit(target)
	var dead SmartOrderResult
	deadBytes, deadOK := ResolveRightClick(w, SmartOrderRequest{
		Player: 0, Team: 0, Selection: selection,
		Target: SmartTarget{Entity: target, Class: data.TCEnemy, ClassSet: true},
	}, append([]byte(nil), base...), &dead)
	t.Logf("FSV dead target BEFORE bytes=%x targetAlive=false AFTER bytes=%x records=%d feedback=%s",
		base, deadBytes, dead.Count, dead.Feedback.String())
	if deadOK || dead.Count != 0 || dead.Feedback != SmartFeedbackDeadTarget || len(deadBytes) != len(base) {
		t.Fatalf("dead target must be client-filtered with no records: ok=%v %+v bytes=%x", deadOK, dead, deadBytes)
	}
}

func TestExplicitAttackMoveBypassesSmartFSV(t *testing.T) {
	w, fighter, _, _ := inputSmartWorld(t)
	pt := fixed.Vec2{X: 512 * fixed.One, Y: 384 * fixed.One}
	sel := inputSelection(fighter)
	var smart, explicit SmartOrderResult
	_, smartOK := ResolveRightClick(w, SmartOrderRequest{
		Player: 0, Team: 0, Seq: 1, Selection: sel,
		Target: SmartTarget{Point: pt},
	}, make([]byte, 0, 128), &smart)
	_, explicitOK := ResolveExplicitOrder(w, ExplicitOrderRequest{
		Player: 0, Seq: 1, Opcode: sim.OpAttack, Selection: sel,
		Target: SmartTarget{Point: pt},
	}, make([]byte, 0, 128), &explicit)
	t.Logf("FSV explicit bypass same click point=(%d,%d): right-click=%s explicit-A=%s",
		pt.X, pt.Y, inputCommandDump(smart), inputCommandDump(explicit))
	if !smartOK || !explicitOK || smart.Records[0].Opcode != sim.OpMove || explicit.Records[0].Opcode != sim.OpAttack {
		t.Fatalf("explicit order did not bypass smart resolver: smart=%s explicit=%s", inputCommandDump(smart), inputCommandDump(explicit))
	}
}

func TestSmartRightClickResolveZeroAllocFSV(t *testing.T) {
	w, fighter, worker, mine := inputSmartWorld(t)
	req := SmartOrderRequest{
		Player:    0,
		Team:      0,
		Seq:       8,
		Selection: inputSelection(fighter, worker),
		Target:    SmartTarget{Entity: mine, Point: fixed.Vec2{X: fixed.One, Y: fixed.One}, Class: data.TCResource, ClassSet: true},
	}
	var out SmartOrderResult
	buf := make([]byte, 0, 128)
	allocs := testing.AllocsPerRun(1000, func() {
		buf = buf[:0]
		_, _ = ResolveRightClick(w, req, buf, &out)
	})
	t.Logf("FSV smart right-click resolve+encode allocs/op=%v records=%s", allocs, inputCommandDump(out))
	if allocs != 0 {
		t.Fatalf("smart right-click resolve+encode allocated: %v", allocs)
	}
}

func sameRecordUnits(rec sim.CommandRecord, want []sim.EntityID) bool {
	if int(rec.UnitCount) != len(want) {
		return false
	}
	for i := range want {
		if rec.Units[i] != want[i] {
			return false
		}
	}
	return true
}

func inputCommandDump(out SmartOrderResult) string {
	s := "["
	for i := uint8(0); i < out.Count; i++ {
		if i > 0 {
			s += " "
		}
		s += inputRecordDump(out.Records[i])
	}
	return s + "]"
}

func inputRecordDump(rec sim.CommandRecord) string {
	s := "{op="
	s += opName(rec.Opcode)
	s += " units=["
	for i := uint8(0); i < rec.UnitCount; i++ {
		if i > 0 {
			s += ","
		}
		s += uintString(uint32(rec.Units[i]))
	}
	s += "] target="
	s += uintString(uint32(rec.Target))
	s += " seq="
	s += uintString(uint32(rec.Seq))
	s += " flags="
	s += uintString(uint32(rec.Flags))
	s += "}"
	return s
}

func opName(op uint8) string {
	switch op {
	case sim.OpMove:
		return "move"
	case sim.OpAttack:
		return "attack"
	case sim.OpRally:
		return "rally"
	case sim.OpHarvest:
		return "harvest"
	case sim.OpRepair:
		return "repair"
	case sim.OpBoard:
		return "board"
	case sim.OpGetItem:
		return "get-item"
	case sim.OpFollow:
		return "follow"
	case sim.OpResumeConstruction:
		return "resume-construction"
	default:
		return uintString(uint32(op))
	}
}

func uintString(v uint32) string {
	if v == 0 {
		return "0"
	}
	var buf [10]byte
	i := len(buf)
	for v > 0 {
		i--
		buf[i] = byte('0' + v%10)
		v /= 10
	}
	return string(buf[i:])
}
