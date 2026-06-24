package sim

// #206 tests: full state serialization (R-SIM-6). SoT = the state
// hash in HashSystems order recomputed from the restored world, the
// save bytes themselves, and the named refusal errors.

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	saveTestCont    sched.ContID = 7
	saveTestHandler HandlerID    = 11
)

// bindSaveWorld constructs an EMPTY world with the full registry
// surface bound — data tables, effect execs, scheduler continuation,
// event handler + subscription. Construction is code, not state: the
// load target must be built exactly like the source was.
func bindSaveWorld(t *testing.T, tb *data.Tables, fired *[]uint32) *World {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(tb.Coeff); err != nil {
		t.Fatal(err)
	}
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	if !w.BindBuffTypes(tb.BuffTypes) {
		t.Fatal("BindBuffTypes failed")
	}
	w.Sched.Register(saveTestCont, func(s *sched.Scheduler, st sched.State) {
		*fired = append(*fired, s.Now())
	})
	w.RegisterHandler(saveTestHandler, func(w *World, e Event) {})
	w.Subscribe(EvUnitDeath, saveTestHandler)
	return w
}

// saveWorld populates a bound world into a mid-combat scene: ranged
// units launching missiles, an active periodic buff stack, a current
// order with two queued pooled entries, and a scheduler suspension
// that crosses the save point.
func saveWorld(t *testing.T) (*World, *data.Tables, *[]uint32) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := buffTables(t)
	fired := &[]uint32{}
	w := bindSaveWorld(t, tb, fired)
	w.SetSeed(0xFACE)

	ranged := data.Attack{
		AttackType: 0, Range: 100 * fixed.One,
		DamageBase: 1, Dice: 1, Sides: 2,
		CooldownTicks: 20, DamagePointTicks: 4, BackswingTicks: 4,
		ProjectileSpeedPerTick: fixed.One, // 80-wu gap -> long flights
	}
	var ids []EntityID
	for i := 0; i < 6; i++ {
		team := uint8(i % 2)
		x := fixed.FromInt(int32(1000 + 80*int32(team)))
		y := fixed.FromInt(int32(1000 + 8*int32(i/2)))
		id := atkUnit(t, w, team, fixed.Vec2{X: x, Y: y}, fixed.One*2)
		w.Healths.MaxLife[w.Healths.Row(id)] = 100000 * fixed.One
		w.Healths.Life[w.Healths.Row(id)] = 100000 * fixed.One
		if team == 0 {
			if !w.SetWeapon(id, 0, &ranged, 0, data.EffectList{}) {
				t.Fatal("SetWeapon failed")
			}
		} else {
			arm(t, w, id, 0, 0)
		}
		w.Combats.AcquisitionRange[w.Combats.Row(id)] = 200 * fixed.One
		ids = append(ids, id)
	}
	// per-unit custom values + a hidden unit straddle the save point (#217):
	// sparse stores with no sim consumer, so they must survive purely via
	// save/load and contribute to the state hash.
	w.SetUserData(ids[0], 0x7FFFFFFF) // int32 max
	w.SetUserData(ids[4], -12345)
	w.ShowUnit(ids[5], false)                  // hide
	w.SetUnitName(ids[2], "Synthetic Champion") // per-instance name override (#217)
	// current order + two queued pooled entries
	pt := func(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }
	if !w.IssueOrder(ids[0], Order{Kind: OrderMove, Point: pt(1100, 1100)}, false) ||
		!w.IssueOrder(ids[0], Order{Kind: OrderMove, Point: pt(1200, 1000)}, true) ||
		!w.IssueOrder(ids[0], Order{Kind: OrderMove, Point: pt(900, 900)}, true) {
		t.Fatal("IssueOrder failed")
	}
	for i := 0; i < 40; i++ {
		w.Step()
	}
	// active buffs, applied between ticks so they straddle the save
	// point: 2-stack periodic poison (40t) + a slow (20t)
	if !w.ApplyBuff(ids[1], ids[0], buffTypeIdx(t, tb, "poison"), 2) ||
		!w.ApplyBuff(ids[3], ids[2], buffTypeIdx(t, tb, "slow"), 1) {
		t.Fatal("ApplyBuff failed")
	}
	// the mid-Wait suspension: sleeps across the save point, wakes
	// ~50 ticks after it
	w.Sched.After(60, saveTestCont, sched.State{42, 0, 0, 0})
	for i := 0; i < 10; i++ {
		w.Step()
	}
	if w.Missiles.Count() == 0 {
		t.Fatal("degenerate fixture: no missiles in flight at the save point")
	}
	if w.Buffs.Live() == 0 {
		t.Fatal("degenerate fixture: no live buffs at the save point")
	}
	if w.Sched.PendingSleepers() == 0 {
		t.Fatal("degenerate fixture: no pending suspension at the save point")
	}
	if w.KV.Count() == 0 {
		t.Fatal("degenerate fixture: no kv/userdata pairs at the save point")
	}
	if w.Hiddens.Count() == 0 {
		t.Fatal("degenerate fixture: no hidden units at the save point")
	}
	if w.UnitNames.Count() == 0 {
		t.Fatal("degenerate fixture: no name overrides at the save point")
	}
	return w, tb, fired
}

