package sim

// #304 hero tests. SoT = hero component dumps (raw fixed-point
// attrs, XP, level, skill points), XP ledger math, dead-pool/revive
// queue dumps, the D-15 record round-trip diff, and twin hashes.

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	tPaladin uint16 = 4
	tAltar   uint16 = 5
	hPaladin uint16 = 0 // hero-table index
)

func heroUnitDefs() []data.Unit {
	defs := techDefs()
	defs = append(defs, data.Unit{ID: "paladin", Life: 100, MoveSpeedPerTick: 2 * fixed.One,
		TurnRatePerTick: 65535, CollisionSize: 16, FoodCost: 5,
		Attacks: []data.Attack{{Range: fixed.FromInt(100), DamageBase: 10, CooldownTicks: 27}}})
	defs = append(defs, data.Unit{ID: "zaltar", Life: 900, CollisionSize: 48,
		FoodProvided: 20, RevivesHeroes: true})
	return defs
}

func heroRules(units int) *data.HeroTables {
	bounty := make([]int64, units)
	bounty[tWorker] = 25
	return &data.HeroTables{
		Curve:         []int64{0, 200, 500, 900},
		ShareRadius:   fixed.FromInt(600),
		Split:         data.SplitEqual,
		DeathPenalty:  100,
		StartSkillPts: 1,
		Bounty:        bounty,
		Heroes: []data.HeroDef{{
			Unit: tPaladin,
			Str:  22 << 32, Agi: 13 << 32, Int: 17 << 32,
			StrG: 5 << 31, AgiG: 3 << 31, IntG: 7 << 30, // 2.5 / 1.5 / 1.75
			Skills: []data.HeroSkill{{Ability: 0, MinHeroLevel: []uint8{1, 3}}},
		}},
		Attr: data.AttrCoeffs{
			StrHP:               25 << 32,
			StrRegen:            (1 << 32) / 400, // 0.0025/tick
			AgiArmor:            (3 << 32) / 10,  // 0.3
			AgiCooldownPermille: 990,
			IntMana:             15 << 32,
			IntManaRegen:        (1 << 32) / 500,
		},
		Revive: data.ReviveSpec{BaseTicks: 10, TicksPerLevel: 5,
			CostsBase: []int64{100, 0}, CostsPerLevel: []int64{50, 0}},
	}
}

func heroWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{Units: 64})
	defs := heroUnitDefs()
	if !w.BindEconomy(2) || !w.BindUnitDefs(defs) ||
		!w.BindAbilityDefs([]data.Ability{{ID: "holy-light"}}) ||
		!w.BindHeroes(heroRules(len(defs))) {
		t.Fatal("bind failed")
	}
	if err := w.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatal(err)
	}
	w.resources[0][0] = 1000
	return w
}

func hdump(w *World, id EntityID) string {
	r := w.Heroes.Row(id)
	if r == -1 {
		return "no-hero-row"
	}
	h := w.Heroes
	hr := w.Healths.Row(id)
	ar := w.Abilities.Row(id)
	return fmt.Sprintf("t%-3d lvl=%d xp=%d str=%d agi=%d int=%d (raw) pts=%d skills=%v maxLife=%d mana=%d armorAdd=%d",
		w.Tick(), h.Level[r], h.XP[r], int64(h.Str[r]), int64(h.Agi[r]), int64(h.Int[r]),
		h.SkillPoints[r], h.SkillLevel[r],
		w.Healths.MaxLife[hr].Floor(), w.Abilities.MaxMana[ar].Floor(),
		w.buffAdd[data.StatArmor][id.Index()])
}

