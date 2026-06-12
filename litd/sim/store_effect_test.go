package sim

import (
	"bytes"
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func eff(x int32) fixed.F64 { return fixed.FromInt(x) }

func effectSpec(model uint16, x, y, scale int32, color uint32) EffectSpec {
	return EffectSpec{
		ModelID:   model,
		Pos:       fixed.Vec2{X: eff(x), Y: eff(y)},
		Scale:     eff(scale),
		ColorRGBA: color,
	}
}

func effectRows(w *World) string {
	var b strings.Builder
	fmt.Fprintf(&b, "count=%d cap=%d rows=[", w.Effects.Count(), w.Effects.Cap())
	for r := int32(0); r < w.Effects.Count(); r++ {
		id := w.Effects.Entity[r]
		tr := w.Transforms.Row(id)
		pos := fixed.Vec2{}
		if tr != -1 {
			pos = w.Transforms.Pos[tr]
		}
		if r > 0 {
			b.WriteString(" ")
		}
		fmt.Fprintf(&b, "{r=%d id=%d model=%d scale=%d color=%08x birth=%d pos=(%d,%d)}",
			r, uint32(id), w.Effects.ModelID[r], w.Effects.Scale[r].Floor(),
			w.Effects.ColorRGBA[r], w.Effects.BirthTick[r], pos.X.Floor(), pos.Y.Floor())
	}
	b.WriteString("]")
	return b.String()
}

func entityFreeChain(w *World, limit int) string {
	var b strings.Builder
	fmt.Fprintf(&b, "head=%d chain=[", w.Ents.freeHead)
	seen := 0
	for e := w.Ents.freeHead; e != -1 && seen < limit; e = w.Ents.slots[e].next {
		if seen > 0 {
			b.WriteString(" ")
		}
		fmt.Fprintf(&b, "%d", e)
		seen++
	}
	if seen == limit {
		b.WriteString(" ...")
	}
	b.WriteString("]")
	return b.String()
}

func effectDump(w *World, id EntityID) string {
	r := w.Effects.Row(id)
	tr := w.Transforms.Row(id)
	if r == -1 {
		return fmt.Sprintf("id=%d alive=%v effectRow=-1 transformRow=%d count=%d",
			uint32(id), w.Ents.Alive(id), tr, w.Effects.Count())
	}
	pos := fixed.Vec2{}
	if tr != -1 {
		pos = w.Transforms.Pos[tr]
	}
	return fmt.Sprintf("id=%d alive=%v effectRow=%d transformRow=%d model=%d scale=%d color=%08x birth=%d pos=(%d,%d) count=%d",
		uint32(id), w.Ents.Alive(id), r, tr, w.Effects.ModelID[r],
		w.Effects.Scale[r].Floor(), w.Effects.ColorRGBA[r], w.Effects.BirthTick[r],
		pos.X.Floor(), pos.Y.Floor(), w.Effects.Count())
}

func effectSaveCaps() Caps {
	return Caps{Units: 1, Projectiles: 1, Effects: 4, ScriptedDoodads: 1}
}

func populateEffectSaveWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(effectSaveCaps())
	if _, ok := w.SpawnEffect(effectSpec(21, 7, 8, 1, 0xff00ffff)); !ok {
		t.Fatal("SpawnEffect first failed")
	}
	w.Step()
	if _, ok := w.SpawnEffect(effectSpec(22, 9, 10, 2, 0x11223344)); !ok {
		t.Fatal("SpawnEffect second failed")
	}
	if _, ok := w.SpawnEffect(effectSpec(23, 11, 12, 3, 0xaabbccdd)); !ok {
		t.Fatal("SpawnEffect third failed")
	}
	w.Step()
	return w
}

func effectSnapshot(t *testing.T, w *World) statehash.Snapshot {
	t.Helper()
	reg := NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	return snap
}

func assertEffectRowOf(t *testing.T, w *World) {
	t.Helper()
	for i := int32(0); i < w.Effects.Count(); i++ {
		id := w.Effects.Entity[i]
		if got := w.Effects.Row(id); got != i {
			t.Fatalf("effect rowOf for %d = %d, want %d; %s", id, got, i, effectRows(w))
		}
		if tr := w.Transforms.Row(id); tr == -1 {
			t.Fatalf("effect %d has no transform row after load; %s", id, effectRows(w))
		}
	}
}

func findSnapshotEntry(s *Snapshot, id EntityID) (SnapshotEntry, bool) {
	for i := range s.Entries {
		if s.Entries[i].ID == id {
			return s.Entries[i], true
		}
	}
	return SnapshotEntry{}, false
}

