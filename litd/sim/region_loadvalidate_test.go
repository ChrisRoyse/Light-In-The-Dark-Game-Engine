package sim

import (
	"bytes"
	"testing"
)

// Regression for #632 (#630-class): RegionStore.NewRegion pops a free-list id and
// indexes s.entries[id] raw, so a corrupt save free-list value out-of-range
// panics on the next create and one pointing at a live slot revives/corrupts a
// live region. validateSave must reject such a save (fail-closed). Decode-stage a
// real save, corrupt only the region free-list, then call validateSave directly.
func TestRegionLoadValidatesFreeListFSV(t *testing.T) {
	w := NewWorld(Caps{})
	r0, _ := w.Regions.NewRegion()  // slot 0, live
	r1, g1 := w.Regions.NewRegion() // slot 1
	r2, _ := w.Regions.NewRegion()  // slot 2, live
	w.Regions.Remove(r1, g1)        // slot 1 dead → free=[1], alive: 0,2
	_ = r0
	_ = r2

	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatalf("save: %v", err)
	}
	blob := buf.Bytes()
	fresh := func() *World { return NewWorld(Caps{}) }

	// Baseline: the un-corrupted decode passes (no false rejection).
	if d := decodeStaged(t, fresh(), blob); validateSave(d, fresh()) != nil {
		t.Fatalf("baseline valid region save rejected: %v", validateSave(d, fresh()))
	}
	t.Logf("BEFORE: baseline region save (3 slots, 1 dead) validates clean")

	cases := []struct {
		name   string
		mutate func(d *decodedSave)
	}{
		{"free id out of range", func(d *decodedSave) { d.regFree[0] = uint32(len(d.regEntries) + 5) }},
		{"free id points at a live slot", func(d *decodedSave) { d.regFree[0] = 0 }}, // slot 0 is alive
		{"free id duplicated", func(d *decodedSave) { d.regFree = append(d.regFree, d.regFree[0]) }},
		{"dead slot missing from free list", func(d *decodedSave) { d.regFree = d.regFree[:0] }},
	}
	for _, c := range cases {
		d := decodeStaged(t, fresh(), blob)
		c.mutate(d)
		err := validateSave(d, fresh())
		t.Logf("AFTER mutate %q -> validateSave err=%v", c.name, err)
		if err == nil {
			t.Fatalf("corrupt region save %q passed validateSave (fail-OPEN); NewRegion would panic/corrupt (#632)", c.name)
		}
	}
}
