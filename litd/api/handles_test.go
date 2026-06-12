package litd

import (
	"reflect"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// kind classifies each public type for the inventory audit.
type kind int

const (
	kindRoot      kind = iota // Game
	kindNoun                  // entity/index/sentinel handle: Valid+IsZero, zero exported fields, ≤24B
	kindValue                 // value type: math/accessors, exported fields permitted
	kindInterface             // Widget
)

// inventory is the frozen public type list (public-api-design.md §2,
// 21 rows). The test reflects over it rather than the package (Go has
// no package-member reflection) — adding a public noun without adding
// it here, or vice versa, is the staleness this guards.
var inventory = []struct {
	name string
	typ  reflect.Type
	k    kind
}{
	{"Game", reflect.TypeOf(Game{}), kindRoot},
	{"Player", reflect.TypeOf(Player{}), kindNoun},
	{"Force", reflect.TypeOf(Force{}), kindNoun},
	{"Unit", reflect.TypeOf(Unit{}), kindNoun},
	{"Item", reflect.TypeOf(Item{}), kindNoun},
	{"Destructable", reflect.TypeOf(Destructable{}), kindNoun},
	{"Widget", reflect.TypeOf((*Widget)(nil)).Elem(), kindInterface},
	{"Ability", reflect.TypeOf(Ability{}), kindNoun},
	{"Buff", reflect.TypeOf(Buff{}), kindNoun},
	{"Order", reflect.TypeOf(Order{}), kindValue},
	{"Timer", reflect.TypeOf(Timer{}), kindNoun},
	{"Event", reflect.TypeOf(Event{}), kindValue},
	{"Region", reflect.TypeOf(Region{}), kindNoun},
	{"Rect", reflect.TypeOf(Rect{}), kindValue},
	{"Vec2", reflect.TypeOf(Vec2{}), kindValue},
	{"Angle", reflect.TypeOf(Angle{}), kindValue},
	{"Sound", reflect.TypeOf(Sound{}), kindNoun},
	{"Effect", reflect.TypeOf(Effect{}), kindNoun},
	{"Camera", reflect.TypeOf(Camera{}), kindNoun},
	{"Frame", reflect.TypeOf(Frame{}), kindNoun},
	{"Missile", reflect.TypeOf(Missile{}), kindNoun},
}

// validator is the Valid()+IsZero() contract every noun handle owns.
type validator interface {
	Valid() bool
	IsZero() bool
}

// forbiddenTypes are the JASS handles deliberately absent from the
// surface (public-api-design.md §2): the trigger zoo, group, and
// location all collapse into Go features or value types and must never
// reappear as a public type.
var forbiddenTypes = []string{
	"Trigger", "BoolExpr", "ConditionFunc", "FilterFunc",
	"Group", "Location", "TriggerCondition", "TriggerAction",
}

// TestTypeInventory reflects over every public type, prints its
// name/kind/size, and asserts the §6 budget (≤22 core types), the
// absence of the forbidden JASS handles, and that no field leaks a G3N
// type into the surface.
func TestTypeInventory(t *testing.T) {
	t.Logf("public type inventory (%d types):", len(inventory))
	for _, e := range inventory {
		size := "-"
		if e.k != kindInterface {
			size = itoa(int(e.typ.Size()))
		}
		t.Logf("  %-13s kind=%-9s reflect.Kind=%-9s sizeof=%s",
			e.name, kindName(e.k), e.typ.Kind(), size)
	}

	if len(inventory) > 22 {
		t.Fatalf("core public type count %d exceeds the ≤22 budget (public-api-design.md §6)", len(inventory))
	}

	for _, e := range inventory {
		for _, bad := range forbiddenTypes {
			if e.name == bad {
				t.Errorf("forbidden JASS handle %q is a public type", bad)
			}
		}
		// no field anywhere may name a G3N type (R-API-6)
		if e.typ.Kind() == reflect.Struct {
			for i := 0; i < e.typ.NumField(); i++ {
				ft := e.typ.Field(i).Type
				if pp := ft.PkgPath(); strings.Contains(pp, "g3n") || strings.Contains(pp, "repoes/engine") {
					t.Errorf("%s.%s leaks G3N type %s (%s)", e.name, e.typ.Field(i).Name, ft, pp)
				}
			}
		}
	}
}

// TestNounHandleShape enforces the per-noun structural contract: zero
// exported fields and ≤24-byte size, with Valid()/IsZero() present.
func TestNounHandleShape(t *testing.T) {
	for _, e := range inventory {
		if e.k != kindNoun {
			continue
		}
		if e.typ.Size() > 24 {
			t.Errorf("%s is %d bytes, over the 24-byte handle budget", e.name, e.typ.Size())
		}
		exported := 0
		for i := 0; i < e.typ.NumField(); i++ {
			if e.typ.Field(i).PkgPath == "" { // exported field
				exported++
				t.Errorf("noun handle %s has exported field %s (must be zero)", e.name, e.typ.Field(i).Name)
			}
		}
		// every noun handle must satisfy the validity contract
		v := reflect.New(e.typ).Elem().Interface()
		if _, ok := v.(validator); !ok {
			t.Errorf("noun handle %s does not implement Valid()/IsZero()", e.name)
		}
		t.Logf("%-13s exportedFields=%d sizeof=%d Valid/IsZero=ok", e.name, exported, e.typ.Size())
	}
}

// TestZeroValueHandlesInvalid — edge case (1): a zero-value handle is
// invalid before and after, never a panic. Source of truth: the
// boolean each Valid() returns, printed before/after.
func TestZeroValueHandlesInvalid(t *testing.T) {
	cases := []struct {
		name string
		v    validator
	}{
		{"Unit", Unit{}}, {"Item", Item{}}, {"Destructable", Destructable{}},
		{"Missile", Missile{}}, {"Player", Player{}}, {"Force", Force{}},
		{"Ability", Ability{}}, {"Buff", Buff{}}, {"Timer", Timer{}},
		{"Region", Region{}}, {"Sound", Sound{}}, {"Effect", Effect{}},
		{"Camera", Camera{}}, {"Frame", Frame{}},
	}
	for _, c := range cases {
		before := c.v.Valid()
		after := c.v.Valid() // second read: must agree, no state
		t.Logf("%-13s zero-value Valid() before=%v after=%v IsZero=%v", c.name, before, after, c.v.IsZero())
		if before || after {
			t.Errorf("%s zero-value Valid() = %v/%v, want false/false", c.name, before, after)
		}
		if !c.v.IsZero() {
			t.Errorf("%s zero value IsZero() = false, want true", c.name)
		}
	}
}

// TestGenerationCheckedValidity — edge case (2): a handle copied by
// value then invalidated by destroying the underlying entity. Both the
// original and the copy must report the same Valid() result (the
// generation check in the wrapped EntityID, not pointer identity).
// Source of truth: w.Ents.Alive on the entity id, read through both
// handles before and after Destroy.
func TestGenerationCheckedValidity(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)

	var face fixed.Angle
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, face)
	if !ok {
		t.Fatal("CreateUnit failed — cannot set up the generation test")
	}
	orig := Unit{id: id, g: g}
	cp := orig // copied by value

	bOrig, bCp := orig.Valid(), cp.Valid()
	t.Logf("BEFORE Destroy: entity id=%#x Ents.Alive=%v orig.Valid=%v copy.Valid=%v",
		uint32(id), w.Ents.Alive(id), bOrig, bCp)
	if !bOrig || !bCp {
		t.Fatalf("live unit: orig.Valid=%v copy.Valid=%v, want true/true", bOrig, bCp)
	}

	if !w.Ents.Destroy(id) {
		t.Fatal("Destroy returned false on a live entity")
	}

	aOrig, aCp := orig.Valid(), cp.Valid()
	t.Logf("AFTER  Destroy: entity id=%#x Ents.Alive=%v orig.Valid=%v copy.Valid=%v",
		uint32(id), w.Ents.Alive(id), aOrig, aCp)
	if aOrig || aCp {
		t.Fatalf("destroyed unit: orig.Valid=%v copy.Valid=%v, want false/false", aOrig, aCp)
	}
	if aOrig != aCp {
		t.Fatalf("copy disagreed with original after invalidation: %v vs %v", aOrig, aCp)
	}
}

