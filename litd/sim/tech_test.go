package sim

// #303 tech tests. SoT = per-player upgrade arrays, admission
// decision traces (returned reasons + EvTrainRefused), derived-stat
// raw cache values, and twin state hashes.

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const tBarracks uint16 = 3
const upgBlades uint16 = 0

// techDefs extends prodDefs with a researching barracks and gives
// the footman a deterministic 10-damage weapon (dice 0).
func techDefs() []data.Unit {
	defs := prodDefs()
	defs[tFootman].Attacks = []data.Attack{{Range: fixed.FromInt(100), DamageBase: 10, CooldownTicks: 27}}
	defs = append(defs, data.Unit{ID: "zbarracks", Life: 1200, CollisionSize: 48,
		Trains: []uint16{tFootman}, Researches: []uint16{upgBlades}})
	defs[tHall].Researches = nil
	return defs
}

// techUpgrades: iron-blades, 2 levels (100g/10t, 150g/20t),
// +2 attack-damage and +1 armor per level, footman only.
func techUpgrades() []data.Upgrade {
	return []data.Upgrade{{
		ID: "iron-blades",
		Levels: []data.UpgradeLevel{
			{Costs: []int64{100, 0}, ResearchTicks: 10},
			{Costs: []int64{150, 0}, ResearchTicks: 20},
		},
		Mods: []data.UpgradeMod{
			{Stat: data.StatAttackDamage, Add: 2 << 32, Permille: 1000},
			{Stat: data.StatArmor, Add: 1, Permille: 1000},
		},
		AppliesTo: []uint16{tFootman},
	}}
}

// techRequires: training a footman needs iron-blades ≥1 AND an alive
// barracks; researching iron-blades needs an alive hall.
func techRequires() []data.Require {
	return []data.Require{
		{Target: tFootman, Upgrades: []data.ReqTerm{{Upgrade: upgBlades, Level: 1}}, Alive: []uint16{tBarracks}},
		{IsUpgrade: true, Target: upgBlades, Alive: []uint16{tHall}},
	}
}

func techWorld(t *testing.T) (*World, EntityID, EntityID) {
	t.Helper()
	w := NewWorld(Caps{Units: 64})
	if !w.BindEconomy(2) || !w.BindUnitDefs(techDefs()) {
		t.Fatal("bind failed")
	}
	if !w.BindTech(techUpgrades(), techRequires()) {
		t.Fatal("tech bind failed")
	}
	hall, ok := w.SpawnFromTable(tHall, 0, 0, pt2(200, 200))
	if !ok {
		t.Fatal("hall spawn failed")
	}
	rax, ok := w.SpawnFromTable(tBarracks, 0, 0, pt2(400, 200))
	if !ok {
		t.Fatal("barracks spawn failed")
	}
	w.resources[0][0] = 1000
	return w, hall, rax
}

func techState(w *World, id EntityID) string {
	return fmt.Sprintf("t%-4d lvl=%d gold=%d dmgAdd=%d armorAdd=%d",
		w.Tick(), w.UpgradeLevel(0, upgBlades), w.Resources(0, 0),
		w.buffAdd[data.StatAttackDamage][id.Index()]>>32, w.buffAdd[data.StatArmor][id.Index()])
}

// Edge 1: train gated on upgrade level — refused at level 0, allowed
// after the research completes.
func TestTechGatedTrainRefusedThenAllowed(t *testing.T) {
	w, _, rax := techWorld(t)
	got := w.EnqueueTrain(rax, tFootman)
	t.Logf("train at lvl 0 -> reason %d (gold=%d)", got, w.Resources(0, 0))
	if got != TrainTechLocked || w.Resources(0, 0) != 1000 {
		t.Fatalf("ungated: reason=%d gold=%d", got, w.Resources(0, 0))
	}
	if got := w.ResearchUpgrade(rax, upgBlades); got != TrainOK {
		t.Fatalf("research enqueue: %d", got)
	}
	for i := 0; i < 11; i++ {
		w.Step()
	}
	t.Logf("after research: lvl=%d gold=%d", w.UpgradeLevel(0, upgBlades), w.Resources(0, 0))
	if w.UpgradeLevel(0, upgBlades) != 1 {
		t.Fatalf("research did not complete: lvl=%d", w.UpgradeLevel(0, upgBlades))
	}
	got = w.EnqueueTrain(rax, tFootman)
	t.Logf("train at lvl 1 -> reason %d (gold=%d)", got, w.Resources(0, 0))
	if got != TrainOK {
		t.Fatalf("gated train still refused: %d", got)
	}
}

