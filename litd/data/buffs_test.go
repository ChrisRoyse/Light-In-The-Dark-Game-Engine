package data

import (
	"strings"
	"testing"
	"testing/fstest"
)

const testBuffsTOML = `
[[buff]]
id = "poison"
name = "Poison"
duration = 2.0
stacking = "stack-count"
max-stacks = 5
period = 0.5
dispellable = true

[[buff.effects]]
prim = "damage"
amount = 4
attack-type = "normal"

[[buff]]
id = "slow"
name = "Slow"
duration = 1.0
stacking = "refresh"

[[buff.mod]]
stat = "move-speed"
permille = 500

[[buff.mod]]
stat = "armor"
add = -3
`

const applyBuffAbilityTOML = `
[[ability]]
id = "envenom"
name = "Envenom"

[[ability.effects]]
prim = "apply-buff"
buff = "poison"
stacks = 2

[[ability.effects]]
prim = "apply-buff"
buff = "slow"
`

// buffFS builds a layout with a buffs table plus an ability that
// applies them by name.
func buffFS(buffs, abilities string) fstest.MapFS {
	fsys := effFS(abilities)
	fsys["buffs/core.toml"] = &fstest.MapFile{Data: []byte(buffs)}
	return fsys
}

// Happy path: rows convert to sim units (seconds → ticks, per-second
// speed → per-tick fixed, names → indices), the periodic composition
// compiles into the shared arena, and apply-buff resolves names to
// sorted-table indices with schema-order params.
func TestBuffLoadHappy(t *testing.T) {
	tb, err := Load(buffFS(testBuffsTOML, applyBuffAbilityTOML))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if len(tb.BuffTypes) != 2 {
		t.Fatalf("got %d buff types, want 2", len(tb.BuffTypes))
	}
	// sorted by ID: poison < slow
	po, sl := &tb.BuffTypes[0], &tb.BuffTypes[1]
	if po.ID != "poison" || sl.ID != "slow" {
		t.Fatalf("sort order: %q, %q", po.ID, sl.ID)
	}
	if po.DurationTicks != 40 || po.Stacking != StackCount || po.MaxStacks != 5 ||
		po.PeriodTicks != 10 || po.Flags&BuffDispellable == 0 {
		t.Errorf("poison row = %+v", *po)
	}
	if po.Periodic.Len != 1 || tb.Effects[po.Periodic.Off].Prim != EPDamage ||
		tb.Effects[po.Periodic.Off].Params[0] != 4 {
		t.Errorf("poison periodic = %+v / %+v", po.Periodic, tb.Effects[po.Periodic.Off])
	}
	if sl.DurationTicks != 20 || sl.Stacking != StackRefresh || sl.MaxStacks != 1 ||
		sl.PeriodTicks != 0 || sl.Periodic.Len != 0 || sl.Flags != 0 {
		t.Errorf("slow row = %+v", *sl)
	}
	if len(sl.Mods) != 2 {
		t.Fatalf("slow mods = %+v", sl.Mods)
	}
	if m := sl.Mods[0]; m.Stat != StatMoveSpeed || m.Add != 0 || m.Permille != 500 {
		t.Errorf("slow mod[0] = %+v", m)
	}
	if m := sl.Mods[1]; m.Stat != StatArmor || m.Add != -3 || m.Permille != 1000 {
		t.Errorf("slow mod[1] = %+v", m)
	}
	// apply-buff invocations: buff = table index, stacks default 1
	var env *Ability
	for i := range tb.Abilities {
		if tb.Abilities[i].ID == "envenom" {
			env = &tb.Abilities[i]
		}
	}
	if env == nil || env.Effects.Len != 2 {
		t.Fatalf("envenom = %+v", env)
	}
	e0 := tb.Effects[env.Effects.Off]
	e1 := tb.Effects[env.Effects.Off+1]
	if e0.Prim != EPApplyBuff || e0.Params[0] != 0 || e0.Params[1] != 2 {
		t.Errorf("apply-buff[poison] params = %+v", e0.Params)
	}
	if e1.Prim != EPApplyBuff || e1.Params[0] != 1 || e1.Params[1] != 1 {
		t.Errorf("apply-buff[slow] params = %+v (want index 1, default stacks 1)", e1.Params)
	}
	t.Logf("poison: %+v", *po)
	t.Logf("slow:   %+v", *sl)
	t.Logf("envenom apply-buff params: %v / %v", e0.Params[:2], e1.Params[:2])
}

// Fail-closed rejections: each malformed row is a LOAD error naming
// the field, never a defaulted row.
func TestBuffLoadRejections(t *testing.T) {
	cases := []struct {
		name, buffs, wantErr string
	}{
		{"unknown stacking", strings.Replace(testBuffsTOML, `stacking = "refresh"`, `stacking = "stack"`, 1), "stacking"},
		{"zero duration", strings.Replace(testBuffsTOML, "duration = 1.0", "duration = 0", 1), "duration"},
		{"unknown stat", strings.Replace(testBuffsTOML, `stat = "armor"`, `stat = "not-a-real-stat"`, 1), "mod.stat"},
		{"permille out of range", strings.Replace(testBuffsTOML, "permille = 500", "permille = 10001", 1), "mod.permille"},
		{"fractional armor add", strings.Replace(testBuffsTOML, "add = -3", "add = -3.5", 1), "mod.add"},
		{"max-stacks out of range", strings.Replace(testBuffsTOML, "max-stacks = 5", "max-stacks = 256", 1), "max-stacks"},
		{"duplicate id", testBuffsTOML + "\n[[buff]]\nid = \"slow\"\nduration = 1.0\nstacking = \"refresh\"\n", "duplicate buff id"},
		{"unknown field", strings.Replace(testBuffsTOML, "dispellable = true", "dispelable = true", 1), "unknown field"},
	}
	for _, c := range cases {
		_, err := Load(buffFS(c.buffs, applyBuffAbilityTOML))
		if err == nil || !strings.Contains(err.Error(), c.wantErr) {
			t.Errorf("%s: err = %v, want containing %q", c.name, err, c.wantErr)
		} else {
			t.Logf("%s: %v", c.name, err)
		}
	}
}