// Base stats: unit row is the zero-attribute baseline; spawn applies
// the full base attributes.
func TestHeroSpawnBaseStats(t *testing.T) {
	w := heroWorld(t)
	id, ok := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	if !ok {
		t.Fatal("spawn failed")
	}
	t.Logf("spawned: %s", hdump(w, id))
	hr := w.Healths.Row(id)
	ar := w.Abilities.Row(id)
	// 100 base + 25×22 str HP = 650; mana 15×17 = 255; armor floor(13×0.3)=3
	if w.Healths.MaxLife[hr] != fixed.FromInt(650) || w.Healths.Life[hr] != fixed.FromInt(650) {
		t.Fatalf("max life raw %d, want %d", int64(w.Healths.MaxLife[hr]), int64(fixed.FromInt(650)))
	}
	if w.Abilities.MaxMana[ar] != fixed.FromInt(255) {
		t.Fatalf("max mana raw %d", int64(w.Abilities.MaxMana[ar]))
	}
	if w.buffAdd[data.StatArmor][id.Index()] != 3 {
		t.Fatalf("agi armor add = %d, want 3", w.buffAdd[data.StatArmor][id.Index()])
	}
	if w.buffMult[data.StatAttackCooldown][id.Index()] == fixed.One {
		t.Fatal("agi cooldown fold missing")
	}
	hrw := w.Heroes.Row(id)
	if w.Heroes.Level[hrw] != 1 || w.Heroes.SkillPoints[hrw] != 1 || w.Heroes.XP[hrw] != 0 {
		t.Fatalf("fresh hero row: %s", hdump(w, id))
	}
}

// Edge 1: a lethal packet pays the table bounty once; two heroes in
// range split 25/2 = 12 each (remainder dropped); an out-of-range
// hero gets nothing.
func TestHeroKillBountySplit(t *testing.T) {
	w := heroWorld(t)
	h1, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	h2, _ := w.SpawnHero(hPaladin, 0, 0, pt2(150, 100))
	far, _ := w.SpawnHero(hPaladin, 0, 0, pt2(5000, 5000))
	victim, _ := w.SpawnFromTable(tWorker, 1, 1, pt2(120, 100))
	xp := func(id EntityID) int64 { return w.Heroes.XP[w.Heroes.Row(id)] }
	t.Logf("before kill: h1=%d h2=%d far=%d", xp(h1), xp(h2), xp(far))
	stepWithPackets(w, DamagePacket{Source: h1, Target: victim, Amount: fixed.FromInt(10000)})
	t.Logf("after kill (bounty 25, 2 in range): h1=%d h2=%d far=%d victimAlive=%v",
		xp(h1), xp(h2), xp(far), w.Ents.Alive(victim))
	if xp(h1) != 12 || xp(h2) != 12 || xp(far) != 0 {
		t.Fatalf("split wrong: h1=%d h2=%d far=%d (want 12/12/0)", xp(h1), xp(h2), xp(far))
	}
}

// Edge 2: the exact curve boundary — 199 XP stays level 1, the 200th
// point levels at the boundary with table-exact growth.
func TestHeroLevelBoundary(t *testing.T) {
	w := heroWorld(t)
	id, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	var levels []string
	w.RegisterHandler(hA, func(w *World, e Event) {
		levels = append(levels, fmt.Sprintf("t%d level %d", w.Tick(), e.Arg))
	})
	w.Subscribe(EvHeroLevel, hA)
	w.AddXP(id, 199)
	before := hdump(w, id)
	t.Logf("T-1 (xp=199): %s", before)
	r := w.Heroes.Row(id)
	if w.Heroes.Level[r] != 1 {
		t.Fatalf("leveled before the boundary: %s", before)
	}
	w.AddXP(id, 1)
	w.Step() // flush the event
	t.Logf("T   (xp=200): %s", hdump(w, id))
	t.Logf("level events: %v", levels)
	if w.Heroes.Level[r] != 2 || w.Heroes.SkillPoints[r] != 2 {
		t.Fatalf("boundary level-up wrong: %s", hdump(w, id))
	}
	// growth: str 22→24.5 raw, life 650→712.5 raw (+25×2.5)
	if w.Heroes.Str[r] != (22<<32)+(5<<31) {
		t.Fatalf("str growth raw %d", int64(w.Heroes.Str[r]))
	}
	hr := w.Healths.Row(id)
	want := fixed.FromInt(650).Add(fixed.F64(125 << 31)) // 62.5
	if w.Healths.MaxLife[hr] != want {
		t.Fatalf("life growth raw %d, want %d", int64(w.Healths.MaxLife[hr]), int64(want))
	}
	if len(levels) != 1 {
		t.Fatalf("EvHeroLevel count: %v", levels)
	}
	// multi-level in one grant: +700 reaches the 900 cap → level 4
	w.AddXP(id, 700)
	if w.Heroes.Level[r] != 4 || w.Heroes.XP[r] != 900 {
		t.Fatalf("multi-level grant: %s", hdump(w, id))
	}
}

