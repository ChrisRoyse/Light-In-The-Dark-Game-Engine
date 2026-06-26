package litd

// FSV for #480: buff lifecycle exposed as TRIGGER HOOKS (OnBuffApplied /
// OnBuffExpired / OnBuffRefreshed) on the ECA substrate, with aura children
// firing them too and the payload accessors (Event.Buff / BuffStacks /
// FromAura). The flagship example: a trigger that, on a stun being applied,
// DISABLES the target's "attack" trigger, and re-enables it when the stun
// expires (S6 — enable/disable of other triggers from a buff hook).
//
// SoT = the buff store (Unit.HasBuff / sim buff pool) joined with the reacting
// trigger's enabled flag (Trigger.IsEnabled), plus the state hash. Buffs are
// applied for REAL through Unit.ApplyBuff — no synthetic Emit.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// buff type indices in the table below.
const (
	btStun     = 0 // 0.5 s refresh stun
	btCmdChild = 1 // aura child (move-speed)
	btCommand  = 2 // aura that maintains cmd-child on allies in radius
)

func hookBuffTypes() []data.BuffType {
	return []data.BuffType{
		{ID: "stun", DurationTicks: 10, Stacking: data.StackRefresh},
		{ID: "cmd-child", DurationTicks: 20, Stacking: data.StackRefresh,
			Mods: []data.StatMod{{Stat: data.StatMoveSpeed, Add: 0, Permille: 1500}}},
		{ID: "command", DurationTicks: 2000, Stacking: data.StackRefresh,
			AuraRadius: fixed.FromInt(200), AuraChild: btCmdChild, AuraLingerTicks: 20},
	}
}

// hookWorld builds a world + game with the buff types bound and two same-team
// units in aura radius. Construction order is fixed so two calls produce
// identical entity ids / trigger refs (needed for the save/load round-trip).
func hookWorld(t *testing.T) (*sim.World, *Game, Unit, Unit) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8, BuffInstances: 32, Triggers: 16, PendingEvents: 64})
	if !w.BindBuffTypes(hookBuffTypes()) {
		t.Fatal("BindBuffTypes failed")
	}
	g := newGame(w)
	mk := func(x int32) Unit {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(1000)}, 0)
		if !ok || !w.Owners.Add(w.Ents, id, 0, 0, 0) ||
			!w.Healths.Add(w.Ents, id, fixed.FromInt(100), 0, 0, 0) {
			t.Fatal("unit setup failed")
		}
		return Unit{id: id, g: g}
	}
	carrier := mk(1000)
	ally := mk(1100) // 100 < 200 radius
	return w, g, carrier, ally
}

// TestBuffHookStunDisablesAttackFSV — the example: OnBuffApplied(stun) disables
// the attack trigger; OnBuffExpired(stun) re-enables it. SoT printed before/
// after across the window: buff presence + the attack trigger's enabled flag.
func TestBuffHookStunDisablesAttackFSV(t *testing.T) {
	_, g, _, target := hookWorld(t)
	stun := g.BuffType("stun")

	// the target's "attack" behavior as a trigger; its enabled flag is the SoT.
	attack := g.NewTrigger().On(EventAttackLaunch).Do(func(Event) {})

	var applied, expired int
	g.NewTrigger().On(EventBuffApplied).
		WhenEvent(func(e Event) bool { return e.Buff() == stun }).
		Do(func(e Event) { applied++; attack.Disable() })
	g.NewTrigger().On(EventBuffExpired).
		WhenEvent(func(e Event) bool { return e.Buff() == stun }).
		Do(func(e Event) { expired++; attack.Enable() })

	t.Logf("BEFORE: target hasStun=%v attackEnabled=%v applied=%d expired=%d",
		target.HasBuff(stun), attack.IsEnabled(), applied, expired)
	if !attack.IsEnabled() {
		t.Fatal("attack trigger should start enabled")
	}

	if !target.ApplyBuff(stun).Valid() {
		t.Fatal("ApplyBuff(stun) failed")
	}
	g.Advance(1) // dispatch the queued EvBuffApplied
	t.Logf("AFTER apply: target hasStun=%v attackEnabled=%v applied=%d",
		target.HasBuff(stun), attack.IsEnabled(), applied)
	if applied != 1 {
		t.Fatalf("OnBuffApplied(stun) fired %d times, want 1", applied)
	}
	if attack.IsEnabled() {
		t.Fatal("attack trigger not disabled by the stun hook")
	}
	if !target.HasBuff(stun) {
		t.Fatal("stun not in the buff store after apply")
	}

	// run past the 10-tick stun duration.
	g.Advance(15)
	t.Logf("AFTER expiry: target hasStun=%v attackEnabled=%v expired=%d",
		target.HasBuff(stun), attack.IsEnabled(), expired)
	if expired != 1 {
		t.Fatalf("OnBuffExpired(stun) fired %d times, want 1", expired)
	}
	if !attack.IsEnabled() {
		t.Fatal("attack trigger not re-enabled on stun expiry")
	}
	if target.HasBuff(stun) {
		t.Fatal("stun still present after expiry")
	}
}

