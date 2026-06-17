package luabind

// GameHandles is the concrete HandleMarshaler (#267) wiring the api handle codec
// into the coroutine persister (#264). A Lua userdata whose Value is an
// api.Handle marshals to the handle's opaque HandleRef (api.RefOf) and, on load,
// rebinds against a live *api.Game (Game.Resolve) — so a host handle held by a
// suspended coroutine survives a save/load and the rebound handle's
// generation-checked Valid() reports staleness if its entity was recycled.
//
// This is the production marshaler the binding layer installs; the persister
// stays decoupled behind the HandleMarshaler interface (persist_thread.go).

import (
	"encoding/json"
	"fmt"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// GameHandles marshals api handle userdata against the game G. G must be
// non-nil to unmarshal (rebind) — a nil game fails closed.
type GameHandles struct {
	G *api.Game
}

// MarshalUserData encodes a handle-bearing userdata to its opaque HandleRef
// bytes. It errors loudly if the userdata is not an api.Handle or is a handle
// kind that is not entity-backed (e.g. Camera) — never a silent drop.
func (h GameHandles) MarshalUserData(ud *lua.LUserData) ([]byte, error) {
	hd, ok := ud.Value.(api.Handle)
	if !ok {
		return nil, fmt.Errorf("userdata value is %T, not an api.Handle", ud.Value)
	}
	ref, ok := api.RefOf(hd)
	if !ok {
		return nil, fmt.Errorf("handle %T is not marshalable through the entity-backed seam", hd)
	}
	return json.Marshal(ref)
}

// UnmarshalUserData rebuilds a handle userdata by resolving the HandleRef
// against the live game. The rebound handle may be stale (Valid()==false) if
// its entity was recycled — that is the caller's to observe, not an error here.
func (h GameHandles) UnmarshalUserData(data []byte) (*lua.LUserData, error) {
	if h.G == nil {
		return nil, fmt.Errorf("GameHandles: nil game, cannot rebind handle")
	}
	var ref api.HandleRef
	if err := json.Unmarshal(data, &ref); err != nil {
		return nil, fmt.Errorf("GameHandles: malformed handle ref: %w", err)
	}
	hd, ok := h.G.Resolve(ref)
	if !ok {
		return nil, fmt.Errorf("GameHandles: handle ref kind %d is unknown/null", ref.Kind)
	}
	return &lua.LUserData{Value: hd}, nil
}
