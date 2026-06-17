package litd

// FSV for the handle marshaling seam (#267 step 1). SoT = the HandleRef bytes
// (opaque, plain types — no sim/bytecode) and the resolved handle's
// generation-checked Valid() across a save/load round-trip and an entity
// recycle. This is the codec #264's userdata->handle rebind resolves through.

import (
	"encoding/json"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestHandleRefRoundTripAndStaleness(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)

	var face fixed.Angle
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, face)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	u := Unit{id: id, g: g}

	// RefOf -> JSON round-trip (the save path) -> Resolve.
	ref, ok := RefOf(u)
	if !ok || ref.Kind != HandleUnit || ref.Raw != uint32(id) {
		t.Fatalf("RefOf(unit) = %+v ok=%v, want {Unit, %#x}", ref, ok, uint32(id))
	}
	blob, _ := json.Marshal(ref)
	t.Logf("FSV ref artifact (plain, opaque): %s", blob)
	var ref2 HandleRef
	if err := json.Unmarshal(blob, &ref2); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}

	h, ok := g.Resolve(ref2)
	if !ok {
		t.Fatal("Resolve returned ok=false for a Unit ref")
	}
	t.Logf("FSV live: Ents.Alive=%v resolved.Valid=%v", w.Ents.Alive(id), h.Valid())
	if !h.Valid() {
		t.Fatalf("resolved handle should be Valid while the entity lives")
	}

	// Recycle the slot: destroy the entity. The SAME ref must now resolve stale.
	if !w.Ents.Destroy(id) {
		t.Fatal("Destroy returned false on a live entity")
	}
	h2, _ := g.Resolve(ref2)
	t.Logf("FSV after destroy: Ents.Alive=%v resolved.Valid=%v", w.Ents.Alive(id), h2.Valid())
	if h2.Valid() {
		t.Fatalf("ref to a destroyed entity must resolve stale (Valid()=false)")
	}

	// Edge: a non-entity-backed handle (Camera) is not marshalable via this seam.
	if _, ok := RefOf(Camera{id: 1, g: g}); ok {
		t.Fatal("RefOf(Camera) should report ok=false (index-based, not entity-backed)")
	}
	// Edge: the null ref resolves to a zero handle (Valid()=false), not ok=false-for-Unit.
	if hz, ok := g.Resolve(HandleRef{}); ok {
		t.Fatalf("Resolve(null ref) should be ok=false, got %v", hz)
	}
	t.Logf("FSV: seam round-trips opaque refs, staleness honored, non-entity + null rejected")
}