// TestBuffHookAuraChildFSV — edge 1: an aura child apply fires OnBuffApplied
// with Event.FromAura() == true and Event.Buff() == the child type.
func TestBuffHookAuraChildFSV(t *testing.T) {
	w, g, carrier, ally := hookWorld(t)
	command := g.BuffType("command")
	cmdChild := g.BuffType("cmd-child")

	var auraChildHits, directHits int
	g.OnBuffApplied(func(e Event) {
		if e.FromAura() {
			auraChildHits++
			t.Logf("aura-child applied: unit=%#x buff=cmd-child? %v fromAura=%v",
				uint32(e.Unit().id), e.Buff() == cmdChild, e.FromAura())
		} else {
			directHits++
		}
	})

	if !carrier.ApplyBuff(command).Valid() {
		t.Fatal("ApplyBuff(command) failed")
	}
	g.Advance(25) // aura eval is throttled; give it room

	t.Logf("FSV #480 aura: directHits=%d auraChildHits=%d allyHasChild=%v",
		directHits, auraChildHits, ally.HasBuff(cmdChild))
	if directHits < 1 {
		t.Fatal("the carrier's own command apply did not fire a non-aura OnBuffApplied")
	}
	if auraChildHits < 1 {
		t.Fatal("no aura-child OnBuffApplied with FromAura()==true")
	}
	// SoT: the child instance really exists on the ally.
	if !ally.HasBuff(cmdChild) {
		t.Fatal("hook fired but the ally carries no cmd-child instance")
	}
	_ = w
}

// TestBuffHookRefreshNoDoubleDisableFSV — edge 2: re-applying the stun while it
// is live fires OnBuffRefreshed (not a second OnBuffApplied); the attack trigger
// stays disabled and is not spuriously toggled.
func TestBuffHookRefreshNoDoubleDisableFSV(t *testing.T) {
	_, g, _, target := hookWorld(t)
	stun := g.BuffType("stun")
	attack := g.NewTrigger().On(EventAttackLaunch).Do(func(Event) {})

	var applied, refreshed, disables int
	g.NewTrigger().On(EventBuffApplied).
		WhenEvent(func(e Event) bool { return e.Buff() == stun }).
		Do(func(e Event) { applied++; disables++; attack.Disable() })
	g.OnBuffRefreshed(func(e Event) {
		if e.Buff() == stun {
			refreshed++
		}
	})

	target.ApplyBuff(stun)
	g.Advance(3) // stun still live (dur 10)
	target.ApplyBuff(stun) // refresh
	g.Advance(1)

	t.Logf("FSV #480 refresh: applied=%d refreshed=%d disables=%d attackEnabled=%v hasStun=%v",
		applied, refreshed, disables, attack.IsEnabled(), target.HasBuff(stun))
	if applied != 1 {
		t.Fatalf("OnBuffApplied fired %d times across a refresh, want 1", applied)
	}
	if refreshed != 1 {
		t.Fatalf("OnBuffRefreshed fired %d times, want 1", refreshed)
	}
	if disables != 1 {
		t.Fatalf("attack disabled %d times (double-disable bug), want 1", disables)
	}
	if attack.IsEnabled() {
		t.Fatal("attack trigger should remain disabled across the refresh")
	}
}

