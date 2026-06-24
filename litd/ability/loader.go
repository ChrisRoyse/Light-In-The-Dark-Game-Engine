// Package ability loads composable-ability spec files (PRD2 06, epic #549,
// #600) from TOML into the sim's AbilitySpecSource. The on-disk format mirrors
// ability-model.md §1: an [ability] table with identity/gating, an
// [ability.timing] sub-table, an array of [[ability.on_cast]] ops (with an
// inline fx={...} on attach_mover), and named [ability.effects.NAME] effect
// lists. The loader does NOT compile effect lists into the engine arena (that
// is the host's data-load step); it produces the source spec plus the set of
// names the spec declares and references, so a validator (tools/abilitycheck,
// #598) can confirm every reference resolves.
package ability

import (
	"fmt"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Template is a loaded ability spec plus the names it declares/references.
type Template struct {
	Source      data.AbilitySpecSource
	EffectLists []string // names declared under [ability.effects.*]
	RefEffects  []string // effect-list names referenced by ops (run_effects / fx.effects)
	RefMovers   []string // mover-kind names referenced by attach_mover
	RefEvents   []string // custom-event names referenced by emit_event
	RefProjs    []string // projectile names referenced by spawn_projectile
}

// ---- raw TOML shapes ----

type rawFile struct {
	Ability rawAbility `toml:"ability"`
}

type rawAbility struct {
	ID        string                   `toml:"id"`
	Name      string                   `toml:"name"`
	CastType  string                   `toml:"cast_type"`
	Indicator string                   `toml:"indicator"`
	CastRange float64                  `toml:"cast_range"`
	ManaCost  int                      `toml:"mana_cost"`
	Cooldown  float64                  `toml:"cooldown"`
	Timing    rawTiming                `toml:"timing"`
	OnCast    []rawOp                  `toml:"on_cast"`
	Effects   map[string]rawEffectList `toml:"effects"`
}

type rawTiming struct {
	Precast   float64 `toml:"precast"`
	CastPoint float64 `toml:"cast_point"`
	Backswing float64 `toml:"backswing"`
}

type rawOp struct {
	Op         string  `toml:"op"`
	Mover      string  `toml:"mover"`
	Projectile string  `toml:"projectile"`
	Effects    string  `toml:"effects"`
	Event      string  `toml:"event"`
	Key        string  `toml:"key"`
	Cont       uint16  `toml:"cont"`
	Speed      float64       `toml:"speed"`
	Range      float64       `toml:"range"`
	Radius     float64       `toml:"radius"`
	Amount     float64       `toml:"amount"`
	Arg        int64         `toml:"arg"`
	Count      int           `toml:"count"`
	Hit        string        `toml:"hit"`
	Pierce     int           `toml:"pierce"`
	AngVel     float64       `toml:"angvel"`
	TurnRate   float64       `toml:"turn_rate"`
	Height     float64       `toml:"height"`
	Decay      int           `toml:"decay"`
	Done       string        `toml:"done"`
	Waypoints  [][2]float64  `toml:"waypoints"`
	Fx         *rawFx        `toml:"fx"`
	Children   []rawOp       `toml:"children"`
}

type rawFx struct {
	Hit     string  `toml:"hit"`
	Radius  float64 `toml:"radius"`
	Pierce  int     `toml:"pierce"`
	Done    string  `toml:"done"`
	Effects string  `toml:"effects"`
}

type rawEffectList struct {
	Ops []map[string]interface{} `toml:"ops"`
}

// LoadTOML decodes one ability spec file. Fail-closed: a TOML syntax error, an
// empty id, or an unknown op is an error — never a partial template.
func LoadTOML(blob []byte) (Template, error) {
	var rf rawFile
	if _, err := toml.Decode(string(blob), &rf); err != nil {
		return Template{}, fmt.Errorf("ability: TOML decode: %w", err)
	}
	a := rf.Ability
	if a.ID == "" {
		return Template{}, fmt.Errorf("ability: missing [ability].id")
	}
	t := Template{
		Source: data.AbilitySpecSource{
			ID: a.ID, Name: a.Name, CastType: a.CastType, Indicator: a.Indicator,
			CastRange: a.CastRange, ManaCost: a.ManaCost, Cooldown: a.Cooldown,
			Precast: a.Timing.Precast, CastPoint: a.Timing.CastPoint, Backswing: a.Timing.Backswing,
		},
	}
	for name := range a.Effects {
		t.EffectLists = append(t.EffectLists, name)
	}
	sort.Strings(t.EffectLists)

	ops, err := convertOps(a.OnCast, &t)
	if err != nil {
		return Template{}, err
	}
	t.Source.OnCast = ops
	dedupeSort(&t.RefEffects)
	dedupeSort(&t.RefMovers)
	dedupeSort(&t.RefEvents)
	dedupeSort(&t.RefProjs)
	return t, nil
}

func convertOps(raw []rawOp, t *Template) ([]data.OpSource, error) {
	if len(raw) == 0 {
		return nil, nil
	}
	out := make([]data.OpSource, 0, len(raw))
	for i := range raw {
		r := &raw[i]
		if r.Op == "" {
			return nil, fmt.Errorf("ability %q: op[%d] missing 'op'", t.Source.ID, i)
		}
		o := data.OpSource{
			Op: r.Op, Mover: r.Mover, Effects: r.Effects, Event: r.Event, Key: r.Key,
			Cont: r.Cont, Speed: r.Speed, Range: r.Range, Radius: r.Radius, Amount: r.Amount,
			Arg: r.Arg, Count: r.Count, Pierce: r.Pierce, HitMask: parseHitMask(r.Hit),
			AngVel: r.AngVel, TurnRate: r.TurnRate, Height: r.Height, Decay: r.Decay, Done: r.Done,
		}
		for _, wp := range r.Waypoints {
			o.Waypoints = append(o.Waypoints, data.WaypointSource{X: wp[0], Y: wp[1]})
		}
		// attach_mover folds the inline fx table into the op fields.
		if r.Fx != nil {
			if r.Fx.Effects != "" {
				o.Effects = r.Fx.Effects
			}
			if r.Fx.Radius != 0 {
				o.Radius = r.Fx.Radius
			}
			if r.Fx.Pierce != 0 {
				o.Pierce = r.Fx.Pierce
			}
			if r.Fx.Done != "" {
				o.Done = r.Fx.Done
			}
			if m := parseHitMask(r.Fx.Hit); m != 0 {
				o.HitMask = m
			}
		}
		// record referenced names for the validator
		if o.Mover != "" {
			t.RefMovers = append(t.RefMovers, o.Mover)
		}
		if o.Effects != "" {
			t.RefEffects = append(t.RefEffects, o.Effects)
		}
		if o.Event != "" {
			t.RefEvents = append(t.RefEvents, o.Event)
		}
		if r.Projectile != "" {
			t.RefProjs = append(t.RefProjs, r.Projectile)
		}
		kids, err := convertOps(r.Children, t)
		if err != nil {
			return nil, err
		}
		o.Children = kids
		out = append(out, o)
	}
	return out, nil
}

// parseHitMask converts a comma-separated hit string ("enemy,ground") into the
// missile-style mask bits. Unknown tokens are ignored (the validator's
// range/type pass reports them). Empty → 0 (compiler defaults to enemy-only).
func parseHitMask(s string) uint16 {
	if s == "" {
		return 0
	}
	var mask uint16
	for _, tok := range strings.Split(s, ",") {
		switch strings.TrimSpace(tok) {
		case "enemy":
			mask |= sim.MissileHitEnemy
		case "ally":
			mask |= sim.MissileHitAlly
		case "ground":
			mask |= sim.MissileHitGround
		case "air":
			mask |= sim.MissileHitAir
		case "structure":
			mask |= sim.MissileHitStructure
		}
	}
	return mask
}

func dedupeSort(s *[]string) {
	if len(*s) == 0 {
		return
	}
	sort.Strings(*s)
	out := (*s)[:1]
	for _, v := range (*s)[1:] {
		if v != out[len(out)-1] {
			out = append(out, v)
		}
	}
	*s = out
}
