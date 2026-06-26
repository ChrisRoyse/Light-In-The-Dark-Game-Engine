package luabind

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

// #591 — Lua mover surface end-to-end. SoT = MoverValid() observed at the
// Lua boundary across the projectile's lifecycle, plus the state hash
// reacting to the spawn.

func luaBool(L *lua.LState, name string) bool {
	v, _ := L.GetGlobal(name).(lua.LBool)
	return bool(v)
}

func TestLuaSpawnProjectileLifecycle(t *testing.T) {
	g, L, _ := newScriptGame(t)
	h0 := g.StateHash()
	if err := L.DoString(`
		m = SpawnProjectile({x=0, y=0}, 1, {goal={x=100, y=0}, speed=10}) -- kind 1 = point
		alive0 = MoverValid(m)
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if !luaBool(L, "alive0") {
		t.Fatal("MoverValid(m) false right after spawn")
	}
	if g.StateHash() == h0 {
		t.Fatal("spawning a projectile did not change the state hash")
	}
	// Arrival at 100/10 = 10 ticks → consumed on impact.
	g.Advance(12)
	if err := L.DoString(`alive1 = MoverValid(m)`); err != nil {
		t.Fatalf("lua2: %v", err)
	}
	if luaBool(L, "alive1") {
		t.Fatal("MoverValid(m) still true after the projectile was consumed")
	}
}

func TestLuaMoverCancel(t *testing.T) {
	_, L, _ := newScriptGame(t)
	if err := L.DoString(`
		m = SpawnProjectile({x=0, y=0}, 0, {direction=0, speed=5, range=1000})
		wasAlive = MoverValid(m)
		CancelMover(m)
		nowAlive = MoverValid(m)
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if !luaBool(L, "wasAlive") {
		t.Fatal("mover not alive after spawn")
	}
	if luaBool(L, "nowAlive") {
		t.Fatal("CancelMover did not free the mover")
	}
}
