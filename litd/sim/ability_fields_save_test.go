package sim

import (
	"bytes"
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func TestAbilityFieldSaveRoundTripBytes(t *testing.T) {
	src, _, _ := abilityFieldSaveWorld(t)
	beforeRows := abilityFieldRowsDump(src)
	beforeFree := abilityFieldFreeDump(src)
	before := abilityFieldHash(t, src)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	afOff := abilityFieldSectionOffsetForTest(src, saved)
	afLen := abilityFieldSectionLenForTest(src)

	dst := abilityFieldLoadWorld(t)
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatal(err)
	}
	afterRows := abilityFieldRowsDump(dst)
	afterFree := abilityFieldFreeDump(dst)
	after := abilityFieldHash(t, dst)
	afIdx := hashSystemIndex(t, "abilityfields")
	t.Logf("FSV ability field save SOURCE: rows=%s free=%s top=%016x abilityfields=%016x",
		beforeRows, beforeFree, before.Top, before.Subs[afIdx])
	t.Logf("FSV ability field section offset=%d len=%d hex=% x", afOff, afLen, saved[afOff:afOff+afLen])
	t.Logf("FSV ability field save LOADED: rows=%s free=%s top=%016x abilityfields=%016x",
		afterRows, afterFree, after.Top, after.Subs[afIdx])

	if before.Top != after.Top {
		t.Fatalf("ability field round-trip hash mismatch; systems=%v", snapDiff(t, &before, &after))
	}
	if beforeRows != afterRows || beforeFree != afterFree {
		t.Fatalf("ability field rows/free changed:\nrows before %s\nrows after  %s\nfree before %s\nfree after  %s",
			beforeRows, afterRows, beforeFree, afterFree)
	}
	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, 0); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("ability field round-trip re-save is not byte-identical")
	}
}

func TestHashAbilityFieldDivergence(t *testing.T) {
	a, _, _ := abilityFieldSaveWorld(t)
	b, b0, _ := abilityFieldSaveWorld(t)
	beforeA := abilityFieldHash(t, a)
	beforeB := abilityFieldHash(t, b)
	if beforeA.Top != beforeB.Top {
		t.Fatalf("fixture worlds diverged before mutation: %016x vs %016x", beforeA.Top, beforeB.Top)
	}
	if !b.SetAbilityField(b0, 0, AbilityFieldCooldown, fixed.FromInt(26)) {
		t.Fatal("SetAbilityField divergence mutation failed")
	}
	afterB := abilityFieldHash(t, b)
	reg := NewHashRegistry()
	culprit, ok := reg.FirstDivergence(&beforeA, &afterB)
	afIdx := hashSystemIndex(t, "abilityfields")
	t.Logf("FSV ability field hash base: rows=%s top=%016x abilityfields=%016x", abilityFieldRowsDump(a), beforeA.Top, beforeA.Subs[afIdx])
	t.Logf("FSV ability field hash changed: rows=%s top=%016x abilityfields=%016x", abilityFieldRowsDump(b), afterB.Top, afterB.Subs[afIdx])
	if !ok || culprit != "abilityfields" {
		t.Fatalf("first divergence=%q ok=%v, want abilityfields", culprit, ok)
	}
}

func TestAbilityFieldSaveTruncatedSectionRefusesAndDoesNotMutate(t *testing.T) {
	src, _, _ := abilityFieldSaveWorld(t)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	full := buf.Bytes()
	afOff := abilityFieldSectionOffsetForTest(src, full)
	afLen := abilityFieldSectionLenForTest(src)

	dst, dst0, _ := abilityFieldSaveWorld(t)
	if !dst.SetAbilityField(dst0, 0, AbilityFieldCooldown, fixed.FromInt(99)) {
		t.Fatal("target mutation setup failed")
	}
	beforeRows := abilityFieldRowsDump(dst)
	beforeFree := abilityFieldFreeDump(dst)
	before := abilityFieldHash(t, dst)
	err := dst.LoadState(bytes.NewReader(full[:afOff+10]), 0)
	if err == nil {
		t.Fatal("truncated ability field section accepted")
	}
	afterRows := abilityFieldRowsDump(dst)
	afterFree := abilityFieldFreeDump(dst)
	after := abilityFieldHash(t, dst)
	t.Logf("FSV truncated abilityfields source section offset=%d len=%d hex=% x", afOff, afLen, full[afOff:afOff+afLen])
	t.Logf("FSV truncated abilityfields target BEFORE: rows=%s free=%s hash=%016x", beforeRows, beforeFree, before.Top)
	t.Logf("FSV truncated abilityfields target AFTER:  rows=%s free=%s hash=%016x err=%v", afterRows, afterFree, after.Top, err)
	if before.Top != after.Top || beforeRows != afterRows || beforeFree != afterFree {
		t.Fatal("failed ability field load mutated the target world")
	}
}