// Edge 2: completion re-derives EXISTING units' stats per the table
// and NEW units inherit; non-applying types stay identity.
func TestTechStatApplication(t *testing.T) {
	w, _, rax := techWorld(t)
	w.SetTechGate(nil) // isolate stat math from the train gate
	foot, ok := w.SpawnFromTable(tFootman, 0, 0, pt2(300, 300))
	if !ok {
		t.Fatal("footman spawn failed")
	}
	worker, _ := w.SpawnFromTable(tWorker, 0, 0, pt2(320, 300))
	cr := w.Combats.Row(foot)
	roll0 := w.rollWeapon(cr, 0)
	t.Logf("before: foot %s | roll=%d", techState(w, foot), roll0.Floor())
	if roll0 != fixed.FromInt(10) {
		t.Fatalf("base roll: %d", roll0.Floor())
	}
	w.ResearchUpgrade(rax, upgBlades)
	for i := 0; i < 11; i++ {
		w.Step()
	}
	roll1 := w.rollWeapon(cr, 0)
	t.Logf("after lvl1: foot %s | roll=%d | worker dmgAdd=%d", techState(w, foot), roll1.Floor(),
		w.buffAdd[data.StatAttackDamage][worker.Index()]>>32)
	if w.buffAdd[data.StatAttackDamage][foot.Index()] != 2<<32 || w.buffAdd[data.StatArmor][foot.Index()] != 1 {
		t.Fatalf("existing unit cache wrong: %s", techState(w, foot))
	}
	if roll1 != fixed.FromInt(12) {
		t.Fatalf("damage roll after lvl1 = %d, want 12", roll1.Floor())
	}
	if w.buffAdd[data.StatAttackDamage][worker.Index()] != 0 {
		t.Fatal("upgrade leaked onto a non-applying type")
	}
	// level 2 stacks; a NEW footman inherits both levels at spawn
	w.ResearchUpgrade(rax, upgBlades)
	for i := 0; i < 21; i++ {
		w.Step()
	}
	fresh, _ := w.SpawnFromTable(tFootman, 0, 0, pt2(340, 300))
	t.Logf("after lvl2: existing %s | fresh dmgAdd=%d armorAdd=%d", techState(w, foot),
		w.buffAdd[data.StatAttackDamage][fresh.Index()]>>32, w.buffAdd[data.StatArmor][fresh.Index()])
	if w.buffAdd[data.StatAttackDamage][foot.Index()] != 4<<32 ||
		w.buffAdd[data.StatAttackDamage][fresh.Index()] != 4<<32 ||
		w.buffAdd[data.StatArmor][fresh.Index()] != 2 {
		t.Fatal("level-2 fold wrong on existing or fresh unit")
	}
	// enemy player's footman (player 1) stays identity
	w.resources[1] = w.resources[1][:2]
	enemy, _ := w.SpawnFromTable(tFootman, 1, 1, pt2(360, 300))
	if w.buffAdd[data.StatAttackDamage][enemy.Index()] != 0 {
		t.Fatal("upgrade leaked across players")
	}
}

