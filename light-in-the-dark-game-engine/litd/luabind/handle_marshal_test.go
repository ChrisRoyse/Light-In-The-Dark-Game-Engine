package luabind

// FSV for the game-backed HandleMarshaler (#267 + #264 integration). SoT = the
// HandleRef recovered from a userdata after a marshal->bytes->unmarshal round
// trip against a REAL *api.Game, and the reloaded register file of a coroutine
// saved/restored through GameHandles. (Handle validity/staleness across an
// entity recycle is FSV'd in litd/api TestHandleRefRoundTripAndStaleness; a
// public path to create a live unit from outside api is gap #387, so this test
// rounds-trips handle identity, not liveness.)

import (
	"encoding/json"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestGameHandlesRoundTripIdentity(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	gh := GameHandles{G: g}

	// A real (entity-backed) Unit handle obtained through the public codec.
	ref := api.HandleRef{Kind: api.HandleUnit, Raw: 0x01000007} // index 7, gen 1
	hd, ok := g.Resolve(ref)
	if !ok {
		t.Fatal("Resolve(unit ref) returned ok=false")
	}
	ud := &lua.LUserData{Value: hd}

	// Marshal -> bytes. SoT: the bytes are exactly the opaque ref JSON.
	blob, err := gh.MarshalUserData(ud)
	if err != nil {
		t.Fatalf("MarshalUserData: %v", err)
	}
	wantBlob, _ := json.Marshal(ref)
	t.Logf("FSV handle userdata token: %s", blob)
	if string(blob) != string(wantBlob) {
		t.Fatalf("token = %s, want %s", blob, wantBlob)
	}

	// Unmarshal -> userdata. SoT: its Value is an api.Handle whose ref matches.
	ud2, err := gh.UnmarshalUserData(blob)
	if err != nil {
		t.Fatalf("UnmarshalUserData: %v", err)
	}
	h2, ok := ud2.Value.(api.Handle)
	if !ok {
		t.Fatalf("rebound userdata Value is %T, not api.Handle", ud2.Value)
	}
	got, ok := api.RefOf(h2)
	if !ok || got != ref {
		t.Fatalf("round-trip ref = %+v ok=%v, want %+v", got, ok, ref)
	}
	t.Logf("FSV identity: ref %+v survived userdata round-trip through a real game", got)

	// Edge: a non-handle userdata fails closed.
	if _, err := gh.MarshalUserData(&lua.LUserData{Value: 42}); err == nil || !strings.Contains(err.Error(), "not an api.Handle") {
		t.Fatalf("non-handle userdata must fail, got %v", err)
	}
	// Edge: a nil game cannot rebind.
	if _, err := (GameHandles{}).UnmarshalUserData(blob); err == nil || !strings.Contains(err.Error(), "nil game") {
		t.Fatalf("nil game unmarshal must fail, got %v", err)
	}
	t.Logf("FSV edges: non-handle userdata + nil-game rebind fail closed")
}

func TestGameHandlesCoroutineRoundTrip(t *testing.T) {
	const udSrc = `
		local u = injected_unit   -- a unit handle userdata
		coroutine.yield()
		return 1
	`
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	gh := GameHandles{G: g}

	ref := api.HandleRef{Kind: api.HandleUnit, Raw: 0x01000003}
	hd, _ := g.Resolve(ref)

	regHot := NewChunkRegistry()
	defer regHot.Close()
	cid, err := regHot.Register("world", udSrc)
	if err != nil {
		t.Fatalf("register: %v", err)
	}
	proto, err := regHot.ResolveProto(cid, "")
	if err != nil {
		t.Fatalf("resolve proto: %v", err)
	}
	rt := lua.NewState()
	ud := rt.NewUserData()
	ud.Value = hd
	rt.SetGlobal("injected_unit", ud)
	co, _ := rt.NewThread()
	if st, rerr, _ := rt.Resume(co, rt.NewFunctionFromProto(proto)); st != lua.ResumeYield || rerr != nil {
		t.Fatalf("expected yield, got state=%v err=%v", st, rerr)
	}

	// Save THROUGH the real game marshaler.
	img, err := SaveThread(regHot, co, gh)
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	blob, _ := json.Marshal(img)
	t.Logf("FSV coroutine artifact: %s", blob)

	// Cold reload through the game marshaler.
	var img2 ThreadImage
	if err := json.Unmarshal(blob, &img2); err != nil {
		t.Fatalf("unmarshal img: %v", err)
	}
	regCold := NewChunkRegistry()
	defer regCold.Close()
	if _, err := regCold.Register("world", udSrc); err != nil {
		t.Fatalf("cold register: %v", err)
	}
	rtCold := lua.NewState()
	th, topFn, err := LoadThread(regCold, rtCold, &img2, gh)
	if err != nil {
		t.Fatalf("LoadThread: %v", err)
	}

	// SoT: the reloaded register file holds a userdata whose api.Handle ref
	// matches the original — the host handle rebound against the live game.
	v := th.LitdSnapshot()
	found := false
	for _, sv := range v.Stack {
		if u, ok := sv.(*lua.LUserData); ok {
			if hv, ok := u.Value.(api.Handle); ok {
				got, _ := api.RefOf(hv)
				if got == ref {
					found = true
				}
			}
		}
	}
	if !found {
		t.Fatalf("reloaded thread has no register userdata resolving to ref %+v", ref)
	}
	t.Logf("FSV: coroutine's unit handle rebound to ref %+v after cold reload", ref)

	// Thread still resumes.
	st, rerr, vals := rtCold.Resume(th, topFn)
	if st != lua.ResumeOK || rerr != nil {
		t.Fatalf("cold resume failed: state=%v err=%v", st, rerr)
	}
	if len(vals) != 1 || vals[0] != lua.LNumber(1) {
		t.Fatalf("resume = %v, want [1]", vals)
	}
}