// Edge 3: LearnSkill refusals — 0 points, tier gate — then success.
func TestHeroLearnSkill(t *testing.T) {
	w := heroWorld(t)
	id, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	r := w.Heroes.Row(id)
	if got := w.LearnSkill(id, 0); got != SkillOK {
		t.Fatalf("learn 1: %d", got)
	}
	ar := w.Abilities.Row(id)
	t.Logf("learned lvl 1: slot ability=%d level=%d pts=%d", w.Abilities.AbilityID[ar][0], w.Abilities.Level[ar][0], w.Heroes.SkillPoints[r])
	if w.Abilities.AbilityID[ar][0] != 1 || w.Abilities.Level[ar][0] != 1 {
		t.Fatal("ability slot not set")
	}
	got := w.LearnSkill(id, 0) // 0 points left
	t.Logf("no points -> %d", got)
	if got != SkillNoPoints {
		t.Fatalf("want SkillNoPoints(%d), got %d", SkillNoPoints, got)
	}
	w.AddXP(id, 200) // level 2: +1 point, but skill lvl 2 needs hero lvl 3
	got = w.LearnSkill(id, 0)
	t.Logf("tier gate (hero lvl 2, needs 3) -> %d", got)
	if got != SkillTierLocked {
		t.Fatalf("want SkillTierLocked(%d), got %d", SkillTierLocked, got)
	}
	w.AddXP(id, 300) // level 3
	if got = w.LearnSkill(id, 0); got != SkillOK || w.Abilities.Level[ar][0] != 2 {
		t.Fatalf("learn 2 at lvl 3: got %d level %d", got, w.Abilities.Level[ar][0])
	}
	if got = w.LearnSkill(id, 0); got != SkillMaxLevel {
		t.Fatalf("over max: %d", got)
	}
	if got = w.LearnSkill(id, 3); got != SkillUnknown {
		t.Fatalf("unknown skill: %d", got)
	}
}