func TestEffectSpawnPoolExhaustion(t *testing.T) {
	w := NewWorld(Caps{Units: 1, Projectiles: 1, Effects: 2, ScriptedDoodads: 1})
	before := effectRows(w)
	id1, ok1 := w.SpawnEffect(effectSpec(7, 10, 20, 2, 0x11223344))
	id2, ok2 := w.SpawnEffect(effectSpec(8, 30, 40, 3, 0x55667788))
	full := effectRows(w)
	id3, ok3 := w.SpawnEffect(effectSpec(9, 50, 60, 4, 0x99aabbcc))
	after := effectRows(w)

	t.Logf("FSV effect exhaustion BEFORE: %s", before)
	t.Logf("FSV effect exhaustion FULL:   id1=%d ok1=%v id2=%d ok2=%v %s", uint32(id1), ok1, uint32(id2), ok2, full)
	t.Logf("FSV effect exhaustion AFTER:  id3=%d ok3=%v %s", uint32(id3), ok3, after)

	if !ok1 || !ok2 || ok3 || id3 != 0 {
		t.Fatalf("spawn/exhaustion results wrong: ok1=%v ok2=%v ok3=%v id3=%d", ok1, ok2, ok3, uint32(id3))
	}
	if w.Effects.Count() != 2 || full != after {
		t.Fatalf("exhausted spawn mutated store: full=%s after=%s", full, after)
	}
}

func TestEffectDestroyInvalidStaleNoMutation(t *testing.T) {
	w := NewWorld(Caps{Units: 1, Projectiles: 1, Effects: 2, ScriptedDoodads: 1})
	id, ok := w.SpawnEffect(effectSpec(3, 100, 200, 1, 0xffffffff))
	if !ok {
		t.Fatal("SpawnEffect failed")
	}
	spawned := effectDump(w, id)
	okDestroy := w.DestroyEffect(id)
	destroyed := effectDump(w, id)
	aliveAfterDestroy := w.Ents.Alive(id)
	effectRowAfterDestroy := w.Effects.Row(id)
	transformRowAfterDestroy := w.Transforms.Row(id)
	emptyBefore := effectRows(w)
	okStale := w.DestroyEffect(id)
	okZero := w.DestroyEffect(0)
	unit, unitOK := w.CreateUnit(fixed.Vec2{X: eff(5), Y: eff(5)}, 0)
	okUnit := w.DestroyEffect(unit)
	emptyAfter := effectRows(w)

	t.Logf("FSV effect destroy SPAWNED:   %s", spawned)
	t.Logf("FSV effect destroy DESTROYED: ok=%v %s", okDestroy, destroyed)
	t.Logf("FSV effect destroy INVALID:   stale=%v zero=%v unitOK=%v unit=%d unitDestroy=%v before=%s after=%s",
		okStale, okZero, unitOK, uint32(unit), okUnit, emptyBefore, emptyAfter)

	if !okDestroy || aliveAfterDestroy || effectRowAfterDestroy != -1 || transformRowAfterDestroy != -1 {
		t.Fatalf("DestroyEffect did not remove effect entity cleanly: %s", destroyed)
	}
	if okStale || okZero || okUnit || emptyBefore != emptyAfter {
		t.Fatalf("invalid/stale DestroyEffect mutated state: before=%s after=%s", emptyBefore, emptyAfter)
	}
	if !unitOK || !w.Ents.Alive(unit) {
		t.Fatalf("DestroyEffect on a unit should leave the unit alive: unitOK=%v alive=%v", unitOK, w.Ents.Alive(unit))
	}
}

func TestEffectAppearsInSnapshotEntries(t *testing.T) {
	w := NewWorld(Caps{Units: 1, Projectiles: 1, Effects: 4, ScriptedDoodads: 1})
	id, ok := w.SpawnEffect(effectSpec(11, 12, 34, 2, 0x01020304))
	if !ok {
		t.Fatal("SpawnEffect failed")
	}
	before := effectDump(w, id)
	w.Step()
	snap := w.Snaps.Curr()
	entry, found := findSnapshotEntry(snap, id)
	t.Logf("FSV effect snapshot STORE: %s", before)
	t.Logf("FSV effect snapshot SNAP:  tick=%d published=%d found=%v entry={id=%d pos=(%d,%d) life=%d flags=%02x entries=%d}",
		snap.Tick, w.Snaps.Published(), found, uint32(entry.ID), entry.Pos.X.Floor(), entry.Pos.Y.Floor(),
		entry.LifeFrac, entry.Flags, len(snap.Entries))

	if !found {
		t.Fatalf("effect id %d absent from snapshot entries", uint32(id))
	}
	if entry.Pos != (fixed.Vec2{X: eff(12), Y: eff(34)}) || entry.LifeFrac != 65535 {
		t.Fatalf("snapshot entry wrong: %+v", entry)
	}
	if entry.Flags&SnapNoLerp == 0 {
		t.Fatalf("spawned effect snapshot should be no-lerp: flags=%02x", entry.Flags)
	}
}