// buildSaveScenario applies a stun, advances partway (stun still live, attack
// disabled), and returns the game so the caller can save it. Construction is
// identical on every call so save/load entity ids and trigger refs line up.
func buildSaveScenario(t *testing.T, applyStun bool) (*Game, Unit, Trigger, sim.EntityID) {
	t.Helper()
	_, g, _, target := hookWorld(t)
	stun := g.BuffType("stun")
	attack := g.NewTrigger().On(EventAttackLaunch).Do(func(Event) {})
	g.NewTrigger().On(EventBuffApplied).
		WhenEvent(func(e Event) bool { return e.Buff() == stun }).
		Do(func(e Event) { attack.Disable() })
	g.NewTrigger().On(EventBuffExpired).
		WhenEvent(func(e Event) bool { return e.Buff() == stun }).
		Do(func(e Event) { attack.Enable() })
	if applyStun {
		target.ApplyBuff(stun)
		g.Advance(4) // dispatched + a few ticks of stun elapsed
	}
	return g, target, attack, target.id
}

// TestBuffHookSaveResumeFSV — edge 3: saving mid-stun and loading into a freshly
// rebuilt world (same triggers re-registered) resumes the buff AND the disabled
// state with an identical hash.
func TestBuffHookSaveResumeFSV(t *testing.T) {
	g, target, attack, _ := buildSaveScenario(t, true)
	stun := g.BuffType("stun")
	if !target.HasBuff(stun) || attack.IsEnabled() {
		t.Fatalf("precondition: hasStun=%v attackEnabled=%v (want true/false)", target.HasBuff(stun), attack.IsEnabled())
	}
	srcHash := g.StateHash()
	var buf bytes.Buffer
	if err := g.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	// fresh world, triggers rebuilt in the same order (so refs match), no stun
	// applied — LoadState restores the buff store + trigger enabled flags.
	g2, target2, attack2, _ := buildSaveScenario(t, false)
	if err := g2.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	dstHash := g2.StateHash()
	stun2 := g2.BuffType("stun")
	t.Logf("FSV #480 save/resume: srcHash=%#016x dstHash=%#016x | hasStun=%v attackEnabled=%v",
		srcHash, dstHash, target2.HasBuff(stun2), attack2.IsEnabled())
	if dstHash != srcHash {
		t.Fatalf("post-load hash %#x != pre-save %#x", dstHash, srcHash)
	}
	if !target2.HasBuff(stun2) {
		t.Fatal("stun did not resume after load")
	}
	if attack2.IsEnabled() {
		t.Fatal("disabled attack trigger did not resume disabled after load")
	}
}

// TestBuffHookDeterminismFSV — edge 4: two identical hook-driven runs hash
// identically.
func TestBuffHookDeterminismFSV(t *testing.T) {
	run := func() uint64 {
		_, g, _, target := hookWorld(t)
		stun := g.BuffType("stun")
		attack := g.NewTrigger().On(EventAttackLaunch).Do(func(Event) {})
		g.NewTrigger().On(EventBuffApplied).
			WhenEvent(func(e Event) bool { return e.Buff() == stun }).
			Do(func(e Event) { attack.Disable() })
		g.NewTrigger().On(EventBuffExpired).
			WhenEvent(func(e Event) bool { return e.Buff() == stun }).
			Do(func(e Event) { attack.Enable() })
		target.ApplyBuff(stun)
		g.Advance(20)
		return g.StateHash()
	}
	a, b := run(), run()
	t.Logf("FSV #480 determinism: run1=%#016x run2=%#016x", a, b)
	if a != b {
		t.Fatalf("buff-hook run nondeterministic: %#x != %#x", a, b)
	}
}