// Edge 3: requirement building destroyed AFTER enqueue — the queued
// research completes (admission-time check), a new enqueue refuses.
func TestTechRequirementCheckedAtAdmissionOnly(t *testing.T) {
	w, hall, rax := techWorld(t)
	if got := w.ResearchUpgrade(rax, upgBlades); got != TrainOK {
		t.Fatalf("enqueue: %d", got)
	}
	w.KillUnit(hall) // iron-blades requires an alive hall
	w.Step()
	t.Logf("hall dead, research in flight: lvl=%d alive(hall)=%v", w.UpgradeLevel(0, upgBlades), w.Ents.Alive(hall))
	for i := 0; i < 11; i++ {
		w.Step()
	}
	t.Logf("queued entry completed: lvl=%d", w.UpgradeLevel(0, upgBlades))
	if w.UpgradeLevel(0, upgBlades) != 1 {
		t.Fatal("queued research did not survive the requirement building's death")
	}
	got := w.ResearchUpgrade(rax, upgBlades)
	t.Logf("new enqueue without hall -> reason %d", got)
	if got != TrainTechLocked {
		t.Fatalf("new enqueue: reason %d, want TrainTechLocked(%d)", got, TrainTechLocked)
	}
}

// Edge 4: max-level cap — the table max and a lower SetTechAllowed
// cap both refuse deterministically.
func TestTechMaxLevelCap(t *testing.T) {
	w, _, rax := techWorld(t)
	research := func() uint8 {
		got := w.ResearchUpgrade(rax, upgBlades)
		for i := 0; i < 21 && got == TrainOK; i++ {
			w.Step()
		}
		return got
	}
	if research() != TrainOK || research() != TrainOK {
		t.Fatal("levels 1..2 refused")
	}
	got := research()
	t.Logf("research at table max (lvl=%d) -> reason %d", w.UpgradeLevel(0, upgBlades), got)
	if got != TrainMaxLevel || w.UpgradeLevel(0, upgBlades) != 2 {
		t.Fatalf("table cap: reason=%d lvl=%d", got, w.UpgradeLevel(0, upgBlades))
	}
	// player 1 capped at 0 via SetTechAllowed
	rax1, _ := w.SpawnFromTable(tBarracks, 1, 1, pt2(600, 200))
	hall1, _ := w.SpawnFromTable(tHall, 1, 1, pt2(700, 200))
	_ = hall1
	w.resources[1] = w.resources[1][:2]
	w.resources[1][0] = 1000
	if !w.SetTechAllowed(1, upgBlades, 0) {
		t.Fatal("SetTechAllowed failed")
	}
	got = w.ResearchUpgrade(rax1, upgBlades)
	t.Logf("player 1 capped at 0 -> reason %d", got)
	if got != TrainMaxLevel {
		t.Fatalf("SetTechAllowed cap: reason %d", got)
	}
}

// Research admission: duplicate in-flight refused; cancel refunds the
// level cost in full; wrong building refused.
func TestTechResearchQueueSemantics(t *testing.T) {
	w, hall, rax := techWorld(t)
	if got := w.ResearchUpgrade(rax, upgBlades); got != TrainOK {
		t.Fatalf("enqueue: %d", got)
	}
	t.Logf("after enqueue: gold=%d (level-1 cost 100)", w.Resources(0, 0))
	if w.Resources(0, 0) != 900 {
		t.Fatalf("cost not deducted: %d", w.Resources(0, 0))
	}
	got := w.ResearchUpgrade(rax, upgBlades)
	t.Logf("duplicate while in flight -> reason %d", got)
	if got != TrainResearchBusy {
		t.Fatalf("duplicate: reason %d, want TrainResearchBusy(%d)", got, TrainResearchBusy)
	}
	if got := w.ResearchUpgrade(hall, upgBlades); got != TrainNotTrainable {
		t.Fatalf("hall (no researches list): reason %d, want TrainNotTrainable", got)
	}
	if !w.CancelTrain(rax, 0) {
		t.Fatal("cancel refused")
	}
	t.Logf("after cancel: gold=%d food=%d", w.Resources(0, 0), w.FoodUsed(0))
	if w.Resources(0, 0) != 1000 || w.FoodUsed(0) != 0 {
		t.Fatalf("research cancel refund wrong: gold=%d food=%d", w.Resources(0, 0), w.FoodUsed(0))
	}
}