func snapDiff(t *testing.T, want, got *statehash.Snapshot) (diverged []string) {
	t.Helper()
	for i := range HashSystems {
		if want.Subs[i] != got.Subs[i] {
			diverged = append(diverged, HashSystems[i])
		}
	}
	return
}

func hashSystemIndex(t *testing.T, name string) int {
	t.Helper()
	for i, got := range HashSystems {
		if got == name {
			return i
		}
	}
	t.Fatalf("hash system %q not found in %v", name, HashSystems)
	return -1
}

// The acceptance criterion: hash before save == hash after load, per
// system; resuming the loaded world stays hash-identical to the
// original run, the suspension fires at the same tick in both, and
// re-saving the loaded world reproduces the bytes exactly.
func TestSaveRoundTrip(t *testing.T) {
	src, tb, firedSrc := saveWorld(t)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	t.Logf("save: %d bytes at tick %d (units=%d missiles=%d buffs=%d sleepers=%d)",
		len(saved), src.Tick(), src.UnitCount(), src.Missiles.Count(), src.Buffs.Live(), src.Sched.PendingSleepers())
	t.Logf("header: % x", saved[:32])

	firedDst := &[]uint32{}
	resetEffectExecs() // twin construction repeats the global registration
	RegisterCoreEffectExecs()
	dst := bindSaveWorld(t, tb, firedDst)
	if err := dst.LoadState(bytes.NewReader(saved), tb.Fingerprint); err != nil {
		t.Fatal(err)
	}

	var after statehash.Snapshot
	dst.HashState(reg, &after)
	t.Logf("hash before save = %016x", before.Top)
	t.Logf("hash after load  = %016x", after.Top)
	for i, name := range HashSystems {
		t.Logf("sub %-10s before=%016x after=%016x", name, before.Subs[i], after.Subs[i])
	}
	if before.Top != after.Top {
		t.Fatalf("restored hash differs; diverged systems: %v", snapDiff(t, &before, &after))
	}

	// re-save the restored world: byte-identical file
	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("re-save of the restored world is not byte-identical")
	}
	t.Logf("re-save byte-identical: %d bytes", buf2.Len())

	// resume both 100 ticks: still hash-identical, suspension fires
	// at the same tick in both worlds
	var sa, sb statehash.Snapshot
	for i := 0; i < 100; i++ {
		src.Step()
		dst.Step()
	}
	src.HashState(reg, &sa)
	dst.HashState(reg, &sb)
	if sa.Top != sb.Top {
		t.Fatalf("resumed runs diverged after 100 ticks; systems: %v", snapDiff(t, &sa, &sb))
	}
	if len(*firedSrc) == 0 || !equalU32(*firedSrc, *firedDst) {
		t.Fatalf("mid-Wait suspension fire ticks differ: src=%v dst=%v", *firedSrc, *firedDst)
	}
	t.Logf("resumed 100 ticks: both hashes %016x; suspension fired at tick %v in BOTH worlds", sa.Top, *firedSrc)
}