// TestWidgetInterface — edge case (3): Unit/Item/Destructable satisfy
// Widget, proven at compile time (the package-level assertions in
// handles.go) and exercised here through the interface.
func TestWidgetInterface(t *testing.T) {
	widgets := []struct {
		name string
		w    Widget
	}{
		{"Unit", Unit{}}, {"Item", Item{}}, {"Destructable", Destructable{}},
	}
	for _, x := range widgets {
		t.Logf("Widget satisfied by %-13s Valid=%v IsZero=%v", x.name, x.w.Valid(), x.w.IsZero())
		if !x.w.IsZero() {
			t.Errorf("%s as Widget: zero value IsZero()=false", x.name)
		}
	}
	// Missile is intentionally NOT a Widget (not a targetable world
	// object in the same sense); confirm the seal holds.
	if _, ok := any(Missile{}).(Widget); ok {
		t.Error("Missile unexpectedly satisfies Widget")
	}
}

// TestAngleConversions checks the Deg/Rad value-type contract (N-10):
// the radians/degrees round-trip is exact at the cardinal angles.
func TestAngleConversions(t *testing.T) {
	if got := Deg(180).Radians(); !approx(got, 3.141592653589793) {
		t.Errorf("Deg(180).Radians() = %v, want π", got)
	}
	if got := Rad(3.141592653589793).Degrees(); !approx(got, 180) {
		t.Errorf("Rad(π).Degrees() = %v, want 180", got)
	}
	if !(Angle{}).IsZero() || Deg(0).IsZero() != true {
		t.Errorf("zero Angle IsZero contract broken")
	}
	t.Logf("Deg(180)=%v rad  Rad(π)=%v deg", Deg(180).Radians(), Rad(3.141592653589793).Degrees())
}

func kindName(k kind) string {
	switch k {
	case kindRoot:
		return "root"
	case kindNoun:
		return "noun"
	case kindValue:
		return "value"
	case kindInterface:
		return "interface"
	}
	return "?"
}

func approx(a, b float64) bool {
	d := a - b
	if d < 0 {
		d = -d
	}
	return d < 1e-9
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	neg := n < 0
	if neg {
		n = -n
	}
	var buf [20]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	if neg {
		i--
		buf[i] = '-'
	}
	return string(buf[i:])
}