// Edge 4: death captures the penalized record; two queued revives on
// one altar hold FIFO semantics; costs scale by record level.
func TestHeroDeathAndReviveQueue(t *testing.T) {
	w := heroWorld(t)
	altar, ok := w.SpawnFromTable(tAltar, 0, 0, pt2(200, 200))
	if !ok || w.Produce.Row(altar) == -1 {
		t.Fatal("altar has no produce row")
	}
	h1, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	h2, _ := w.SpawnHero(hPaladin, 0, 0, pt2(150, 100))
	h3, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 150))
	w.AddXP(h1, 250) // level 2, xp 250
	w.AddXP(h3, 200) // level 2 at exactly the boundary — floor must bind
	w.KillUnit(h1)
	w.KillUnit(h2)
	w.KillUnit(h3)
	w.Step()
	rec0, rec1, rec2 := w.DeadHero(0, 0), w.DeadHero(0, 1), w.DeadHero(0, 2)
	t.Logf("pool[0]: used=%v lvl=%d xp=%d", rec0.Used, rec0.Level, rec0.XP)
	t.Logf("pool[1]: used=%v lvl=%d xp=%d", rec1.Used, rec1.Level, rec1.XP)
	t.Logf("pool[2]: used=%v lvl=%d xp=%d (penalty 200→180 floored at curve[1]=200)", rec2.Used, rec2.Level, rec2.XP)
	// penalty 10%: 250 → 225, floor curve[1]=200 keeps 225
	if !rec0.Used || rec0.Level != 2 || rec0.XP != 225 {
		t.Fatalf("record 0 wrong: %+v", rec0)
	}
	if !rec1.Used || rec1.Level != 1 || rec1.XP != 0 {
		t.Fatalf("record 1 wrong: %+v", rec1)
	}
	// 200 × 0.9 = 180 < curve[1] → floored: heroes never de-level
	if !rec2.Used || rec2.Level != 2 || rec2.XP != 200 {
		t.Fatalf("record 2 floor wrong: %+v", rec2)
	}
	gold := w.Resources(0, 0)
	if got := w.ReviveHero(altar, 0); got != TrainOK { // lvl 2: 100+50 = 150
		t.Fatalf("revive 0: %d", got)
	}
	if got := w.ReviveHero(altar, 0); got != TrainResearchBusy {
		t.Fatalf("duplicate revive: %d", got)
	}
	if got := w.ReviveHero(altar, 1); got != TrainOK { // lvl 1: 100
		t.Fatalf("revive 1: %d", got)
	}
	if got := w.ReviveHero(altar, 3); got != TrainUnknownType {
		t.Fatalf("empty slot: %d", got)
	}
	pr := w.Produce.Row(altar)
	t.Logf("queue: %v flags=%v gold %d->%d food=%d/%d", w.Produce.Queue[pr][:2], w.Produce.QFlags[pr][:2],
		gold, w.Resources(0, 0), w.FoodUsed(0), w.FoodCap(0))
	if w.Resources(0, 0) != gold-250 || w.FoodUsed(0) != 10 {
		t.Fatalf("revive admission ledger: gold=%d food=%d", w.Resources(0, 0), w.FoodUsed(0))
	}
	// lvl-2 revive takes 10+5=15 ticks; lvl-1 then 10 more
	var spawned []EntityID
	w.RegisterHandler(hB, func(w *World, e Event) { spawned = append(spawned, e.Dst) })
	w.Subscribe(EvUnitTrained, hB)
	for i := 0; i < 30; i++ {
		w.Step()
	}
	t.Logf("revived %d heroes; pool[0].Used=%v pool[1].Used=%v food=%d", len(spawned), w.DeadHero(0, 0).Used, w.DeadHero(0, 1).Used, w.FoodUsed(0))
	if len(spawned) != 2 {
		t.Fatalf("revives completed: %d", len(spawned))
	}
	first := w.Heroes.Row(spawned[0])
	if w.Heroes.Level[first] != 2 || w.Heroes.XP[first] != 225 {
		t.Fatalf("FIFO order broken: first revived %s", hdump(w, spawned[0]))
	}
	if w.DeadHero(0, 0).Used || w.DeadHero(0, 1).Used {
		t.Fatal("pool slots not freed")
	}
	if w.FoodUsed(0) != 10 { // both heroes alive again at 5 food each
		t.Fatalf("food after revives: %d", w.FoodUsed(0))
	}
	// cancel-refund path: kill one again, queue, cancel
	w.KillUnit(spawned[0])
	w.Step()
	gold = w.Resources(0, 0)
	if got := w.ReviveHero(altar, 0); got != TrainOK {
		t.Fatalf("re-revive: %d", got)
	}
	if !w.CancelTrain(altar, 0) {
		t.Fatal("cancel refused")
	}
	t.Logf("cancel refund: gold %d->%d pool[0].Used=%v", gold, w.Resources(0, 0), w.DeadHero(0, 0).Used)
	if w.Resources(0, 0) != gold || !w.DeadHero(0, 0).Used {
		t.Fatal("cancel did not refund / freed the record")
	}
}

// Edge 5: D-15 record round-trip onto a FRESH world — bit-identical
// re-extract.
func TestHeroRecordRoundTrip(t *testing.T) {
	w := heroWorld(t)
	id, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	w.AddXP(id, 250)
	w.LearnSkill(id, 0)
	rec, ok := w.ExtractHeroRecord(id)
	if !ok {
		t.Fatal("extract failed")
	}
	w2 := heroWorld(t)
	id2, ok := w2.InstantiateHero(&rec, 3, 1, pt2(400, 400))
	if !ok {
		t.Fatal("instantiate failed")
	}
	rec2, _ := w2.ExtractHeroRecord(id2)
	t.Logf("source record: %+v", rec)
	t.Logf("round-trip:    %+v", rec2)
	if rec != rec2 {
		t.Fatalf("record diff:\n%+v\n%+v", rec, rec2)
	}
	// derived state re-applied: life, mana, ability slot, armor fold
	if hdump(w, id)[5:] != hdump(w2, id2)[5:] {
		t.Fatalf("derived state diff:\n%s\n%s", hdump(w, id), hdump(w2, id2))
	}
	ar := w2.Abilities.Row(id2)
	if w2.Abilities.AbilityID[ar][0] != 1 || w2.Abilities.Level[ar][0] != 1 {
		t.Fatal("learned skill lost in round-trip")
	}
}

