package luabind

// mover_bindings.go — hand-written Lua bindings for the unified motion
// controller (PRD2 05, #591). One options table feeds every verb:
// MoveLinear/Homing/Point/OrbitUnit/OrbitPoint/Arc/Spline/Custom and
// SpawnProjectile. A Mover is pushed as interned userdata (identity
// equality, survives save/load as the underlying mover does). Cancel/Valid
// are free functions taking the handle, matching the group surface.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// moverOpts parses a Lua options table into api.MoverOptions. Every key is
// optional; absent keys leave the zero value. Vec2 keys are {x=,y=} tables;
// angle keys (angvel/turnrate/direction) are numbers in degrees.
func (b gameBinder) moverOpts(L *lua.LState, i int) api.MoverOptions {
	var o api.MoverOptions
	t, ok := L.Get(i).(*lua.LTable)
	if !ok {
		return o
	}
	num := func(k string) (float64, bool) {
		if v, ok := t.RawGetString(k).(lua.LNumber); ok {
			return float64(v), true
		}
		return 0, false
	}
	unit := func(k string) api.Unit {
		if ud, ok := t.RawGetString(k).(*lua.LUserData); ok {
			if u, ok := ud.Value.(api.Unit); ok {
				return u
			}
		}
		return api.Unit{}
	}
	boolean := func(k string) bool {
		v, ok := t.RawGetString(k).(lua.LBool)
		return ok && bool(v)
	}
	vec := func(k string) api.Vec2 {
		if vt, ok := t.RawGetString(k).(*lua.LTable); ok {
			x, _ := vt.RawGetString("x").(lua.LNumber)
			y, _ := vt.RawGetString("y").(lua.LNumber)
			return api.Vec2{X: float64(x), Y: float64(y)}
		}
		return api.Vec2{}
	}

	o.Target = unit("target")
	o.Anchor = unit("anchor")
	o.Owner = unit("owner")
	o.Goal = vec("goal")
	if d, ok := num("direction"); ok {
		o.Direction = api.Deg(d)
	}
	if d, ok := num("angvel"); ok {
		o.AngVel = api.Deg(d)
	}
	if d, ok := num("turnrate"); ok {
		o.TurnRate = api.Deg(d)
	}
	if v, ok := num("speed"); ok {
		o.Speed = v
	}
	if v, ok := num("radius"); ok {
		o.Radius = v
	}
	if v, ok := num("height"); ok {
		o.Height = v
	}
	if v, ok := num("range"); ok {
		o.Range = v
	}
	if v, ok := num("damage"); ok {
		o.Damage = v
	}
	if v, ok := num("pierce"); ok {
		o.Pierce = int(v)
	}
	if v, ok := num("decay"); ok {
		o.Decay = int(v)
	}
	if v, ok := num("hitmask"); ok {
		o.HitMask = api.MoverHitMask(uint16(v))
	}
	if v, ok := num("done"); ok {
		o.Done = api.MoverDone(uint8(v))
	}
	if v, ok := num("oncomplete"); ok {
		o.OnDone = uint16(v)
	}
	if v, ok := num("custom"); ok {
		o.Custom = uint16(v)
	}
	if v, ok := num("fx"); ok {
		o.FX = uint16(v)
	}
	o.Authority = boolean("authority")
	o.Flying = boolean("flying")
	o.Consume = boolean("consume")
	if wt, ok := t.RawGetString("waypoints").(*lua.LTable); ok {
		n := wt.Len()
		o.Waypoints = make([]api.Vec2, 0, n)
		for k := 1; k <= n; k++ {
			if vt, ok := wt.RawGetInt(k).(*lua.LTable); ok {
				x, _ := vt.RawGetString("x").(lua.LNumber)
				y, _ := vt.RawGetString("y").(lua.LNumber)
				o.Waypoints = append(o.Waypoints, api.Vec2{X: float64(x), Y: float64(y)})
			}
		}
	}
	return o
}

func (b gameBinder) bindMoveLinear(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.MoveLinear(b.moverOpts(L, 1))))
	return 1
}
func (b gameBinder) bindMoveHoming(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.MoveHoming(b.moverOpts(L, 1))))
	return 1
}
func (b gameBinder) bindMovePoint(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.MovePoint(b.moverOpts(L, 1))))
	return 1
}
func (b gameBinder) bindMoveOrbitUnit(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.MoveOrbitUnit(b.moverOpts(L, 1))))
	return 1
}
func (b gameBinder) bindMoveOrbitPoint(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.MoveOrbitPoint(b.moverOpts(L, 1))))
	return 1
}
func (b gameBinder) bindMoveArc(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.MoveArc(b.moverOpts(L, 1))))
	return 1
}
func (b gameBinder) bindMoveSpline(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.MoveSpline(b.moverOpts(L, 1))))
	return 1
}
func (b gameBinder) bindMoveCustom(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.MoveCustom(b.moverOpts(L, 1))))
	return 1
}

// bindSpawnProjectile: SpawnProjectile(origin, kind, opts) -> Mover.
// kind is a number (0 linear, 1 point, 2 arc, 3 homing).
func (b gameBinder) bindSpawnProjectile(L *lua.LState) int {
	origin := argVec2(L, 1)
	kind := api.ProjectileKind(uint8(L.CheckInt(2)))
	L.Push(pushHandle(L, b.g.SpawnProjectile(origin, kind, b.moverOpts(L, 3))))
	return 1
}

// argMover reads arg i as a Mover userdata.
func argMover(L *lua.LState, i int) api.Mover {
	m, ok := handleArg(L, i).(api.Mover)
	if !ok {
		L.ArgError(i, "expected Mover userdata (from a Move* verb)")
	}
	return m
}

func bindMoverCancel(L *lua.LState) int { argMover(L, 1).Cancel(); return 0 }
func bindMoverValid(L *lua.LState) int  { L.Push(lua.LBool(argMover(L, 1).Valid())); return 1 }

// registerMovers installs the mover verbs. Called from Register.
func registerMovers(L *lua.LState, b gameBinder) {
	L.SetGlobal("MoveLinear", L.NewFunction(b.bindMoveLinear))
	L.SetGlobal("MoveHoming", L.NewFunction(b.bindMoveHoming))
	L.SetGlobal("MovePoint", L.NewFunction(b.bindMovePoint))
	L.SetGlobal("MoveOrbitUnit", L.NewFunction(b.bindMoveOrbitUnit))
	L.SetGlobal("MoveOrbitPoint", L.NewFunction(b.bindMoveOrbitPoint))
	L.SetGlobal("MoveArc", L.NewFunction(b.bindMoveArc))
	L.SetGlobal("MoveSpline", L.NewFunction(b.bindMoveSpline))
	L.SetGlobal("MoveCustom", L.NewFunction(b.bindMoveCustom))
	L.SetGlobal("SpawnProjectile", L.NewFunction(b.bindSpawnProjectile))
	L.SetGlobal("CancelMover", L.NewFunction(bindMoverCancel))
	L.SetGlobal("MoverValid", L.NewFunction(bindMoverValid))
}
