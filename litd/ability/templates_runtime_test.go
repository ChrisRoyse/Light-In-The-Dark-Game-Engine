package ability_test

// #602 — abilities FSV suite: every shipped template CASTS at runtime (its
// primitives instantiate and act), the registry is serializable/hashable across
// save+load, and a mismatched template file is rejected at the join fingerprint.
// SoT = the live sim state after a cast (mover count / victim Life) and the
// load fingerprint gate. (abilitycheck-green is gated separately in
// tools/abilitycheck and scripts/preflight.sh.)

import (
	"bytes"
	"os"
	"path/filepath"
	"sync"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ability"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

const specsDir = "../../docs/prd2/06-ability-composition/templates/specs"

var execOnce sync.Once

// registerDamageExec installs a synthetic 40-damage exec for EPDamage once per
// test binary (the registry is global, frozen-after-first-use semantics).
func registerDamageExec() {
	execOnce.Do(func() {
		sim.RegisterEffectExec(data.EPDamage, func(w *sim.World, ctx sim.EffectCtx, e *data.CompiledEffect) {
			w.QueueDamage(sim.DamagePacket{Source: ctx.Source, Target: ctx.Target, Amount: 40 * fixed.One})
		})
	})
}

// templateWorld builds a world, binds the synthetic damage arena + matrix,
// loads the named template, registers its declared effect lists + events, and
// registers the ability. Returns the world and the ability's spec index.
func templateWorld(t *testing.T, file string) (*sim.World, uint16) {
	t.Helper()
	registerDamageExec()
	blob, err := os.ReadFile(filepath.Join(specsDir, file))
	if err != nil {
		t.Fatalf("read %s: %v", file, err)
	}
	tpl, err := ability.LoadTOML(blob)
	if err != nil {
		t.Fatalf("load %s: %v", file, err)
	}
	w := sim.NewWorld(sim.Caps{Units: 32, Movers: 16})
	if err := w.BindEffects([]data.CompiledEffect{{Prim: data.EPDamage}}); err != nil {
		t.Fatal(err)
	}
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatal(err)
	}
	for _, name := range tpl.EffectLists {
		w.RegisterEffectListName(name, sim.EffectListSpan(0, 1))
	}
	for _, ev := range tpl.RefEvents {
		w.CustomEvents.RegisterEventKind(ev)
	}
	if _, err := registerLowered(w, tpl.Source); err != nil {
		t.Fatalf("register %s: %v", file, err)
	}
	idx, ok := w.AbilityDefs.Lookup(tpl.Source.ID)
	if !ok {
		t.Fatalf("ability %q not found after registration", tpl.Source.ID)
	}
	return w, idx
}

func mkCaster(w *sim.World) sim.EntityID {
	c, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Owners.Add(w.Ents, c, 1, 1, 1)
	w.Healths.Add(w.Ents, c, 100*fixed.One, 0, 0, 0)
	return c
}

func mkEnemy(w *sim.World, x int64) sim.EntityID {
	e, _ := w.CreateUnit(fixed.Vec2{X: fixed.F64(x) * fixed.One}, 0)
	w.Owners.Add(w.Ents, e, 2, 2, 2)
	w.Healths.Add(w.Ents, e, 1000*fixed.One, 0, 0, 0)
	return e
}

func life(w *sim.World, e sim.EntityID) fixed.F64 { return w.Healths.Life[w.Healths.Row(e)] }

// TestTemplatesCastRuntime casts each template via the interpreter and confirms
// its headline primitive instantiates/acts.
func TestTemplatesCastRuntime(t *testing.T) {
	cases := []struct {
		file        string
		wantMover   bool // attach_mover templates: a live mover after cast
		wantAoEStep int  // group/timer templates: enemy damaged after N steps (0 = skip)
	}{
		{"homing-bolt.toml", true, 0},
		{"orbital-guardian.toml", true, 0},
		{"chain-bounce.toml", true, 0},
		{"boomerang.toml", true, 0},
		{"nova-ring.toml", false, 1},
		{"persistent-field.toml", false, 12},
	}
	for _, c := range cases {
		w, idx := templateWorld(t, c.file)
		caster := mkCaster(w)
		enemy := mkEnemy(w, 100)
		_ = enemy
		before := w.Movers.Count()
		if !w.CastAbility(idx, caster, enemy, fixed.Vec2{X: 100 * fixed.One}) {
			t.Fatalf("%s: CastAbility returned false", c.file)
		}
		if c.wantMover {
			if w.Movers.Count() <= before {
				t.Fatalf("%s: no mover instantiated (count %d->%d)", c.file, before, w.Movers.Count())
			}
			t.Logf("%-22s cast → movers=%d", c.file, w.Movers.Count())
		}
		if c.wantAoEStep > 0 {
			for i := 0; i < c.wantAoEStep; i++ {
				w.Step()
			}
			dmg := 1000*fixed.One - life(w, enemy)
			if dmg < 40*fixed.One {
				t.Fatalf("%s: AoE enemy damage=%d, want >=40 after %d steps", c.file, int64(dmg), c.wantAoEStep)
			}
			t.Logf("%-22s cast → enemy damage=%d after %d steps", c.file, int64(dmg), c.wantAoEStep)
		}
	}
}

