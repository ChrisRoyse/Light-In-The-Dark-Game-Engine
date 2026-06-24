package luabind

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

// #581 / #616 — a coroutine WaitForEvent on a CUSTOM kind wakes when that
// custom event is emitted. SoT = a Go mark() the coroutine calls after it
// resumes. End-to-end: register → Run+WaitForEvent → Emit → wake.
func TestLuaCoroutineWaitsOnCustomEvent(t *testing.T) {
	g, L, _ := newScriptGame(t)
	var resumed int
	L.SetGlobal("mark", L.NewFunction(func(L *lua.LState) int { resumed++; return 0 }))

	if err := L.DoString(`
		k = RegisterEvent("ping")
		Run(function()
			WaitForEvent(k)   -- parks until a "ping" custom event fires
			mark()
		end)
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	// Let the coroutine reach its WaitForEvent park.
	g.Advance(1)
	if resumed != 0 {
		t.Fatalf("coroutine resumed before any event: %d", resumed)
	}
	// Emit the custom event; the parked coroutine must wake.
	if err := L.DoString(`Emit(k, nil, nil, 0)`); err != nil {
		t.Fatalf("emit: %v", err)
	}
	g.Advance(2) // dispatch + the 1-tick resume hop
	if resumed != 1 {
		t.Fatalf("coroutine resumed %d times after custom emit, want 1", resumed)
	}
}