// Twin determinism + save: pools and rows hash, survive a v5
// round-trip, and resume identically through a queued revive.
func TestHeroDeterminismAndSave(t *testing.T) {
	build := func() *World {
		w := NewWorld(Caps{Units: 64})
		defs := heroUnitDefs()
		w.BindEconomy(2)
		w.BindUnitDefs(defs)
		w.BindAbilityDefs([]data.Ability{{ID: "holy-light"}})
		w.BindHeroes(heroRules(len(defs)))
		w.resources[0][0] = 1000
		altar, _ := w.SpawnFromTable(tAltar, 0, 0, pt2(200, 200))
		h1, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
		w.AddXP(h1, 250)
		w.LearnSkill(h1, 0)
		w.KillUnit(h1)
		w.Step()
		w.ReviveHero(altar, 0)
		for i := 0; i < 5; i++ {
			w.Step() // save mid-revive (10 ticks remain)
		}
		return w
	}
	a, b := build(), build()
	reg := NewHashRegistry()
	var sa, sb statehash.Snapshot
	a.HashState(reg, &sa)
	b.HashState(NewHashRegistry(), &sb)
	t.Logf("twin A=%016x B=%016x", sa.Top, sb.Top)
	if sa.Top != sb.Top {
		t.Fatal("twin divergence")
	}
	a.deadHeroes[0][0].XP++ // sensitivity: pool bytes are state
	var sa2 statehash.Snapshot
	a.HashState(reg, &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("dead-pool mutation invisible to the hash")
	}
	a.deadHeroes[0][0].XP--

	var buf bytes.Buffer
	if err := a.SaveState(&buf, 7); err != nil {
		t.Fatal(err)
	}
	w2 := NewWorld(Caps{Units: 64})
	defs := heroUnitDefs()
	w2.BindEconomy(2)
	w2.BindUnitDefs(defs)
	w2.BindAbilityDefs([]data.Ability{{ID: "holy-light"}})
	w2.BindHeroes(heroRules(len(defs)))
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 7); err != nil {
		t.Fatal(err)
	}
	var sl statehash.Snapshot
	w2.HashState(NewHashRegistry(), &sl)
	if sl.Top != sa.Top {
		t.Fatalf("load diverged: %016x vs %016x", sl.Top, sa.Top)
	}
	for i := 0; i < 20; i++ { // the queued revive completes in both
		a.Step()
		w2.Step()
	}
	a.HashState(reg, &sa)
	w2.HashState(NewHashRegistry(), &sl)
	t.Logf("resumed through revive: orig=%016x loaded=%016x heroes=%d/%d", sa.Top, sl.Top, a.Heroes.Count(), w2.Heroes.Count())
	if sa.Top != sl.Top || w2.Heroes.Count() != 1 {
		t.Fatal("resume through revive diverged")
	}
	// load without BindHeroes refuses by name
	w3 := NewWorld(Caps{Units: 64})
	w3.BindEconomy(2)
	w3.BindUnitDefs(defs)
	if err := w3.LoadState(bytes.NewReader(buf.Bytes()), 7); err == nil {
		t.Fatal("load without hero tables accepted")
	} else {
		t.Logf("unbound heroes refused: %v", err)
	}
}

// R-GC-1: hero-active combat ticks (XP grants on kills) allocate
// nothing.
func TestHeroTickAllocs(t *testing.T) {
	w := heroWorld(t)
	hero, _ := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	victims := make([]EntityID, 0, 40)
	for i := 0; i < 40; i++ {
		v, ok := w.SpawnFromTable(tWorker, 1, 1, pt2(110+int32(i), 100))
		if !ok {
			t.Fatal("victim spawn failed")
		}
		victims = append(victims, v)
	}
	i := 0
	w.OnCombatPhase = func(tick uint32) {
		if i < len(victims) {
			w.QueueDamage(DamagePacket{Source: hero, Target: victims[i], Amount: fixed.FromInt(10000)})
			i++
		}
	}
	w.Step()
	allocs := testing.AllocsPerRun(30, func() { w.Step() })
	t.Logf("allocs/op with kill-XP active: %v (hero %s)", allocs, hdump(w, hero))
	if allocs != 0 {
		t.Fatalf("hero tick allocates: %v", allocs)
	}
}