func TestEffectMutatorsStoreDeltas(t *testing.T) {
	w := NewWorld(Caps{Units: 1, Projectiles: 1, Effects: 4, ScriptedDoodads: 1})
	id, ok := w.SpawnEffect(effectSpec(13, 10, 20, 2, 0x11223344))
	if !ok {
		t.Fatal("SpawnEffect failed")
	}
	before := effectDump(w, id)
	okScale := w.SetEffectScale(id, eff(3))
	okColor := w.SetEffectColor(id, 0xaabbccdd)
	okPos := w.SetEffectPos(id, fixed.Vec2{X: eff(30), Y: eff(40)})
	after := effectDump(w, id)

	t.Logf("FSV effect mutators BEFORE: %s", before)
	t.Logf("FSV effect mutators AFTER:  scaleOK=%v colorOK=%v posOK=%v %s", okScale, okColor, okPos, after)

	r := w.Effects.Row(id)
	tr := w.Transforms.Row(id)
	if !okScale || !okColor || !okPos || r == -1 || tr == -1 {
		t.Fatalf("effect mutator returned false or lost rows: %s", after)
	}
	if w.Effects.Scale[r] != eff(3) || w.Effects.ColorRGBA[r] != 0xaabbccdd {
		t.Fatalf("effect store delta wrong: %s", after)
	}
	if w.Transforms.Pos[tr] != (fixed.Vec2{X: eff(30), Y: eff(40)}) {
		t.Fatalf("effect position delta wrong: %s", after)
	}
}

func TestEffectMutatorsZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{Units: 1, Projectiles: 1, Effects: 4, ScriptedDoodads: 1})
	id, ok := w.SpawnEffect(effectSpec(15, 1, 2, 1, 0xffffffff))
	if !ok {
		t.Fatal("SpawnEffect failed")
	}
	if n := testing.AllocsPerRun(1000, func() {
		w.SetEffectScale(id, eff(4))
		w.SetEffectColor(id, 0x01020304)
		w.SetEffectPos(id, fixed.Vec2{X: eff(5), Y: eff(6)})
	}); n != 0 {
		t.Fatalf("effect mutators allocate %.1f/run, want 0", n)
	}
	t.Logf("FSV effect mutators zero-alloc verified: %s", effectDump(w, id))
}

func TestEffectSaveRoundTripBytes(t *testing.T) {
	src := populateEffectSaveWorld(t)
	beforeRows := effectRows(src)
	before := effectSnapshot(t, src)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	effOff := effectSectionOffsetForTest(src, saved)
	effLen := effectSectionLenForTest(src)

	dst := NewWorld(effectSaveCaps())
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatal(err)
	}
	after := effectSnapshot(t, dst)
	afterRows := effectRows(dst)
	effIdx := hashSystemIndex(t, "effects")

	t.Logf("FSV effect save SOURCE: %s top=%016x effects=%016x", beforeRows, before.Top, before.Subs[effIdx])
	t.Logf("FSV effect section offset=%d len=%d hex=% x", effOff, effLen, saved[effOff:effOff+effLen])
	t.Logf("FSV effect save LOADED: %s top=%016x effects=%016x", afterRows, after.Top, after.Subs[effIdx])

	if before.Top != after.Top {
		t.Fatalf("effect round-trip hash mismatch; systems=%v", snapDiff(t, &before, &after))
	}
	if beforeRows != afterRows {
		t.Fatalf("effect rows changed:\nbefore %s\nafter  %s", beforeRows, afterRows)
	}
	assertEffectRowOf(t, dst)

	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, 0); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("effect round-trip re-save is not byte-identical")
	}
}

func TestHashEffectDivergence(t *testing.T) {
	reg := NewHashRegistry()
	a := NewWorld(effectSaveCaps())
	b := NewWorld(effectSaveCaps())
	idA, okA := a.SpawnEffect(effectSpec(31, 1, 2, 1, 0x01020304))
	idB, okB := b.SpawnEffect(effectSpec(31, 1, 2, 1, 0x01020304))
	if !okA || !okB || uint32(idA) != uint32(idB) {
		t.Fatalf("failed to build identical effect worlds: idA=%d okA=%v idB=%d okB=%v", idA, okA, idB, okB)
	}
	if !b.SetEffectScale(idB, eff(5)) {
		t.Fatal("SetEffectScale failed")
	}
	var same, diff statehash.Snapshot
	a.HashState(reg, &same)
	b.HashState(reg, &diff)
	culprit, ok := reg.FirstDivergence(&same, &diff)
	effIdx := hashSystemIndex(t, "effects")
	t.Logf("FSV effect hash base: %s top=%016x effects=%016x", effectRows(a), same.Top, same.Subs[effIdx])
	t.Logf("FSV effect hash changed: %s top=%016x effects=%016x", effectRows(b), diff.Top, diff.Subs[effIdx])
	if !ok || culprit != "effects" {
		t.Fatalf("first divergence=%q ok=%v, want effects", culprit, ok)
	}
}