const (
	saveClockSectionLen     = 8 + 8 + 1 + 8 + 4
	saveGameStateSectionLen = MaxPlayers
	saveEffectRowLen        = 2 + 8 + 4 + 4 + 4
	saveAbilityFieldRowLen  = 4 + 4 + 1 + 1 + 8
)

func effectSectionLenForTest(w *World) int {
	return 4 + int(w.Effects.Count())*saveEffectRowLen
}

func abilityFieldSectionLenForTest(w *World) int {
	return 4 + int(w.AbilityFields.Count())*saveAbilityFieldRowLen + 4 + len(w.AbilityFields.free)*4 + 8
}

func abilityDefSectionLenForTest(w *World) int {
	n := 4
	for i := range w.runtimeAbilityDefs {
		def := &w.runtimeAbilityDefs[i]
		n += 4 + len(def.ID)
		n += 4 + len(def.Name)
		n += 2 + 2 + 4 + 2 + 2 + 2 + 2 + 8
	}
	return n
}

func clockSectionOffsetForTest(w *World, saved []byte) int {
	blob := w.Sched.Save(make([]byte, 0, w.Sched.SaveSize()))
	return len(saved) - 4 - 4 - len(blob) - abilityDefSectionLenForTest(w) - abilityFieldSectionLenForTest(w) -
		effectSectionLenForTest(w) - saveGameStateSectionLen - saveClockSectionLen
}

func gameStateSectionOffsetForTest(w *World, saved []byte) int {
	return clockSectionOffsetForTest(w, saved) + saveClockSectionLen
}

func effectSectionOffsetForTest(w *World, saved []byte) int {
	return gameStateSectionOffsetForTest(w, saved) + saveGameStateSectionLen
}

func abilityFieldSectionOffsetForTest(w *World, saved []byte) int {
	return effectSectionOffsetForTest(w, saved) + effectSectionLenForTest(w)
}

func abilityDefSectionOffsetForTest(w *World, saved []byte) int {
	return abilityFieldSectionOffsetForTest(w, saved) + abilityFieldSectionLenForTest(w)
}

func TestSaveClockRoundTripAndResume(t *testing.T) {
	src := NewWorld(Caps{})
	src.SetTimeOfDay(13*fixed.One + 37*fixed.One/100)
	if !src.SetTimeOfDayScale(2*fixed.One + fixed.One/2) {
		t.Fatal("positive scale rejected")
	}
	for i := 0; i < 7; i++ {
		src.Step()
	}
	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	clockOff := clockSectionOffsetForTest(src, saved)
	t.Logf("FSV clock save BEFORE: %s hash=%016x", clockFSV(src), before.Top)
	t.Logf("FSV clock section offset=%d hex=% x", clockOff, saved[clockOff:clockOff+saveClockSectionLen])

	dst := NewWorld(Caps{})
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatal(err)
	}
	var after statehash.Snapshot
	dst.HashState(reg, &after)
	t.Logf("FSV clock load AFTER:  %s hash=%016x", clockFSV(dst), after.Top)
	clockIdx := hashSystemIndex(t, "clock")
	t.Logf("FSV clock subhash before=%016x after=%016x", before.Subs[clockIdx], after.Subs[clockIdx])
	if before.Top != after.Top {
		t.Fatalf("clock round-trip hash mismatch; systems=%v", snapDiff(t, &before, &after))
	}
	if src.tod != dst.tod || src.todScale != dst.todScale || src.todFrozen != dst.todFrozen ||
		src.todCarry != dst.todCarry || src.dayLengthTicks != dst.dayLengthTicks {
		t.Fatalf("clock fields did not round-trip:\nsrc %s\ndst %s", clockFSV(src), clockFSV(dst))
	}

	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, 0); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("clock round-trip re-save is not byte-identical")
	}

	for i := 0; i < 300; i++ {
		src.Step()
		dst.Step()
	}
	var resumedSrc, resumedDst statehash.Snapshot
	src.HashState(reg, &resumedSrc)
	dst.HashState(reg, &resumedDst)
	t.Logf("FSV clock resume src: %s hash=%016x", clockFSV(src), resumedSrc.Top)
	t.Logf("FSV clock resume dst: %s hash=%016x", clockFSV(dst), resumedDst.Top)
	if resumedSrc.Top != resumedDst.Top {
		t.Fatalf("resumed clock worlds diverged; systems=%v", snapDiff(t, &resumedSrc, &resumedDst))
	}
}

