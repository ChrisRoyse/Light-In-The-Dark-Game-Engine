package litd

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Public key-value store surface (PRD2 03, #573). Typed accessors keep the
// tagged union safe at the API boundary: a getter checks the stored type
// and returns (zero, false) on a mismatch or an absent key, never
// reinterpreting bits. Keys are plain strings, interned on first write;
// reads use a non-interning lookup so a get of an unseen key is a clean
// miss (no key-table growth). Scopes: entity (Unit handle), global
// (Game), player (Player handle).

// kvAccess is the shared typed implementation bound to one owner. Public
// scope methods are thin wrappers over it, so the type discipline lives
// in exactly one place.
type kvAccess struct {
	g     *Game
	owner uint64
}

func (a kvAccess) ok() bool { return a.g != nil && a.g.w != nil }

// keyForRead resolves a key string to its id WITHOUT interning (0 = unseen
// ⇒ guaranteed miss).
func (a kvAccess) keyForRead(key string) uint32 { return a.g.w.KV.KeyID(key) }

func (a kvAccess) set(key string, typ sim.KVType, val, val2 int64) {
	if !a.ok() {
		return
	}
	a.g.w.KV.KVSet(a.owner, a.g.w.KV.InternKey(key), typ, val, val2)
}

func (a kvAccess) get(key string) (sim.KVType, int64, int64, bool) {
	if !a.ok() {
		return 0, 0, 0, false
	}
	id := a.keyForRead(key)
	if id == 0 {
		return 0, 0, 0, false
	}
	return a.g.w.KV.KVGet(a.owner, id)
}

func (a kvAccess) SetInt(key string, v int64) { a.set(key, sim.KVInt, v, 0) }
func (a kvAccess) GetInt(key string) (int64, bool) {
	typ, v, _, ok := a.get(key)
	if !ok || typ != sim.KVInt {
		return 0, false
	}
	return v, true
}

func (a kvAccess) SetReal(key string, v float64) { a.set(key, sim.KVFixed, int64(fromFloat(v)), 0) }
func (a kvAccess) GetReal(key string) (float64, bool) {
	typ, v, _, ok := a.get(key)
	if !ok || typ != sim.KVFixed {
		return 0, false
	}
	return toFloat(fixed.F64(v)), true
}

func (a kvAccess) SetBool(key string, v bool) {
	var b int64
	if v {
		b = 1
	}
	a.set(key, sim.KVBool, b, 0)
}
func (a kvAccess) GetBool(key string) (bool, bool) {
	typ, v, _, ok := a.get(key)
	if !ok || typ != sim.KVBool {
		return false, false
	}
	return v != 0, true
}

func (a kvAccess) SetString(key, v string) {
	if !a.ok() {
		return
	}
	a.set(key, sim.KVString, int64(a.g.w.KV.InternStr(v)), 0)
}
func (a kvAccess) GetString(key string) (string, bool) {
	typ, v, _, ok := a.get(key)
	if !ok || typ != sim.KVString {
		return "", false
	}
	return a.g.w.KV.StrValue(uint32(v))
}

func (a kvAccess) SetUnit(key string, u Unit) { a.set(key, sim.KVEntity, int64(uint32(u.id)), 0) }
func (a kvAccess) GetUnit(key string) (Unit, bool) {
	typ, v, _, ok := a.get(key)
	if !ok || typ != sim.KVEntity {
		return Unit{}, false
	}
	return Unit{id: sim.EntityID(uint32(v)), g: a.g}, true
}

func (a kvAccess) SetPoint(key string, v Vec2) {
	a.set(key, sim.KVVec2, int64(fromFloat(v.X)), int64(fromFloat(v.Y)))
}
func (a kvAccess) GetPoint(key string) (Vec2, bool) {
	typ, x, y, ok := a.get(key)
	if !ok || typ != sim.KVVec2 {
		return Vec2{}, false
	}
	return Vec2{X: toFloat(fixed.F64(x)), Y: toFloat(fixed.F64(y))}, true
}

func (a kvAccess) SetGroupRef(key string, gr Group) { a.set(key, sim.KVGroup, int64(uint32(gr.id)), 0) }
func (a kvAccess) GetGroupRef(key string) (Group, bool) {
	typ, v, _, ok := a.get(key)
	if !ok || typ != sim.KVGroup {
		return Group{}, false
	}
	return Group{id: sim.GroupID(uint32(v)), g: a.g}, true
}

func (a kvAccess) Has(key string) bool {
	id := a.keyForRead(key)
	return a.ok() && id != 0 && a.g.w.KV.KVHas(a.owner, id)
}

func (a kvAccess) DeleteKey(key string) {
	if id := a.keyForRead(key); a.ok() && id != 0 {
		a.g.w.KV.KVDelete(a.owner, id)
	}
}

// EachKey visits the owner's keys in interned-id-resolved order (the
// store's ascending (Owner,Key) order — deterministic).
func (a kvAccess) EachKey(fn func(key string)) {
	if !a.ok() || fn == nil {
		return
	}
	a.g.w.KV.KVEachOwner(a.owner, func(keyID uint32, _ sim.KVType, _, _ int64) {
		if s, ok := a.g.w.KV.KeyString(keyID); ok {
			fn(s)
		}
	})
}

// getDynamic returns the stored value as a Go value (int64 / float64 /
// bool / string / Unit / Vec2 / Group), or nil when absent. The dynamic
// boundary for the Lua GetKV binding (#573), which needs the stored type
// without the caller naming it.
func (a kvAccess) getDynamic(key string) any {
	typ, v, v2, ok := a.get(key)
	if !ok {
		return nil
	}
	switch typ {
	case sim.KVInt:
		return v
	case sim.KVFixed:
		return toFloat(fixed.F64(v))
	case sim.KVBool:
		return v != 0
	case sim.KVString:
		s, _ := a.g.w.KV.StrValue(uint32(v))
		return s
	case sim.KVEntity:
		return Unit{id: sim.EntityID(uint32(v)), g: a.g}
	case sim.KVVec2:
		return Vec2{X: toFloat(fixed.F64(v)), Y: toFloat(fixed.F64(v2))}
	case sim.KVGroup:
		return Group{id: sim.GroupID(uint32(v)), g: a.g}
	}
	return nil
}

// -- entity scope (Unit) — the full typed union -----------------------

func (u Unit) kv() kvAccess { return kvAccess{g: u.g, owner: sim.EntityKVOwner(u.id)} }

func (u Unit) SetInt(key string, v int64)          { u.kv().SetInt(key, v) }
func (u Unit) GetInt(key string) (int64, bool)     { return u.kv().GetInt(key) }
func (u Unit) SetReal(key string, v float64)       { u.kv().SetReal(key, v) }
func (u Unit) GetReal(key string) (float64, bool)  { return u.kv().GetReal(key) }
func (u Unit) SetBool(key string, v bool)          { u.kv().SetBool(key, v) }
func (u Unit) GetBool(key string) (bool, bool)     { return u.kv().GetBool(key) }
func (u Unit) SetString(key, v string)             { u.kv().SetString(key, v) }
func (u Unit) GetString(key string) (string, bool) { return u.kv().GetString(key) }
func (u Unit) SetUnit(key string, v Unit)          { u.kv().SetUnit(key, v) }
func (u Unit) GetUnit(key string) (Unit, bool)     { return u.kv().GetUnit(key) }
func (u Unit) SetPoint(key string, v Vec2)         { u.kv().SetPoint(key, v) }
func (u Unit) GetPoint(key string) (Vec2, bool)    { return u.kv().GetPoint(key) }
func (u Unit) SetGroupRef(key string, v Group)     { u.kv().SetGroupRef(key, v) }
func (u Unit) GetGroupRef(key string) (Group, bool) { return u.kv().GetGroupRef(key) }
func (u Unit) Has(key string) bool                 { return u.kv().Has(key) }
func (u Unit) DeleteKey(key string)                { u.kv().DeleteKey(key) }
func (u Unit) EachKey(fn func(key string))         { u.kv().EachKey(fn) }

// -- global scope (Game) ----------------------------------------------

func (g *Game) globalKV() kvAccess { return kvAccess{g: g, owner: sim.GlobalKVOwner()} }

func (g *Game) SetGlobalInt(key string, v int64)          { g.globalKV().SetInt(key, v) }
func (g *Game) GetGlobalInt(key string) (int64, bool)     { return g.globalKV().GetInt(key) }
func (g *Game) SetGlobalReal(key string, v float64)       { g.globalKV().SetReal(key, v) }
func (g *Game) GetGlobalReal(key string) (float64, bool)  { return g.globalKV().GetReal(key) }
func (g *Game) SetGlobalBool(key string, v bool)          { g.globalKV().SetBool(key, v) }
func (g *Game) GetGlobalBool(key string) (bool, bool)     { return g.globalKV().GetBool(key) }
func (g *Game) SetGlobalString(key, v string)             { g.globalKV().SetString(key, v) }
func (g *Game) GetGlobalString(key string) (string, bool) { return g.globalKV().GetString(key) }
func (g *Game) SetGlobalUnit(key string, v Unit)          { g.globalKV().SetUnit(key, v) }
func (g *Game) GetGlobalUnit(key string) (Unit, bool)     { return g.globalKV().GetUnit(key) }
func (g *Game) HasGlobal(key string) bool                 { return g.globalKV().Has(key) }
func (g *Game) DeleteGlobal(key string)                   { g.globalKV().DeleteKey(key) }
func (g *Game) EachGlobalKey(fn func(key string))         { g.globalKV().EachKey(fn) }

// -- player scope (Player) --------------------------------------------

func (p Player) kv() kvAccess { return kvAccess{g: p.g, owner: sim.PlayerKVOwner(uint8(p.idx))} }

func (p Player) SetInt(key string, v int64)          { p.kv().SetInt(key, v) }
func (p Player) GetInt(key string) (int64, bool)     { return p.kv().GetInt(key) }
func (p Player) SetReal(key string, v float64)       { p.kv().SetReal(key, v) }
func (p Player) GetReal(key string) (float64, bool)  { return p.kv().GetReal(key) }
func (p Player) SetBool(key string, v bool)          { p.kv().SetBool(key, v) }
func (p Player) GetBool(key string) (bool, bool)     { return p.kv().GetBool(key) }
func (p Player) SetString(key, v string)             { p.kv().SetString(key, v) }
func (p Player) GetString(key string) (string, bool) { return p.kv().GetString(key) }
func (p Player) HasKey(key string) bool              { return p.kv().Has(key) }
func (p Player) DeleteKey(key string)                { p.kv().DeleteKey(key) }
func (p Player) EachKey(fn func(key string))         { p.kv().EachKey(fn) }

// GetDynamic resolves a key to its Go value (nil if absent) for the
// dynamic Lua binding. Entity / global / player scopes.
func (u Unit) GetDynamic(key string) any        { return u.kv().getDynamic(key) }
func (g *Game) GetGlobalDynamic(key string) any { return g.globalKV().getDynamic(key) }
func (p Player) GetDynamic(key string) any      { return p.kv().getDynamic(key) }
