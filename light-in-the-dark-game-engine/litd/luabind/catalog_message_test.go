package luabind

// Catalog message bindings (#267): Game_Print / Game_ClearMessages, bound by
// hand (the generated dispatch defers Print for its variadic ...PrintOption).
// SoT = the UIMessageEvent stream the api emits via OnUI — the observable
// outcome of the trigger.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestGameMessageBindingsFSV(t *testing.T) {
	g, _ := confGame(t, 5)
	var events []api.UIMessageEvent
	g.OnUI(func(ev api.UIMessageEvent) { events = append(events, ev) })

	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	setP := func(name string, p api.Player) {
		ud := L.NewUserData()
		ud.Value = p
		L.SetGlobal(name, ud)
	}
	setP("p1", g.Player(1))
	setP("p2", g.Player(2))

	// Print to one player: exactly one UIPrint event with the text.
	if err := L.DoString(`Game_Print({p1}, "hello world")`); err != nil {
		t.Fatalf("Game_Print: %v", err)
	}
	if len(events) != 1 || events[0].Kind != api.UIPrint || events[0].Text != "hello world" {
		t.Fatalf("Print SoT mismatch: %+v", events)
	}
	t.Logf("FSV #267 Print: emitted %+v", events[0])

	// Print to two players: one event each (the loop over the player slice).
	events = nil
	if err := L.DoString(`Game_Print({p1, p2}, "to both")`); err != nil {
		t.Fatalf("Game_Print 2: %v", err)
	}
	if len(events) != 2 {
		t.Fatalf("Print to 2 players emitted %d events, want 2", len(events))
	}

	// ClearMessages: a UIClear event.
	events = nil
	if err := L.DoString(`Game_ClearMessages({p1})`); err != nil {
		t.Fatalf("Game_ClearMessages: %v", err)
	}
	if len(events) != 1 || events[0].Kind != api.UIClear {
		t.Fatalf("ClearMessages SoT mismatch: %+v", events)
	}
	t.Logf("FSV #267 ClearMessages: emitted %+v", events[0])

	// Fail-closed: a non-Player element raises, emits nothing.
	events = nil
	if err := L.DoString(`Game_Print({"not a player"}, "x")`); err == nil {
		t.Fatal("Print with a non-Player element must raise")
	}
	if len(events) != 0 {
		t.Fatalf("a rejected Print still emitted events: %+v", events)
	}
	t.Logf("FSV #267 fail-closed: non-Player arg raised, no event emitted")
}