func TestEffectSaveTruncatedSectionRefusesAndDoesNotMutate(t *testing.T) {
	src := populateEffectSaveWorld(t)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	full := buf.Bytes()
	effOff := effectSectionOffsetForTest(src, full)
	effLen := effectSectionLenForTest(src)

	dst := NewWorld(effectSaveCaps())
	if _, ok := dst.SpawnEffect(effectSpec(44, 101, 202, 4, 0x55667788)); !ok {
		t.Fatal("target SpawnEffect failed")
	}
	beforeRows := effectRows(dst)
	before := effectSnapshot(t, dst)
	err := dst.LoadState(bytes.NewReader(full[:effOff+10]), 0)
	if err == nil {
		t.Fatal("truncated effect section accepted")
	}
	after := effectSnapshot(t, dst)
	afterRows := effectRows(dst)
	t.Logf("FSV truncated effects source section offset=%d len=%d hex=% x", effOff, effLen, full[effOff:effOff+effLen])
	t.Logf("FSV truncated effects target BEFORE: %s hash=%016x", beforeRows, before.Top)
	t.Logf("FSV truncated effects target AFTER:  %s hash=%016x err=%v", afterRows, after.Top, err)
	if before.Top != after.Top || beforeRows != afterRows {
		t.Fatal("failed effect load mutated the target world")
	}
}

func TestEffectSaveSpawnDestroySpawnReuseOrder(t *testing.T) {
	caps := Caps{Units: 1, Projectiles: 1, Effects: 3, ScriptedDoodads: 1}
	src := NewWorld(caps)
	a, okA := src.SpawnEffect(effectSpec(51, 1, 1, 1, 0x11111111))
	b, okB := src.SpawnEffect(effectSpec(52, 2, 2, 2, 0x22222222))
	initial := effectRows(src)
	destroyed := src.DestroyEffect(a)
	afterDestroy := effectRows(src)
	c, okC := src.SpawnEffect(effectSpec(53, 3, 3, 3, 0x33333333))
	beforeRows := effectRows(src)
	beforeFree := entityFreeChain(src, 8)
	if !okA || !okB || !destroyed || !okC || a.Index() != c.Index() || a.Generation() == c.Generation() {
		t.Fatalf("reuse fixture invalid: a=%d okA=%v b=%d okB=%v destroyed=%v c=%d okC=%v", a, okA, b, okB, destroyed, c, okC)
	}

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	dst := NewWorld(caps)
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatal(err)
	}
	afterRows := effectRows(dst)
	afterFree := entityFreeChain(dst, 8)
	assertEffectRowOf(t, dst)
	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, 0); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("effect reuse pattern re-save is not byte-identical")
	}
	nextSrc, okSrc := src.SpawnEffect(effectSpec(54, 4, 4, 4, 0x44444444))
	nextDst, okDst := dst.SpawnEffect(effectSpec(54, 4, 4, 4, 0x44444444))
	t.Logf("FSV effect reuse INITIAL:       %s", initial)
	t.Logf("FSV effect reuse AFTER DESTROY: ok=%v %s", destroyed, afterDestroy)
	t.Logf("FSV effect reuse BEFORE SAVE:   %s free=%s", beforeRows, beforeFree)
	t.Logf("FSV effect reuse AFTER LOAD:    %s free=%s", afterRows, afterFree)
	t.Logf("FSV effect reuse NEXT SPAWN:    src=%d ok=%v dst=%d ok=%v", nextSrc, okSrc, nextDst, okDst)
	if beforeRows != afterRows || beforeFree != afterFree {
		t.Fatalf("effect row/free order changed:\nrows before %s\nrows after  %s\nfree before %s\nfree after  %s", beforeRows, afterRows, beforeFree, afterFree)
	}
	if !okSrc || !okDst || nextSrc != nextDst {
		t.Fatalf("entity free-list order not preserved: src=%d ok=%v dst=%d ok=%v", nextSrc, okSrc, nextDst, okDst)
	}
}
