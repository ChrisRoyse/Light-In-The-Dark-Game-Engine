package luabind

// customevent_bindings.go — hand-written Lua bindings for script-defined
// custom events (PRD2 04, #619). RegisterEvent mints a named kind; Emit/
// EmitGroup fire one; subscription uses the existing OnEvent (already
// widened to resolve registered custom kinds). Same shape as
// bindings_catalog.go.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// bindRegisterEvent: RegisterEvent(name) -> kind number (0 if full).
func (b gameBinder) bindRegisterEvent(L *lua.LState) int {
	L.Push(lua.LNumber(b.g.RegisterEvent(L.CheckString(1))))
	return 1
}

// bindEventKind: EventKind(name) -> kind number (0 if unregistered).
func (b gameBinder) bindEventKind(L *lua.LState) int {
	L.Push(lua.LNumber(b.g.EventKindByName(L.CheckString(1))))
	return 1
}

// bindEmitEvent: Emit(kind, src, dst, arg). src/dst optional (nil → zero
// Unit); arg optional (default 0). Returns true if queued.
func (b gameBinder) bindEmitEvent(L *lua.LState) int {
	kind := api.EventKind(L.CheckInt(1))
	src := optUnit(L, 2)
	dst := optUnit(L, 3)
	var arg int64
	if n, ok := L.Get(4).(lua.LNumber); ok {
		arg = int64(n)
	}
	L.Push(lua.LBool(b.g.Emit(kind, src, dst, arg)))
	return 1
}

// bindEmitGroup: EmitGroup(kind, src, group). Returns true if queued.
func (b gameBinder) bindEmitGroup(L *lua.LState) int {
	kind := api.EventKind(L.CheckInt(1))
	src := optUnit(L, 2)
	grp := argGroup(L, 3)
	L.Push(lua.LBool(b.g.EmitGroup(kind, src, grp)))
	return 1
}

// optUnit reads arg i as a Unit userdata, or the zero Unit if nil/absent.
func optUnit(L *lua.LState, i int) api.Unit {
	if ud, ok := L.Get(i).(*lua.LUserData); ok {
		if u, ok := ud.Value.(api.Unit); ok {
			return u
		}
	}
	return api.Unit{}
}

// registerCustomEvents installs the custom-event verbs. Called from
// Register. OnEvent is already a global (the event-subscription surface).
func registerCustomEvents(L *lua.LState, b gameBinder) {
	L.SetGlobal("RegisterEvent", L.NewFunction(b.bindRegisterEvent))
	L.SetGlobal("EventKind", L.NewFunction(b.bindEventKind))
	L.SetGlobal("Emit", L.NewFunction(b.bindEmitEvent))
	L.SetGlobal("EmitGroup", L.NewFunction(b.bindEmitGroup))
}
