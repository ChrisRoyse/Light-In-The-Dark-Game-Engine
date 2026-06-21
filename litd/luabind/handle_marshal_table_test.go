package luabind

// Guard test for the #489 handle-marshal contract. Every api type the binding
// layer can hand to Lua as a userdata (`pushHandle`-able) is one a world script
// could capture as a Game_Every / trigger upvalue, so SaveScripts must give a
// DEFINITE answer for each: either it marshals through the GameHandles seam and
// round-trips to the same opaque token, or it fails CLOSED with a message that
// names the concrete type — never a silent drop.
//
// This converts the silent gap #489 documented (RefOf's default branch) into an
// enforced, enumerated contract. When a future world needs one of the
// fail-closed types, add its HandleKind + RefOf/Resolve case and move it to the
// marshalable table (with a round-trip), mirroring the ItemType/ResourceNodeType
// additions here.
//
// SoT = the bytes GameHandles.MarshalUserData emits (the real SaveScripts path),
// re-marshaled after an Unmarshal to prove token identity; for the fail path,
// the error string.

import (
	"strings"
	"testing"
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	data "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	lua "github.com/yuin/gopher-lua"
)

func TestPushHandleMarshalContract(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 7})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{{ID: "hfoo", Life: 100, CollisionSize: 16}}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineItems([]data.Item{{ID: "potion", Class: 1, Charges: 2}}); err != nil {
		t.Fatalf("DefineItems: %v", err)
	}
	if err := g.DefineBuffTypes([]data.BuffType{{ID: "slow", DurationTicks: 40, Stacking: data.StackCount, MaxStacks: 3}}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	if err := g.DefineResourceNodes([]data.ResourceNodeType{{ID: "goldmine", Resource: 0, Amount: 500}}); err != nil {
		t.Fatalf("DefineResourceNodes: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit: invalid hero")
	}
	gh := GameHandles{G: g}

	// --- Stable handles: MUST marshal and round-trip to an identical token. ---
	marshalable := []struct {
		name string
		h    api.Handle
	}{
		{"Unit", u},
		{"Player", g.Player(1)},
		{"UnitType", g.UnitType("hfoo")},
		{"BuffType", g.BuffType("slow")},
		{"ItemType", g.ItemType("potion")},                 // #489
		{"ResourceNodeType", g.ResourceNodeType("goldmine")}, // #489
		{"Storage", g.Storage()},
	}
	for _, c := range marshalable {
		t.Run("marshal/"+c.name, func(t *testing.T) {
			if c.h.IsZero() {
				t.Fatalf("%s fixture is the null handle — pick a defined one", c.name)
			}
			blob, err := gh.MarshalUserData(&lua.LUserData{Value: c.h})
			if err != nil {
				t.Fatalf("%s must marshal through the seam, got %v", c.name, err)
			}
			ud2, err := gh.UnmarshalUserData(blob)
			if err != nil {
				t.Fatalf("%s unmarshal: %v", c.name, err)
			}
			// SoT: re-marshaling the rebound handle yields byte-identical token.
			blob2, err := gh.MarshalUserData(ud2)
			if err != nil || string(blob2) != string(blob) {
				t.Fatalf("%s token drift across round-trip: %s -> %s (err %v)", c.name, blob, blob2, err)
			}
			t.Logf("FSV %s marshals + round-trips, token=%s", c.name, blob)
		})
	}

	// --- Session / command handles: NOT stable serializable tokens. MUST fail
	// closed with a message that names the concrete api type (so the next agent
	// knows exactly what to add), never a silent drop. ---
	failclosed := []struct {
		name     string
		v        any
		wantFrag string
	}{
		// Timer satisfies api.Handle (Valid+IsZero) but has no entity-backed ref.
		{"Timer", g.After(time.Second, func() {}), "not marshalable through the entity-backed seam"},
		// Trigger/Subscription/Order are not api.Handle at all (missing Valid or IsZero).
		{"Trigger", g.NewTrigger(), "not an api.Handle"},
		{"Subscription", g.OnAttack(func(api.Event) {}), "not an api.Handle"},
		{"Order", g.Order("attack"), "not an api.Handle"},
	}
	for _, c := range failclosed {
		t.Run("failclosed/"+c.name, func(t *testing.T) {
			_, err := gh.MarshalUserData(&lua.LUserData{Value: c.v})
			if err == nil {
				t.Fatalf("%s must fail closed at the marshal seam, got nil error", c.name)
			}
			if !strings.Contains(err.Error(), c.wantFrag) {
				t.Fatalf("%s error %q must contain %q", c.name, err, c.wantFrag)
			}
			// Contract: the message names the concrete type. The api package is
			// declared `package litd`, so %T renders it as "litd.<Name>".
			if !strings.Contains(err.Error(), "litd."+c.name) {
				t.Fatalf("%s error %q must name the type (litd.%s)", c.name, err, c.name)
			}
			t.Logf("FSV %s fails closed (names the type): %v", c.name, err)
		})
	}
}
