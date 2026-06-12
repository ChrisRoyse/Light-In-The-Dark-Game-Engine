package sim

// #206 tests: full state serialization (R-SIM-6). SoT = the state
// hash (17 systems, #334-extended) recomputed from the restored
// world, the save bytes themselves, and the named refusal errors.

import (
	"bytes"
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
	b[len(SaveMagic)]++ // version field
	err := NewWorld(Caps{}).LoadState(bytes.NewReader(b), tb.Fingerprint)
	if err == nil {
		t.Fatal("version mismatch accepted")
	}
	t.Logf("refusal: %v", err)
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
	headerLen := len(SaveMagic) + 4 + 8 + 7*4 + 4 + 4 + 16
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