func TestAbilityFieldSaveSetClearSetFreeListOrder(t *testing.T) {
	src := abilityFieldLoadWorld(t)
	unit := addAbilityFieldUnit(t, src, 1)
	if !src.SetAbilityField(unit, 0, AbilityFieldCooldown, fixed.FromInt(1)) ||
		!src.SetAbilityField(unit, 0, AbilityFieldManaCost, fixed.FromInt(2)) {
		t.Fatal("initial override setup failed")
	}
	if !src.AbilityFields.Remove(unit, 0, AbilityFieldCooldown) {
		t.Fatal("override remove failed")
	}
	if !src.SetAbilityField(unit, 0, AbilityFieldRange, fixed.FromInt(3)) {
		t.Fatal("set after clear failed")
	}
	beforeRows := abilityFieldRowsDump(src)
	beforeFree := abilityFieldFreeDump(src)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	dst := abilityFieldLoadWorld(t)
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatal(err)
	}
	afterRows := abilityFieldRowsDump(dst)
	afterFree := abilityFieldFreeDump(dst)
	if !src.SetAbilityField(unit, 0, AbilityFieldDamage, fixed.FromInt(4)) ||
		!dst.SetAbilityField(unit, 0, AbilityFieldDamage, fixed.FromInt(4)) {
		t.Fatal("post-load next override setup failed")
	}
	nextSrc := abilityFieldRowsDump(src)
	nextDst := abilityFieldRowsDump(dst)
	t.Logf("FSV ability field free-list BEFORE SAVE: rows=%s free=%s", beforeRows, beforeFree)
	t.Logf("FSV ability field free-list AFTER LOAD:  rows=%s free=%s", afterRows, afterFree)
	t.Logf("FSV ability field free-list NEXT SET:    src=%s dst=%s", nextSrc, nextDst)
	if beforeRows != afterRows || beforeFree != afterFree {
		t.Fatalf("ability field row/free order changed:\nrows before %s\nrows after  %s\nfree before %s\nfree after  %s",
			beforeRows, afterRows, beforeFree, afterFree)
	}
	if nextSrc != nextDst {
		t.Fatalf("next allocation diverged after load:\nsrc %s\ndst %s", nextSrc, nextDst)
	}
}

func abilityFieldSaveWorld(t *testing.T) (*World, EntityID, EntityID) {
	t.Helper()
	w := abilityFieldLoadWorld(t)
	a := addAbilityFieldUnit(t, w, 2)
	b := addAbilityFieldUnit(t, w, 2)
	if !w.SetAbilityField(a, 0, AbilityFieldCooldown, fixed.FromInt(25)) ||
		!w.SetAbilityField(a, 0, AbilityFieldManaCost, fixed.FromInt(12)) ||
		!w.SetAbilityField(b, 1, AbilityFieldRange, fixed.FromInt(333)) {
		t.Fatal("ability field save fixture override setup failed")
	}
	return w, a, b
}

func abilityFieldLoadWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(abilityFieldTestCaps())
	bindAbilityFieldDefs(t, w)
	return w
}

func abilityFieldHash(t *testing.T, w *World) statehash.Snapshot {
	t.Helper()
	reg := NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	return snap
}

func abilityFieldFreeDump(w *World) string {
	var b strings.Builder
	fmt.Fprintf(&b, "freeLen=%d stack=[", len(w.AbilityFields.free))
	for i, f := range w.AbilityFields.free {
		if i > 0 {
			b.WriteByte(' ')
		}
		fmt.Fprintf(&b, "%d", f)
	}
	b.WriteByte(']')
	return b.String()
}
