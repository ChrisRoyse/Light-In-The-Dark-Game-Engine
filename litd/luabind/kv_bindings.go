package luabind

// kv_bindings.go — hand-written Lua bindings for the key-value store
// (PRD2 03, #573). Lua is dynamically typed, so SetKV/GetKV infer the
// variant from the Lua value; explicit SetKVInt/SetKVReal pin the numeric
// type. Scopes: entity (Unit handle), global, player. Same hand-written
// shape as bindings_catalog.go.

import (
	"math"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// kvSetInferred writes a Lua value into entity scope, inferring the type:
// integral number→Int, fractional number→Real, string→String, bool→Bool,
// Unit userdata→Unit, Group userdata→GroupRef. Unsupported values raise.
func kvSetInferred(L *lua.LState, u api.Unit, key string, vi int) {
	switch v := L.Get(vi).(type) {
	case lua.LBool:
		u.SetBool(key, bool(v))
	case lua.LNumber:
		f := float64(v)
		if f == math.Trunc(f) {
			u.SetInt(key, int64(f))
		} else {
			u.SetReal(key, f)
		}
	case lua.LString:
		u.SetString(key, string(v))
	case *lua.LUserData:
		switch h := v.Value.(type) {
		case api.Unit:
			u.SetUnit(key, h)
		case api.Group:
			u.SetGroupRef(key, h)
		default:
			L.ArgError(vi, "SetKV: unsupported handle type")
		}
	default:
		L.ArgError(vi, "SetKV: unsupported value type")
	}
}

// pushDynamic pushes a Go KV value (from GetDynamic) as the matching Lua
// value; nil → Lua nil.
func pushDynamic(L *lua.LState, v any) int {
	switch x := v.(type) {
	case nil:
		L.Push(lua.LNil)
	case int64:
		L.Push(lua.LNumber(x))
	case float64:
		L.Push(lua.LNumber(x))
	case bool:
		L.Push(lua.LBool(x))
	case string:
		L.Push(lua.LString(x))
	case api.Unit:
		L.Push(pushHandle(L, x))
	case api.Group:
		L.Push(pushHandle(L, x))
	case api.Vec2:
		t := L.NewTable()
		t.RawSetString("x", lua.LNumber(x.X))
		t.RawSetString("y", lua.LNumber(x.Y))
		L.Push(t)
	default:
		L.Push(lua.LNil)
	}
	return 1
}

// --- entity scope ---

func bindSetKV(L *lua.LState) int {
	u := argUnit(L, 1)
	kvSetInferred(L, u, L.CheckString(2), 3)
	return 0
}
func (b gameBinder) bindGetKV(L *lua.LState) int {
	return pushDynamic(L, argUnit(L, 1).GetDynamic(L.CheckString(2)))
}
func bindSetKVInt(L *lua.LState) int {
	argUnit(L, 1).SetInt(L.CheckString(2), int64(L.CheckNumber(3)))
	return 0
}
func bindSetKVReal(L *lua.LState) int {
	argUnit(L, 1).SetReal(L.CheckString(2), float64(L.CheckNumber(3)))
	return 0
}
func bindHasKV(L *lua.LState) int {
	L.Push(lua.LBool(argUnit(L, 1).Has(L.CheckString(2))))
	return 1
}
func bindDeleteKV(L *lua.LState) int {
	argUnit(L, 1).DeleteKey(L.CheckString(2))
	return 0
}
func bindEachKV(L *lua.LState) int {
	u := argUnit(L, 1)
	fn := L.CheckFunction(2)
	u.EachKey(func(key string) {
		L.Push(fn)
		L.Push(lua.LString(key))
		L.Call(1, 0)
	})
	return 0
}

// --- global scope ---

func (b gameBinder) bindSetGlobalKV(L *lua.LState) int {
	key := L.CheckString(1)
	switch v := L.Get(2).(type) {
	case lua.LBool:
		b.g.SetGlobalBool(key, bool(v))
	case lua.LNumber:
		f := float64(v)
		if f == math.Trunc(f) {
			b.g.SetGlobalInt(key, int64(f))
		} else {
			b.g.SetGlobalReal(key, f)
		}
	case lua.LString:
		b.g.SetGlobalString(key, string(v))
	case *lua.LUserData:
		if h, ok := v.Value.(api.Unit); ok {
			b.g.SetGlobalUnit(key, h)
		} else {
			L.ArgError(2, "SetGlobalKV: unsupported handle type")
		}
	default:
		L.ArgError(2, "SetGlobalKV: unsupported value type")
	}
	return 0
}
func (b gameBinder) bindGetGlobalKV(L *lua.LState) int {
	return pushDynamic(L, b.g.GetGlobalDynamic(L.CheckString(1)))
}

// --- player scope ---

func (b gameBinder) bindSetPlayerKV(L *lua.LState) int {
	p := argPlayer(L, 1)
	key := L.CheckString(2)
	switch v := L.Get(3).(type) {
	case lua.LBool:
		p.SetBool(key, bool(v))
	case lua.LNumber:
		f := float64(v)
		if f == math.Trunc(f) {
			p.SetInt(key, int64(f))
		} else {
			p.SetReal(key, f)
		}
	case lua.LString:
		p.SetString(key, string(v))
	default:
		L.ArgError(3, "SetPlayerKV: unsupported value type")
	}
	return 0
}
func bindGetPlayerKV(L *lua.LState) int {
	return pushDynamic(L, argPlayer(L, 1).GetDynamic(L.CheckString(2)))
}

// registerKV installs the KV verbs as globals. Called from Register.
func registerKV(L *lua.LState, b gameBinder) {
	L.SetGlobal("SetKV", L.NewFunction(bindSetKV))
	L.SetGlobal("GetKV", L.NewFunction(b.bindGetKV))
	L.SetGlobal("SetKVInt", L.NewFunction(bindSetKVInt))
	L.SetGlobal("SetKVReal", L.NewFunction(bindSetKVReal))
	L.SetGlobal("HasKV", L.NewFunction(bindHasKV))
	L.SetGlobal("DeleteKV", L.NewFunction(bindDeleteKV))
	L.SetGlobal("EachKV", L.NewFunction(bindEachKV))
	L.SetGlobal("SetGlobalKV", L.NewFunction(b.bindSetGlobalKV))
	L.SetGlobal("GetGlobalKV", L.NewFunction(b.bindGetGlobalKV))
	L.SetGlobal("SetPlayerKV", L.NewFunction(b.bindSetPlayerKV))
	L.SetGlobal("GetPlayerKV", L.NewFunction(bindGetPlayerKV))
}
