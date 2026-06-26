package ability

// #600 FSV — the six shipped templates are real, valid composable specs. SoT =
// the loaded + compiled AbilitySpec read directly: every template loads, every
// reference resolves, and each one's compiled op tree matches its documented
// motion archetype. (The cast-each runtime FSV — state JSON / screenshot —
// lives with the demo integration in #601/#602.)

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

const templatesDir = "../../docs/prd2/06-ability-composition/templates/specs"

func loadTemplate(t *testing.T, name string) Template {
	t.Helper()
	blob, err := os.ReadFile(filepath.Join(templatesDir, name))
	if err != nil {
		t.Fatalf("read %s: %v", name, err)
	}
	tpl, err := LoadTOML(blob)
	if err != nil {
		t.Fatalf("load %s: %v", name, err)
	}
	return tpl
}

// firstOpKind returns the kind of the first op of a compiled spec.
func opKinds(spec sim.AbilitySpec) []sim.AbilityOpKind {
	out := make([]sim.AbilityOpKind, len(spec.OnCast))
	for i := range spec.OnCast {
		out[i] = spec.OnCast[i].Kind
	}
	return out
}

// TestAllTemplatesCompile loads + compiles every shipped template and prints the
// compiled SoT for inspection.
func TestAllTemplatesCompile(t *testing.T) {
	files := []string{
		"homing-bolt.toml", "orbital-guardian.toml", "nova-ring.toml",
		"chain-bounce.toml", "boomerang.toml", "persistent-field.toml",
	}
	for _, f := range files {
		tpl := loadTemplate(t, f)
		if refErrs := CheckTemplateRefs(tpl); len(refErrs) != 0 {
			t.Fatalf("%s ref errors: %v", f, refErrs)
		}
		spec, err := Compile(tpl)
		if err != nil {
			t.Fatalf("%s compile: %v", f, err)
		}
		t.Logf("%-22s id=%-18q ops=%d cooldown=%dt castRange=%d effects=%v",
			f, spec.ID, len(spec.OnCast), spec.Cooldown, int64(spec.CastRange), tpl.EffectLists)
	}
}

// TestTemplateArchetypes verifies each template's op tree matches the archetype
// the library documents — a structural SoT, not just "it compiled".
func TestTemplateArchetypes(t *testing.T) {
	// homing-bolt: spawn_projectile → attach_mover(homing)
	hb, _ := Compile(loadTemplate(t, "homing-bolt.toml"))
	if got := opKinds(hb); len(got) != 2 || got[0] != sim.OpSpawnProjectile || got[1] != sim.OpAttachMover {
		t.Fatalf("homing-bolt ops=%v, want [spawn_projectile attach_mover]", got)
	}
	if hb.OnCast[1].MoverKind != sim.MoverHoming {
		t.Fatalf("homing-bolt mover=%d, want homing", hb.OnCast[1].MoverKind)
	}
	if hb.OnCast[1].EffectList.Len == 0 {
		t.Fatal("homing-bolt fx.effects did not resolve to a payload")
	}
	t.Logf("homing-bolt: mover=homing pierce=%d payloadLen=%d", hb.OnCast[1].Pierce, hb.OnCast[1].EffectList.Len)

	// orbital-guardian: orbit_unit, high pierce (persistent sweeper)
	og, _ := Compile(loadTemplate(t, "orbital-guardian.toml"))
	if og.OnCast[1].MoverKind != sim.MoverOrbitUnit {
		t.Fatalf("orbital-guardian mover=%d, want orbit_unit", og.OnCast[1].MoverKind)
	}
	t.Logf("orbital-guardian: mover=orbit_unit radius=%d pierce=%d", int64(og.OnCast[1].Radius), og.OnCast[1].Pierce)

	// nova-ring: fill_group → for_each_in_group(run_effects)
	nr, _ := Compile(loadTemplate(t, "nova-ring.toml"))
	if got := opKinds(nr); len(got) != 2 || got[0] != sim.OpFillGroup || got[1] != sim.OpForEachInGroup {
		t.Fatalf("nova-ring ops=%v, want [fill_group for_each_in_group]", got)
	}
	if len(nr.OnCast[1].Children) != 1 || nr.OnCast[1].Children[0].Kind != sim.OpRunEffects {
		t.Fatal("nova-ring for_each body is not run_effects")
	}
	t.Logf("nova-ring: fill radius=%d → for_each → run_effects", int64(nr.OnCast[0].Radius))

	// chain-bounce: homing with pierce>1 (the chain)
	cb, _ := Compile(loadTemplate(t, "chain-bounce.toml"))
	if cb.OnCast[1].MoverKind != sim.MoverHoming || cb.OnCast[1].Pierce < 2 {
		t.Fatalf("chain-bounce mover=%d pierce=%d, want homing pierce>=2", cb.OnCast[1].MoverKind, cb.OnCast[1].Pierce)
	}
	t.Logf("chain-bounce: mover=homing pierce=%d", cb.OnCast[1].Pierce)

	// boomerang: spline
	bm, _ := Compile(loadTemplate(t, "boomerang.toml"))
	if bm.OnCast[1].MoverKind != sim.MoverSpline {
		t.Fatalf("boomerang mover=%d, want spline", bm.OnCast[1].MoverKind)
	}
	t.Logf("boomerang: mover=spline pierce=%d", bm.OnCast[1].Pierce)

	// persistent-field: times{count,arg} → fill_group → for_each → run_effects
	pf, _ := Compile(loadTemplate(t, "persistent-field.toml"))
	if pf.OnCast[0].Kind != sim.OpTimes || pf.OnCast[0].Count != 6 || pf.OnCast[0].Arg != 10 {
		t.Fatalf("persistent-field op0=%d count=%d arg=%d, want times 6 every 10t", pf.OnCast[0].Kind, pf.OnCast[0].Count, pf.OnCast[0].Arg)
	}
	if len(pf.OnCast[0].Children) != 2 || pf.OnCast[0].Children[0].Kind != sim.OpFillGroup {
		t.Fatal("persistent-field times body is not [fill_group for_each]")
	}
	t.Logf("persistent-field: times count=%d every %dt → fill_group → for_each", pf.OnCast[0].Count, pf.OnCast[0].Arg)
}