// Edge 5 + save: twin runs hash-identical; the arrays restore through
// save/load and the stat caches re-derive without any live buff.
func TestTechDeterminismAndSave(t *testing.T) {
	build := func() *World {
		w := NewWorld(Caps{Units: 64})
		w.BindEconomy(2)
		w.BindUnitDefs(techDefs())
		w.BindTech(techUpgrades(), techRequires())
		w.SpawnFromTable(tHall, 0, 0, pt2(200, 200))
		rax, _ := w.SpawnFromTable(tBarracks, 0, 0, pt2(400, 200))
		w.resources[0][0] = 1000
		w.ResearchUpgrade(rax, upgBlades)
		for i := 0; i < 15; i++ {
			w.Step()
		}
		w.SpawnFromTable(tFootman, 0, 0, pt2(300, 300))
		w.ResearchUpgrade(rax, upgBlades)
		for i := 0; i < 25; i++ {
			w.Step()
		}
		return w
	}
	a, b := build(), build()
	reg := NewHashRegistry()
	var sa, sb statehash.Snapshot
	a.HashState(reg, &sa)
	b.HashState(NewHashRegistry(), &sb)
	t.Logf("twin A=%016x B=%016x lvl=%d/%d", sa.Top, sb.Top, a.UpgradeLevel(0, upgBlades), b.UpgradeLevel(0, upgBlades))
	if sa.Top != sb.Top {
		t.Fatal("twin divergence")
	}
	// hash sensitivity: a level flip moves the hash
	a.upgradeLevel[2][upgBlades] = 1
	var sa2 statehash.Snapshot
	a.HashState(reg, &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("upgrade level invisible to the state hash")
	}
	a.upgradeLevel[2][upgBlades] = 0

	var buf bytes.Buffer
	if err := a.SaveState(&buf, 7); err != nil {
		t.Fatal(err)
	}
	w2 := NewWorld(Caps{Units: 64})
	w2.BindEconomy(2)
	w2.BindUnitDefs(techDefs())
	w2.BindTech(techUpgrades(), techRequires())
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 7); err != nil {
		t.Fatal(err)
	}
	var sl statehash.Snapshot
	w2.HashState(NewHashRegistry(), &sl)
	t.Logf("saved=%016x loaded=%016x lvl=%d", sa.Top, sl.Top, w2.UpgradeLevel(0, upgBlades))
	if sl.Top != sa.Top || w2.UpgradeLevel(0, upgBlades) != 2 {
		t.Fatal("tech arrays lost in save round-trip")
	}
	// the footman has NO buffs — its upgrade fold must still re-derive
	var foot EntityID
	ut := w2.UnitTypes
	for i := int32(0); i < ut.Count(); i++ {
		if ut.TypeID[i] == tFootman {
			foot = ut.Entity[i]
		}
	}
	t.Logf("loaded footman dmgAdd=%d armorAdd=%d", w2.buffAdd[data.StatAttackDamage][foot.Index()]>>32, w2.buffAdd[data.StatArmor][foot.Index()])
	if w2.buffAdd[data.StatAttackDamage][foot.Index()] != 4<<32 {
		t.Fatal("upgrade stat cache not re-derived at load")
	}
	// load without BindTech refuses by name
	w3 := NewWorld(Caps{Units: 64})
	w3.BindEconomy(2)
	w3.BindUnitDefs(techDefs())
	if err := w3.LoadState(bytes.NewReader(buf.Bytes()), 7); err == nil {
		t.Fatal("load without bound tech accepted")
	} else {
		t.Logf("unbound tech refused: %v", err)
	}
}

// R-GC-1: research-active ticks allocate nothing.
func TestTechTickAllocs(t *testing.T) {
	w, _, rax := techWorld(t)
	w.resources[0][0] = 1 << 40
	w.ResearchUpgrade(rax, upgBlades)
	w.Step()
	allocs := testing.AllocsPerRun(60, func() {
		w.Step()
		if !w.researchPending(0, upgBlades) && w.UpgradeLevel(0, upgBlades) < 2 {
			w.ResearchUpgrade(rax, upgBlades)
		}
	})
	t.Logf("allocs/op with active research: %v (lvl=%d)", allocs, w.UpgradeLevel(0, upgBlades))
	if allocs != 0 {
		t.Fatalf("research tick allocates: %v", allocs)
	}
}