// apply-buff with an unresolvable name fails at load — the buff table
// is the closed reference set.
func TestApplyBuffUnknownName(t *testing.T) {
	bad := strings.Replace(applyBuffAbilityTOML, `buff = "slow"`, `buff = "haste"`, 1)
	_, err := Load(buffFS(testBuffsTOML, bad))
	if err == nil || !strings.Contains(err.Error(), `"haste" is not a defined buff`) {
		t.Fatalf("err = %v", err)
	}
	t.Logf("rejected: %v", err)
}

// A buff's periodic composition may apply-buff — including itself
// (names are collected before compiling).
func TestBuffPeriodicSelfReference(t *testing.T) {
	self := testBuffsTOML + `
[[buff]]
id = "spreading"
duration = 1.0
stacking = "independent"
period = 0.5

[[buff.effects]]
prim = "apply-buff"
buff = "spreading"
`
	tb, err := Load(buffFS(self, applyBuffAbilityTOML))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	var sp *BuffType
	for i := range tb.BuffTypes {
		if tb.BuffTypes[i].ID == "spreading" {
			sp = &tb.BuffTypes[i]
		}
	}
	idx := int64(-1)
	for i := range tb.BuffTypes {
		if tb.BuffTypes[i].ID == "spreading" {
			idx = int64(i)
		}
	}
	if sp == nil || sp.Periodic.Len != 1 || tb.Effects[sp.Periodic.Off].Params[0] != idx {
		t.Fatalf("self-ref compile: %+v", sp)
	}
	t.Logf("spreading periodic apply-buff resolves to own index %d", idx)
}

// Fingerprint sensitivity: any buff value change moves the hash; an
// untouched reload is bit-identical.
func TestBuffFingerprint(t *testing.T) {
	base, err := Load(buffFS(testBuffsTOML, applyBuffAbilityTOML))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	again, err := Load(buffFS(testBuffsTOML, applyBuffAbilityTOML))
	if err != nil {
		t.Fatalf("reload: %v", err)
	}
	if base.Fingerprint != again.Fingerprint {
		t.Fatalf("identical input, fingerprint %x != %x", base.Fingerprint, again.Fingerprint)
	}
	changed, err := Load(buffFS(strings.Replace(testBuffsTOML, "permille = 500", "permille = 501", 1), applyBuffAbilityTOML))
	if err != nil {
		t.Fatalf("load changed: %v", err)
	}
	if changed.Fingerprint == base.Fingerprint {
		t.Fatal("permille change did not move the fingerprint")
	}
	t.Logf("base %x, reload %x, permille+1 %x", base.Fingerprint, again.Fingerprint, changed.Fingerprint)
}

const auraBuffsTOML = testBuffsTOML + `
[[buff]]
id = "command"
duration = 100.0
stacking = "refresh"
[buff.aura]
radius = 200.0
child = "slow"
linger = 1.0
`

// Aura block converts: radius to fixed, child to table index, linger
// to ticks — and every malformed block is a load error.
func TestBuffAuraBlock(t *testing.T) {
	tb, err := Load(buffFS(auraBuffsTOML, applyBuffAbilityTOML))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	var cmd *BuffType
	slowIdx := -1
	for i := range tb.BuffTypes {
		if tb.BuffTypes[i].ID == "command" {
			cmd = &tb.BuffTypes[i]
		}
		if tb.BuffTypes[i].ID == "slow" {
			slowIdx = i
		}
	}
	if cmd == nil || cmd.AuraRadius != 200<<32 || int(cmd.AuraChild) != slowIdx || cmd.AuraLingerTicks != 20 {
		t.Fatalf("command aura = %+v (slow at %d)", cmd, slowIdx)
	}
	t.Logf("command: radius=%d child=%d linger=%dt", int64(cmd.AuraRadius), cmd.AuraChild, cmd.AuraLingerTicks)

	cases := []struct{ name, buffs, wantErr string }{
		{"zero radius", strings.Replace(auraBuffsTOML, "radius = 200.0", "radius = 0", 1), "aura.radius"},
		{"unknown child", strings.Replace(auraBuffsTOML, `child = "slow"`, `child = "nothing"`, 1), "aura.child"},
		{"self child", strings.Replace(auraBuffsTOML, `child = "slow"`, `child = "command"`, 1), "aura.child"},
		{"linger below floor", strings.Replace(auraBuffsTOML, "linger = 1.0", "linger = 0.1", 1), "aura.linger"},
		{"unknown aura field", strings.Replace(auraBuffsTOML, "linger = 1.0", "linger = 1.0\nrange = 5", 1), "unknown field"},
	}
	for _, c := range cases {
		_, err := Load(buffFS(c.buffs, applyBuffAbilityTOML))
		if err == nil || !strings.Contains(err.Error(), c.wantErr) {
			t.Errorf("%s: err = %v, want containing %q", c.name, err, c.wantErr)
		} else {
			t.Logf("%s: %v", c.name, err)
		}
	}
}