// TestTemplatesSaveLoadRoundTrip registers every template, casts one, then
// save→fresh-world→re-register→load and confirms the fingerprint gate accepts
// the matching peer (serializable/hashable contract).
func TestTemplatesSaveLoadRoundTrip(t *testing.T) {
	registerDamageExec()
	files := []string{"homing-bolt.toml", "orbital-guardian.toml", "nova-ring.toml",
		"chain-bounce.toml", "boomerang.toml", "persistent-field.toml"}

	build := func() *sim.World {
		w := sim.NewWorld(sim.Caps{Units: 32, Movers: 16})
		if err := w.BindEffects([]data.CompiledEffect{{Prim: data.EPDamage}}); err != nil {
			t.Fatal(err)
		}
		w.BindDamageMatrix([][]int32{{1000}})
		w.SetDataFingerprint(0xF1A3)
		for _, f := range files {
			blob, _ := os.ReadFile(filepath.Join(specsDir, f))
			tpl, err := ability.LoadTOML(blob)
			if err != nil {
				t.Fatalf("%s: %v", f, err)
			}
			for _, n := range tpl.EffectLists {
				w.RegisterEffectListName(n, sim.EffectListSpan(0, 1))
			}
			for _, ev := range tpl.RefEvents {
				w.CustomEvents.RegisterEventKind(ev)
			}
			if _, err := registerLowered(w, tpl.Source); err != nil {
				t.Fatalf("register %s: %v", f, err)
			}
		}
		return w
	}

	src := build()
	fp := src.JoinFingerprint()
	mkCaster(src)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	t.Logf("save: %d bytes, JoinFingerprint=%016x", buf.Len(), fp)

	dst := build() // identical registration order → identical fingerprint
	if dst.JoinFingerprint() != fp {
		t.Fatalf("rebuilt world fingerprint %016x != %016x", dst.JoinFingerprint(), fp)
	}
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("matching peer rejected the save: %v", err)
	}
	t.Logf("matching peer loaded OK at fingerprint %016x", fp)
}

// TestTemplateFingerprintReject: a peer whose homing-bolt was retuned (one
// number) computes a different join fingerprint and refuses the save.
func TestTemplateFingerprintReject(t *testing.T) {
	registerDamageExec()
	load := func(tweak bool) *sim.World {
		blob, _ := os.ReadFile(filepath.Join(specsDir, "homing-bolt.toml"))
		tpl, err := ability.LoadTOML(blob)
		if err != nil {
			t.Fatal(err)
		}
		if tweak {
			tpl.Source.Cooldown += 0.5 // one retuned number
		}
		w := sim.NewWorld(sim.Caps{Units: 8})
		if err := w.BindEffects([]data.CompiledEffect{{Prim: data.EPDamage}}); err != nil {
			t.Fatal(err)
		}
		w.SetDataFingerprint(0xF1A3)
		for _, n := range tpl.EffectLists {
			w.RegisterEffectListName(n, sim.EffectListSpan(0, 1))
		}
		if _, err := registerLowered(w, tpl.Source); err != nil {
			t.Fatal(err)
		}
		return w
	}
	a := load(false)
	b := load(true)
	fpA, fpB := a.JoinFingerprint(), b.JoinFingerprint()
	t.Logf("peerA=%016x peerB(retuned)=%016x", fpA, fpB)
	if fpA == fpB {
		t.Fatal("retuned template shares a join fingerprint (would desync)")
	}
	var buf bytes.Buffer
	if err := a.SaveState(&buf, fpA); err != nil {
		t.Fatal(err)
	}
	bFresh := load(true)
	if err := bFresh.LoadState(bytes.NewReader(buf.Bytes()), fpB); err == nil {
		t.Fatal("retuned peer accepted a save from the original — silent desync")
	} else {
		t.Logf("retuned peer rejected the save: %v", err)
	}
}

// registerLowered lowers a float source (litd/data, #628) then registers it —
// the sim API now takes the fixed-point lowered form.
func registerLowered(w *sim.World, src data.AbilitySpecSource) (uint16, error) {
	lo, err := data.LowerAbilitySpec(src)
	if err != nil {
		return 0, err
	}
	return w.RegisterAbilitySpecAuto(lo)
}
