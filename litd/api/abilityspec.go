package litd

// Composable-ability authoring surface (PRD2 06, epic #549, #599). A Go author
// builds an ability as DATA — a spec that composes the five primitives by name
// — and registers it with one call. The Lua surface (luabind) mirrors this
// exactly, so an AI or human author reaches abilities identically from either
// language (R-ABL-5): same names, same params, same compiled spec, same cast.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// AbilityOpDef is one author-facing composition op. Numbers are public
// (float64 world units / int counts); references are names resolved at
// registration. Mirrors the op vocabulary (spawn_projectile, attach_mover,
// fill_group, run_effects, emit_event, set_kv, get_kv, after, loop, times,
// for_each_in_group, if). Children carry nested ops for after/loop/times/
// for_each_in_group/if.
type AbilityOpDef struct {
	Op string

	Mover   string // attach_mover: kind name (linear/homing/point/...)
	Effects string // run_effects / mover payload: effect-list name
	Event   string // emit_event: custom-event name
	Key     string // set_kv/get_kv/if: KV key name
	Cont    uint16 // custom mover step / cont id

	Speed  float64
	Range  float64
	Radius float64
	Amount float64
	Arg    int64

	Count   int
	HitMask uint16
	Pierce  int

	// Extended mover params (#622): AngVel/TurnRate in degrees-per-tick,
	// Decay per-mille, Done one of expire/loop/detonate/cont, Waypoints world
	// units (spline control points).
	AngVel    float64
	TurnRate  float64
	Height    float64
	Decay     int
	Done      string
	Waypoints [][2]float64

	Children []AbilityOpDef
}

// AbilitySpecDef is the author-facing composable ability. Durations are
// float64 seconds (quantized to ticks at registration); CastRange is world
// units. CastType/Indicator are the lowercase names ("active", "line", ...).
type AbilitySpecDef struct {
	ID        string
	Name      string
	CastType  string
	Indicator string
	CastRange float64
	ManaCost  int
	Cooldown  float64

	Precast   float64
	CastPoint float64
	Backswing float64

	OnCast []AbilityOpDef
}

// RegisterAbilitySpec compiles and registers a composable ability, returning
// its ref. The error is non-nil (and the ref zero) when a reference does not
// resolve or a number is out of range — the same fail-closed validation the
// Lua surface and abilitycheck use. Effect lists, custom events, and custom
// mover steps the spec names must already be registered at setup.
func (g *Game) RegisterAbilitySpec(def AbilitySpecDef) (AbilityRef, error) {
	if g == nil || g.w == nil {
		return 0, errNoGame{}
	}
	src := data.AbilitySpecSource{
		ID: def.ID, Name: def.Name, CastType: def.CastType, Indicator: def.Indicator,
		CastRange: def.CastRange, ManaCost: def.ManaCost, Cooldown: def.Cooldown,
		Precast: def.Precast, CastPoint: def.CastPoint, Backswing: def.Backswing,
		OnCast: opDefsToSource(def.OnCast),
	}
	// Lower the float authoring numbers to fixed-point (litd/data, #628) before
	// handing the spec to the deterministic sim core, which never sees a float.
	lowered, err := data.LowerAbilitySpec(src)
	if err != nil {
		g.reportInvalid("Game.RegisterAbilitySpec (" + err.Error() + ")")
		return 0, err
	}
	ref, err := g.w.RegisterAbilitySpecAuto(lowered)
	if err != nil {
		g.reportInvalid("Game.RegisterAbilitySpec (" + err.Error() + ")")
		return 0, err
	}
	return AbilityRef(ref), nil
}

// RegisterEffectListName binds a compiled effect list (by arena offset/length)
// to an author name so composable abilities can reference it. Returns false on
// an empty name or an out-of-range span. Setup-time.
func (g *Game) RegisterEffectListName(name string, offset, length uint16) bool {
	if g == nil || g.w == nil {
		return false
	}
	return g.w.RegisterEffectListName(name, sim.EffectListSpan(offset, length))
}

func opDefsToSource(ops []AbilityOpDef) []data.OpSource {
	if len(ops) == 0 {
		return nil
	}
	out := make([]data.OpSource, len(ops))
	for i := range ops {
		o := &ops[i]
		out[i] = data.OpSource{
			Op: o.Op, Mover: o.Mover, Effects: o.Effects, Event: o.Event, Key: o.Key,
			Cont: o.Cont, Speed: o.Speed, Range: o.Range, Radius: o.Radius, Amount: o.Amount,
			Arg: o.Arg, Count: o.Count, HitMask: o.HitMask, Pierce: o.Pierce,
			AngVel: o.AngVel, TurnRate: o.TurnRate, Height: o.Height, Decay: o.Decay, Done: o.Done,
			Children: opDefsToSource(o.Children),
		}
		for _, wp := range o.Waypoints {
			out[i].Waypoints = append(out[i].Waypoints, data.WaypointSource{X: wp[0], Y: wp[1]})
		}
	}
	return out
}

type errNoGame struct{}

func (errNoGame) Error() string { return "litd: no game" }