func TestSaveClockFrozenRoundTripBytes(t *testing.T) {
	src := NewWorld(Caps{})
	src.SetTimeOfDay(13*fixed.One + 37*fixed.One/100)
	src.SuspendTimeOfDay(true)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	dst := NewWorld(Caps{})
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatal(err)
	}
	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, 0); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV frozen clock saved: %s", clockFSV(src))
	t.Logf("FSV frozen clock loaded: %s", clockFSV(dst))
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("frozen clock re-save is not byte-identical")
	}
	if dst.tod != src.tod || !dst.todFrozen || dst.todCarry != src.todCarry {
		t.Fatalf("frozen clock fields changed:\nsrc %s\ndst %s", clockFSV(src), clockFSV(dst))
	}
}

func TestSaveClockTruncatedSectionRefusesAndDoesNotMutate(t *testing.T) {
	src := NewWorld(Caps{})
	src.SetTimeOfDay(23*fixed.One + 99*fixed.One/100)
	for i := 0; i < 8; i++ {
		src.Step()
	}
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	full := buf.Bytes()
	clockOff := clockSectionOffsetForTest(src, full)

	dst := NewWorld(Caps{})
	dst.SetTimeOfDay(7 * fixed.One)
	if !dst.SetTimeOfDayScale(3 * fixed.One) {
		t.Fatal("positive scale rejected")
	}
	dst.SuspendTimeOfDay(true)
	reg := NewHashRegistry()
	var pristine, after statehash.Snapshot
	dst.HashState(reg, &pristine)
	before := clockFSV(dst)
	err := dst.LoadState(bytes.NewReader(full[:clockOff+10]), 0)
	if err == nil {
		t.Fatal("truncated clock section accepted")
	}
	dst.HashState(reg, &after)
	t.Logf("FSV truncated clock source: %s section=% x", clockFSV(src), full[clockOff:clockOff+saveClockSectionLen])
	t.Logf("FSV truncated clock target BEFORE: %s hash=%016x", before, pristine.Top)
	t.Logf("FSV truncated clock target AFTER:  %s hash=%016x err=%v", clockFSV(dst), after.Top, err)
	if pristine.Top != after.Top || before != clockFSV(dst) {
		t.Fatal("failed clock load mutated the target world")
	}
}

func TestHashClockDivergence(t *testing.T) {
	reg := NewHashRegistry()
	w1 := NewWorld(Caps{})
	w2 := NewWorld(Caps{})
	w1.SetTimeOfDay(6 * fixed.One)
	w2.SetTimeOfDay(6 * fixed.One)
	w2.SuspendTimeOfDay(true)
	w1.Step()
	w2.Step()
	var a, b statehash.Snapshot
	w1.HashState(reg, &a)
	w2.HashState(reg, &b)
	culprit, ok := reg.FirstDivergence(&a, &b)
	clockIdx := hashSystemIndex(t, "clock")
	t.Logf("FSV clock hash active: %s top=%016x clock=%016x", clockFSV(w1), a.Top, a.Subs[clockIdx])
	t.Logf("FSV clock hash frozen: %s top=%016x clock=%016x", clockFSV(w2), b.Top, b.Subs[clockIdx])
	if !ok || culprit != "clock" {
		t.Fatalf("first divergence=%q ok=%v, want clock", culprit, ok)
	}

	w3 := NewWorld(Caps{})
	w4 := NewWorld(Caps{})
	w3.SetTimeOfDay(5 * fixed.One)
	w4.SetTimeOfDay(5 * fixed.One)
	w4.todCarry = 1
	w3.HashState(reg, &a)
	w4.HashState(reg, &b)
	culprit, ok = reg.FirstDivergence(&a, &b)
	t.Logf("FSV clock carry hash zero: %s clock=%016x", clockFSV(w3), a.Subs[clockIdx])
	t.Logf("FSV clock carry hash one:  %s clock=%016x", clockFSV(w4), b.Subs[clockIdx])
	if !ok || culprit != "clock" {
		t.Fatalf("carry first divergence=%q ok=%v, want clock", culprit, ok)
	}
}

