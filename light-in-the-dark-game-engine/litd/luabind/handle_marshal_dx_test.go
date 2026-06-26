package luabind

// #663 regression: a captured non-entity-backed handle (Ability, Camera, ...) held
// as a closure upvalue makes savegame.Write fail. The OLD error — "handle litd.X is
// not marshalable through the entity-backed seam" — named neither the cause (a
// closure capturing a handle that cannot survive save/load) nor the fix (capture a
// ref/id and re-derive inside the callback). This asserts the error is now
// actionable: it names the type, the root reason, and the save-safe pattern.
//
// SoT = the actual error string returned by GameHandles.MarshalUserData, read here
// (not a return code). Camera is the canonical non-entity-backed handle (cited in
// handle_marshal.go) and is trivially constructable; it exercises the identical
// RefOf-false path an Ability upvalue hits during #641-class save failures.

import (
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestMarshalNonEntityHandleActionableErrorFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	gh := GameHandles{G: g}

	// A non-entity-backed handle (Camera) — RefOf returns false for it, the exact
	// path an Ability upvalue takes. SoT BEFORE: confirm it is genuinely the
	// unmarshalable class (RefOf false) so the test proves the message, not a typo.
	cam := g.Camera(g.Player(0))
	if _, ok := api.RefOf(cam); ok {
		t.Fatal("precondition: Camera is expected to be non-entity-backed (RefOf false)")
	}

	_, err = gh.MarshalUserData(&lua.LUserData{Value: cam})
	if err == nil {
		t.Fatal("marshaling a non-entity-backed handle must fail (fail-closed), got nil error")
	}
	msg := err.Error()
	t.Logf("FSV #663 error message:\n  %s", msg)

	// The message must be ACTIONABLE — name the cause and the save-safe pattern.
	// These fragments are the DX contract the author reads when their save dies.
	for _, want := range []string{
		"#663",            // traceable to the discovery
		"not ",            // states it is not entity-backed
		"entity-backed",   // the root reason
		"save/load",       // the consequence
		"Game_Every",      // names the closure class that captures upvalues
		"Game_AbilityRef", // the marshal-safe REF source
		"Unit_AddAbility", // re-derive inside the callback
		"idempotent",      // why re-deriving is safe
	} {
		if !strings.Contains(msg, want) {
			t.Fatalf("error message missing actionable fragment %q.\nfull message: %s", want, msg)
		}
	}

	// It must also name the offending handle TYPE so the author knows which capture
	// to fix (the old message did this via %T; keep it).
	if !strings.Contains(msg, "Camera") {
		t.Fatalf("error message should name the handle type (Camera), got: %s", msg)
	}

	// Contrast (the fix works): an entity-backed Player handle marshals cleanly — so
	// the actionable error fires ONLY for the unmarshalable class, never a false
	// alarm on the save-safe handles the message recommends.
	if _, ok := api.RefOf(g.Player(0)); !ok {
		t.Fatal("precondition: Player must be entity-backed (RefOf true) — it is the recommended save-safe handle")
	}
	if _, err := gh.MarshalUserData(&lua.LUserData{Value: g.Player(0)}); err != nil {
		t.Fatalf("an entity-backed Player handle must marshal cleanly (the message recommends it), got: %v", err)
	}
	t.Logf("FSV #663: Player (entity-backed) marshals clean; Camera (not) yields the actionable error")
}