// TestHeroStatAccessorsFSV: the World hero-stat accessors (HeroLevel/HeroXP/
// HeroStr/HeroAgi/HeroInt/IsHero) read the matching Heroes store columns. SoT =
// the Heroes store rows. Paladin base attributes are deliberately distinct
// (str 25, agi 13, int 15) so any cross-wiring between columns is caught.
func TestHeroStatAccessorsFSV(t *testing.T) {
	w := heroWorld(t)
	id, ok := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	if !ok {
		t.Fatal("spawn failed")
	}
	r := w.Heroes.Row(id)
	t.Logf("spawn SoT: lvl=%d xp=%d str=%d agi=%d int=%d", w.Heroes.Level[r], w.Heroes.XP[r],
		int64(w.Heroes.Str[r]), int64(w.Heroes.Agi[r]), int64(w.Heroes.Int[r]))

	// Base attributes from the paladin hero table (distinct: str22/agi13/int17,
	// confirmed against the store SoT above — any cross-wiring would show).
	if !w.IsHero(id) {
		t.Fatal("IsHero false on a spawned hero")
	}
	if w.HeroLevel(id) != 1 || w.HeroXP(id) != 0 {
		t.Errorf("fresh hero: level=%d xp=%d, want 1/0", w.HeroLevel(id), w.HeroXP(id))
	}
	if w.HeroStr(id).Floor() != 22 || w.HeroAgi(id).Floor() != 13 || w.HeroInt(id).Floor() != 17 {
		t.Errorf("base attrs: str=%d agi=%d int=%d, want 22/13/17 (cross-wire?)",
			w.HeroStr(id).Floor(), w.HeroAgi(id).Floor(), w.HeroInt(id).Floor())
	}

	// Mutate the store to distinct values incl. a fractional Str (Floor test).
	w.Heroes.Level[r] = 5
	w.Heroes.XP[r] = 777
	w.Heroes.Str[r] = fixed.FromInt(40) + fixed.One/2 // 40.5 -> Floor 40
	w.Heroes.Agi[r] = fixed.FromInt(33)
	w.Heroes.Int[r] = fixed.FromInt(28)
	t.Logf("mutated SoT: lvl=%d xp=%d str=%d.5 agi=%d int=%d",
		w.Heroes.Level[r], w.Heroes.XP[r], w.Heroes.Str[r].Floor(), w.Heroes.Agi[r].Floor(), w.Heroes.Int[r].Floor())
	if w.HeroLevel(id) != 5 || w.HeroXP(id) != 777 {
		t.Errorf("mutated: level=%d xp=%d, want 5/777", w.HeroLevel(id), w.HeroXP(id))
	}
	if w.HeroStr(id).Floor() != 40 || w.HeroAgi(id).Floor() != 33 || w.HeroInt(id).Floor() != 28 {
		t.Errorf("mutated attrs: str=%d agi=%d int=%d, want 40/33/28",
			w.HeroStr(id).Floor(), w.HeroAgi(id).Floor(), w.HeroInt(id).Floor())
	}

	// EDGE: a non-hero unit -> all zero, IsHero false.
	worker, ok := w.SpawnFromTable(tWorker, 1, 1, pt2(120, 100))
	if !ok {
		t.Fatal("spawn worker failed")
	}
	if w.IsHero(worker) || w.HeroLevel(worker) != 0 || w.HeroXP(worker) != 0 ||
		w.HeroStr(worker) != 0 || w.HeroAgi(worker) != 0 || w.HeroInt(worker) != 0 {
		t.Errorf("non-hero worker: isHero=%v level=%d str=%d", w.IsHero(worker), w.HeroLevel(worker), w.HeroStr(worker).Floor())
	}

	// EDGE: a never-used entity id -> all zero, no panic.
	var nobody EntityID
	if w.IsHero(nobody) || w.HeroLevel(nobody) != 0 || w.HeroStr(nobody) != 0 {
		t.Error("zero EntityID reported hero state")
	}
}