func gameStateFSV(w *World) string {
	return fmt.Sprintf("tick=%d results=%v pending=%v", w.Tick(), w.results, w.resultPending)
}

func settleGameState(t *testing.T, w *World) {
	t.Helper()
	if !w.SetVictory(0) || !w.SetDefeat(1) || !w.SetLeft(3) {
		t.Fatalf("failed to stage mixed gamestate: %s", gameStateFSV(w))
	}
	w.Step()
}

func TestSaveGameStateRoundTripBytes(t *testing.T) {
	src := NewWorld(Caps{})
	settleGameState(t, src)
	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	gsOff := gameStateSectionOffsetForTest(src, saved)

	dst := NewWorld(Caps{})
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatal(err)
	}
	var after statehash.Snapshot
	dst.HashState(reg, &after)
	gsIdx := hashSystemIndex(t, "gamestate")
	t.Logf("FSV gamestate save BEFORE: %s hash=%016x gamestate=%016x", gameStateFSV(src), before.Top, before.Subs[gsIdx])
	t.Logf("FSV gamestate section offset=%d hex=% x", gsOff, saved[gsOff:gsOff+saveGameStateSectionLen])
	t.Logf("FSV gamestate load AFTER:  %s hash=%016x gamestate=%016x", gameStateFSV(dst), after.Top, after.Subs[gsIdx])
	if before.Top != after.Top {
		t.Fatalf("gamestate round-trip hash mismatch; systems=%v", snapDiff(t, &before, &after))
	}
	if src.results != dst.results || dst.resultPending != ([MaxPlayers]uint8{}) {
		t.Fatalf("gamestate did not round-trip cleanly:\nsrc %s\ndst %s", gameStateFSV(src), gameStateFSV(dst))
	}

	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, 0); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("gamestate round-trip re-save is not byte-identical")
	}
}

func TestHashGameStateDivergence(t *testing.T) {
	reg := NewHashRegistry()
	won := NewWorld(Caps{})
	lost := NewWorld(Caps{})
	if !won.SetVictory(0) || !lost.SetDefeat(0) {
		t.Fatal("failed to stage gamestate divergence")
	}
	won.Step()
	lost.Step()
	var a, b statehash.Snapshot
	won.HashState(reg, &a)
	lost.HashState(reg, &b)
	culprit, ok := reg.FirstDivergence(&a, &b)
	gsIdx := hashSystemIndex(t, "gamestate")
	t.Logf("FSV gamestate hash won:  %s top=%016x gamestate=%016x", gameStateFSV(won), a.Top, a.Subs[gsIdx])
	t.Logf("FSV gamestate hash lost: %s top=%016x gamestate=%016x", gameStateFSV(lost), b.Top, b.Subs[gsIdx])
	if !ok || culprit != "gamestate" {
		t.Fatalf("first divergence=%q ok=%v, want gamestate", culprit, ok)
	}
}

