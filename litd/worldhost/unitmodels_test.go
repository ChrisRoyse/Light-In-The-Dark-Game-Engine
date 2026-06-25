package worldhost_test

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

// Host.UnitModels is the type→art binding the render layer reads to attach a model
// per unit (the sim itself is model-agnostic). SoT = the loaded firstclash data: the
// host must surface each unit type's declared model path verbatim from the data
// tables, so a renderer can preload + attach without re-reading the world. Loading a
// world is headless (no GL), so this runs in the normal test gate.
func TestHostUnitModelsFSV(t *testing.T) {
	h, err := worldhost.Load(firstclashDir, 24680, 50_000_000)
	if err != nil {
		t.Fatalf("load firstclash: %v", err)
	}
	defer h.Close()

	if len(h.UnitModels) == 0 {
		t.Fatal("UnitModels empty — host did not retain the type→model binding")
	}

	// Spot-check declared paths from the data (worlds/firstclash/data/units/*.toml):
	// a Vigil building, a Vigil unit, and an Unbound unit. These are the exact
	// strings the renderer resolves against assets/.
	want := map[string]string{
		"bastion":    "buildings/vigil/bastion.glb",
		"oathsworn":  "units/vigil/oathsworn.glb",
		"forager":    "units/unbound/forager.glb",
		"footman":    "units/footman.glb",
		"fire_kraal": "buildings/unbound/fire_kraal.glb",
	}
	for code, model := range want {
		got, ok := h.UnitModels[code]
		if !ok {
			t.Errorf("code %q missing; available keys: %v", code, keysOf(h.UnitModels))
			continue
		}
		if got != model {
			t.Errorf("UnitModels[%q] = %q, want %q", code, got, model)
		} else {
			t.Logf("FSV: %q → %q (declared model surfaced verbatim)", code, got)
		}
	}

	// Every non-empty model path must be a unit-art path (units/… or buildings/…),
	// never an absolute or traversing path — the render resolver joins it under
	// assets/, so a bogus path would be a real defect.
	for code, m := range h.UnitModels {
		if m == "" {
			continue
		}
		if m[0] == '/' || hasDotDot(m) {
			t.Errorf("unit %q model %q is absolute/traversing — unsafe to resolve", code, m)
		}
	}
}

func keysOf(m map[string]string) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	return ks
}

func hasDotDot(s string) bool {
	for i := 0; i+1 < len(s); i++ {
		if s[i] == '.' && s[i+1] == '.' {
			return true
		}
	}
	return false
}