// TestSetHeroStatsFSV: SetHeroStr/Agi/Int set the attribute AND apply the
// derived consequences. SoT = the derived stores (Healths.MaxLife/Life,
// Abilities.MaxMana, buffAdd[StatArmor]) plus the Heroes attribute columns.
// Paladin coeffs: StrHP=25, IntMana=15, AgiArmor=0.3 (base str22/agi13/int17 ->
// life650 mana255 armor3).
func TestSetHeroStatsFSV(t *testing.T) {
	w := heroWorld(t)
	id, ok := w.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	if !ok {
		t.Fatal("spawn failed")
	}
	hr := w.Healths.Row(id)
	ar := w.Abilities.Row(id)
	idx := id.Index()
	t.Logf("BEFORE: str=%d life=%d/%d int=%d mana=%d agi=%d armorAdd=%d",
		w.HeroStr(id).Floor(), w.Healths.Life[hr].Floor(), w.Healths.MaxLife[hr].Floor(),
		w.HeroInt(id).Floor(), w.Abilities.MaxMana[ar].Floor(), w.HeroAgi(id).Floor(),
		w.buffAdd[data.StatArmor][idx])
	if w.Healths.MaxLife[hr] != fixed.FromInt(650) || w.Abilities.MaxMana[ar] != fixed.FromInt(255) || w.buffAdd[data.StatArmor][idx] != 3 {
		t.Fatalf("unexpected baseline: life=%d mana=%d armor=%d", w.Healths.MaxLife[hr].Floor(), w.Abilities.MaxMana[ar].Floor(), w.buffAdd[data.StatArmor][idx])
	}

	// Strength 22 -> 32 (+10): max life +250 = 900, current life tracks.
	if !w.SetHeroStr(id, fixed.FromInt(32)) {
		t.Fatal("SetHeroStr returned false")
	}
	t.Logf("AFTER SetHeroStr(32): str=%d life=%d/%d", w.HeroStr(id).Floor(), w.Healths.Life[hr].Floor(), w.Healths.MaxLife[hr].Floor())
	if w.HeroStr(id).Floor() != 32 || w.Healths.MaxLife[hr] != fixed.FromInt(900) || w.Healths.Life[hr] != fixed.FromInt(900) {
		t.Errorf("str set: str=%d maxLife=%d life=%d, want 32/900/900", w.HeroStr(id).Floor(), w.Healths.MaxLife[hr].Floor(), w.Healths.Life[hr].Floor())
	}

	// Intelligence 17 -> 27 (+10): max mana +150 = 405.
	if !w.SetHeroInt(id, fixed.FromInt(27)) {
		t.Fatal("SetHeroInt returned false")
	}
	if w.HeroInt(id).Floor() != 27 || w.Abilities.MaxMana[ar] != fixed.FromInt(405) {
		t.Errorf("int set: int=%d maxMana=%d, want 27/405", w.HeroInt(id).Floor(), w.Abilities.MaxMana[ar].Floor())
	}

	// Agility 13 -> 23: armor add floor(23*0.3)=floor(6.9)=6.
	if !w.SetHeroAgi(id, fixed.FromInt(23)) {
		t.Fatal("SetHeroAgi returned false")
	}
	t.Logf("AFTER SetHeroAgi(23): agi=%d armorAdd=%d", w.HeroAgi(id).Floor(), w.buffAdd[data.StatArmor][idx])
	if w.HeroAgi(id).Floor() != 23 || w.buffAdd[data.StatArmor][idx] != 6 {
		t.Errorf("agi set: agi=%d armorAdd=%d, want 23/6", w.HeroAgi(id).Floor(), w.buffAdd[data.StatArmor][idx])
	}

	// EDGE: lowering strength 32 -> 22 returns max life to 650.
	if !w.SetHeroStr(id, fixed.FromInt(22)) {
		t.Fatal("SetHeroStr lower returned false")
	}
	if w.Healths.MaxLife[hr] != fixed.FromInt(650) {
		t.Errorf("str lowered: maxLife=%d, want 650", w.Healths.MaxLife[hr].Floor())
	}

	// EDGE: a non-hero unit -> false, no panic, no derived change.
	worker, _ := w.SpawnFromTable(tWorker, 1, 1, pt2(140, 100))
	wl := w.Healths.MaxLife[w.Healths.Row(worker)]
	if w.SetHeroStr(worker, fixed.FromInt(50)) {
		t.Error("SetHeroStr on a non-hero returned true")
	}
	if w.Healths.MaxLife[w.Healths.Row(worker)] != wl {
		t.Error("SetHeroStr changed a non-hero's life")
	}
}