func TestSaveGameStateTruncatedSectionRefusesAndDoesNotMutate(t *testing.T) {
	src := NewWorld(Caps{})
	settleGameState(t, src)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	full := buf.Bytes()
	gsOff := gameStateSectionOffsetForTest(src, full)

	dst := NewWorld(Caps{})
	if !dst.SetDefeat(2) {
		t.Fatal("failed to stage target result")
	}
	dst.Step()
	reg := NewHashRegistry()
	var pristine, after statehash.Snapshot
	dst.HashState(reg, &pristine)
	before := gameStateFSV(dst)
	err := dst.LoadState(bytes.NewReader(full[:gsOff+5]), 0)
	if err == nil {
		t.Fatal("truncated gamestate section accepted")
	}
	dst.HashState(reg, &after)
	t.Logf("FSV truncated gamestate source: %s section=% x", gameStateFSV(src), full[gsOff:gsOff+saveGameStateSectionLen])
	t.Logf("FSV truncated gamestate target BEFORE: %s hash=%016x", before, pristine.Top)
	t.Logf("FSV truncated gamestate target AFTER:  %s hash=%016x err=%v", gameStateFSV(dst), after.Top, err)
	if pristine.Top != after.Top || before != gameStateFSV(dst) {
		t.Fatal("failed gamestate load mutated the target world")
	}
}

func TestSaveGameStatePendingRefused(t *testing.T) {
	w := NewWorld(Caps{})
	if !w.SetVictory(0) {
		t.Fatal("failed to stage pending victory")
	}
	before := gameStateFSV(w)
	var buf bytes.Buffer
	err := w.SaveState(&buf, 0)
	after := gameStateFSV(w)
	t.Logf("FSV pending gamestate save BEFORE: %s", before)
	t.Logf("FSV pending gamestate save AFTER:  %s bytes=%d err=%v", after, buf.Len(), err)
	if err == nil {
		t.Fatal("pending result save accepted")
	}
	if before != after || buf.Len() != 0 {
		t.Fatalf("pending save mutated/wrote state: before=%s after=%s bytes=%d", before, after, buf.Len())
	}
}

func equalU32(a, b []uint32) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// Edge: saving mid-tick (queued damage in flight) is refused.
func TestSaveRefusesMidTick(t *testing.T) {
	src, tb, _ := saveWorld(t)
	var hookErr error
	src.OnCombatPhase = func(tick uint32) {
		src.QueueDamage(DamagePacket{Source: 0, Target: src.Transforms.Entity[0], Amount: fixed.One})
		var buf bytes.Buffer
		hookErr = src.SaveState(&buf, tb.Fingerprint)
	}
	src.Step()
	if hookErr == nil {
		t.Fatal("mid-tick save was not refused")
	}
	t.Logf("refusal: %v", hookErr)
}

// Edge: load into a world with different caps is refused by name.
func TestSaveLoadCapsMismatch(t *testing.T) {
	src, tb, _ := saveWorld(t)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	dst := NewWorld(Caps{Units: 100})
	err := dst.LoadState(&buf, tb.Fingerprint)
	if err == nil {
		t.Fatal("caps mismatch accepted")
	}
	t.Logf("refusal: %v", err)
}

// Edge: a bumped format version is refused before any body decode.
func TestSaveLoadVersionRefused(t *testing.T) {
	src, tb, _ := saveWorld(t)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	b := buf.Bytes()
	for _, tc := range []struct {
		name string
		edit func([]byte)
	}{
		{"future", func(b []byte) { b[len(SaveMagic)]++ }},
		{"previous", func(b []byte) { b[len(SaveMagic)]-- }},
	} {
		mut := append([]byte(nil), b...)
		tc.edit(mut)
		err := NewWorld(Caps{}).LoadState(bytes.NewReader(mut), tb.Fingerprint)
		if err == nil {
			t.Fatalf("%s version mismatch accepted", tc.name)
		}
		t.Logf("%s version refusal: %v", tc.name, err)
	}
}

// Edge: fingerprint mismatch (same save, different bound tables) is
// refused before any body decode.
func TestSaveLoadFingerprintRefused(t *testing.T) {
	src, tb, _ := saveWorld(t)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	err := NewWorld(Caps{}).LoadState(bytes.NewReader(buf.Bytes()), tb.Fingerprint^1)
	if err == nil {
		t.Fatal("fingerprint mismatch accepted")
	}
	t.Logf("refusal: %v", err)
}

// Edge: truncation anywhere and trailing garbage are both named
// errors, and the target world is untouched (hash unchanged).
func TestSaveLoadTruncationAndTrailing(t *testing.T) {
	src, tb, firedSrc := saveWorld(t)
	_ = firedSrc
	var buf bytes.Buffer
	if err := src.SaveState(&buf, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	full := buf.Bytes()

	resetEffectExecs()
	RegisterCoreEffectExecs()
	firedDst := &[]uint32{}
	dst := bindSaveWorld(t, tb, firedDst)
	reg := NewHashRegistry()
	var pristine, after statehash.Snapshot
	dst.HashState(reg, &pristine)

	for _, cut := range []int{len(full) / 4, len(full) / 2, len(full) - 3} {
		err := dst.LoadState(bytes.NewReader(full[:cut]), tb.Fingerprint)
		if err == nil {
			t.Fatalf("truncation at %d accepted", cut)
		}
		t.Logf("truncate@%d refusal: %v", cut, err)
	}
	trailing := append(append([]byte(nil), full...), 0xEE)
	if err := dst.LoadState(bytes.NewReader(trailing), tb.Fingerprint); err == nil {
		t.Fatal("trailing byte accepted")
	} else {
		t.Logf("trailing refusal: %v", err)
	}
	dst.HashState(reg, &after)
	if pristine.Top != after.Top {
		t.Fatal("failed loads mutated the target world")
	}
	t.Logf("target world untouched across 4 failed loads: hash %016x", after.Top)
}

// Edge: a save whose subscription table references a handler the
// target never registered is refused (registries are code, not
// state — same contract as scheduler continuations).
func TestSaveLoadUnregisteredHandlerRefused(t *testing.T) {
	src, tb, _ := saveWorld(t)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	// target world: same tables, NO handler/cont registration
	resetEffectExecs()
	RegisterCoreEffectExecs()
	bare := NewWorld(Caps{})
	if err := bare.BindDamageMatrix(tb.Coeff); err != nil {
		t.Fatal(err)
	}
	if err := bare.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	if !bare.BindBuffTypes(tb.BuffTypes) {
		t.Fatal("BindBuffTypes failed")
	}
	err := bare.LoadState(bytes.NewReader(buf.Bytes()), tb.Fingerprint)
	if err == nil {
		t.Fatal("unregistered handler/continuation accepted")
	}
	t.Logf("refusal: %v", err)
}

// The saved set is the hashed set: flipping any single payload byte
// after the header must either refuse to load or change the restored
// hash — a corruption can never restore silently hash-identical.
func TestSaveCorruptionNeverSilent(t *testing.T) {
	src, tb, _ := saveWorld(t)
	reg := NewHashRegistry()
	var want statehash.Snapshot
	src.HashState(reg, &want)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	full := buf.Bytes()
	headerLen := len(SaveMagic) + 4 + 8 + 9*4 + 4 + 4 + 16
	step := len(full)/97 + 1
	refused, changed := 0, 0
	for off := headerLen; off < len(full); off += step {
		mut := append([]byte(nil), full...)
		mut[off] ^= 0x01
		resetEffectExecs()
		RegisterCoreEffectExecs()
		fired := &[]uint32{}
		dst := bindSaveWorld(t, tb, fired)
		if err := dst.LoadState(bytes.NewReader(mut), tb.Fingerprint); err != nil {
			refused++
			continue
		}
		var got statehash.Snapshot
		dst.HashState(reg, &got)
		if got.Top == want.Top {
			t.Fatalf("byte flip at offset %d loaded AND restored hash-identical — saved set ⊄ hashed set", off)
		}
		changed++
	}
	if refused+changed == 0 {
		t.Fatal("degenerate: no offsets probed")
	}
	t.Logf("%d single-byte corruptions probed: %d refused, %d loaded with changed hash, 0 silent", refused+changed, refused, changed)
}
