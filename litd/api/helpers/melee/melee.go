// Package melee is the data-driven melee setup library — the D4 keep for
// blizzard.j's ~40-function melee BJ library (MeleeStartingResources,
// MeleeStartingUnits, MeleeStartingVisibility, the victory/defeat
// conditions). It reimplements that library PURELY on the public litd/api
// surface plus per-faction data tables, proving the melee game mode loses
// no power when expressed on the deduplicated API.
//
// There are deliberately NO hard-coded race functions (the WC3 BJs branched
// on a five-race enum). A faction is a TOML table (data/melee/<name>.toml)
// describing its starting resources and starting units; the same code runs
// every faction. This mirrors litd/ai/melee, which runs one controller over
// per-faction strategy tables.
//
// Imports: litd/api (the public surface), BurntSushi/toml, and the standard
// library — nothing else (the helpers no-power-lost gate).
package melee

import (
	"fmt"
	"os"
	"time"

	"github.com/BurntSushi/toml"
	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// Squad is a count of one unit type by its bound code (e.g. "hpea" x5).
type Squad struct {
	Code  string `toml:"code"`
	Count int    `toml:"count"`
}

// Faction is one playable melee race's start table. TownHall is the code of
// the first building dropped at the start location; Workers is the harvester
// squad; Extra is any additional starting units (scouts, a hero-altar, …).
// Gold/Lumber/FoodCap seed the starting economy.
type Faction struct {
	Name     string  `toml:"name"`
	Gold     int     `toml:"gold"`
	Lumber   int     `toml:"lumber"`
	FoodCap  int     `toml:"food_cap"`
	TownHall string  `toml:"town_hall"`
	Workers  Squad   `toml:"workers"`
	Extra    []Squad `toml:"extra"`
}

// LoadFaction reads and validates a faction table from a TOML file. A
// missing file, malformed TOML, an unknown key, or a table that fails
// validation is a loud error — never a silent default (R-FSV / fail-closed).
func LoadFaction(path string) (*Faction, error) {
	blob, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("melee: read faction %q: %w", path, err)
	}
	return LoadFactionBytes(blob)
}

// LoadFactionBytes parses and validates a faction table from raw TOML.
func LoadFactionBytes(blob []byte) (*Faction, error) {
	var f Faction
	md, err := toml.Decode(string(blob), &f)
	if err != nil {
		return nil, fmt.Errorf("melee: decode faction: %w", err)
	}
	if u := md.Undecoded(); len(u) > 0 {
		return nil, fmt.Errorf("melee: unknown faction keys: %v", u)
	}
	if err := f.validate(); err != nil {
		return nil, err
	}
	return &f, nil
}

// validate fails closed on a structurally unusable faction table.
func (f *Faction) validate() error {
	if f.Name == "" {
		return fmt.Errorf("melee: faction missing name")
	}
	if f.TownHall == "" {
		return fmt.Errorf("melee: faction %q missing town_hall code", f.Name)
	}
	if f.Gold < 0 || f.Lumber < 0 || f.FoodCap < 0 {
		return fmt.Errorf("melee: faction %q has negative resources (gold=%d lumber=%d food_cap=%d)",
			f.Name, f.Gold, f.Lumber, f.FoodCap)
	}
	if f.Workers.Count < 0 {
		return fmt.Errorf("melee: faction %q worker count negative", f.Name)
	}
	for i, e := range f.Extra {
		if e.Code == "" || e.Count < 0 {
			return fmt.Errorf("melee: faction %q extra[%d] invalid (code=%q count=%d)", f.Name, i, e.Code, e.Count)
		}
	}
	return nil
}

// SpawnHero creates the hero named by heroCode for p at pos facing the given
// angle, returning the hero unit. Unlike the forgiving Game.CreateUnit (which
// silently returns the zero Unit on a bad code, R-API-5), this fails CLOSED and
// LOUDLY: an unknown code, or a code that resolves to a non-hero unit type, is
// an error and NOTHING is spawned. That loud-on-typo behavior is what a
// start-script (human- or AI-authored) needs — a misspelled hero code must
// surface as a setup defect, not a missing hero discovered mid-match. The
// hero-ness is checked BEFORE creation (Game.IsHeroType) so the all-or-nothing
// contract holds: on error the unit count is unchanged.
func SpawnHero(g *litd.Game, p litd.Player, heroCode string, pos litd.Vec2, facing litd.Angle) (litd.Unit, error) {
	if g == nil {
		return litd.Unit{}, fmt.Errorf("melee: SpawnHero nil game")
	}
	t := g.UnitType(heroCode)
	if t.IsZero() {
		return litd.Unit{}, fmt.Errorf("melee: SpawnHero code %q not bound in unit table", heroCode)
	}
	if !g.IsHeroType(t) {
		return litd.Unit{}, fmt.Errorf("melee: SpawnHero code %q is not a hero type", heroCode)
	}
	u := g.CreateUnit(p, t, pos, facing)
	if !u.Valid() {
		return litd.Unit{}, fmt.Errorf("melee: SpawnHero %q create failed (unit cap or foreign owner)", heroCode)
	}
	return u, nil
}

// Setup binds one player slot to the faction it will play.
type Setup struct {
	Player  litd.Player
	Faction *Faction
}

// StartingResources seeds p's gold, lumber, and (when > 0) food cap from the
// faction table. D4 keep for MeleeStartingResources. Built on
// Player.SetGold/SetLumber/SetFoodCap.
func StartingResources(g *litd.Game, p litd.Player, f *Faction) {
	if g == nil || f == nil {
		return
	}
	p.SetGold(f.Gold)
	p.SetLumber(f.Lumber)
	if f.FoodCap > 0 {
		p.SetFoodCap(f.FoodCap)
	}
}

// StartingUnits drops the faction's town hall, worker squad, and any extra
// units at p's start location, returning every spawned unit in creation
// order. D4 keep for MeleeStartingUnits, built on Game.UnitType +
// Game.CreateUnit.
//
// It fails closed and LOUDLY (returns an error, spawning nothing) when a
// required unit code is not bound in the world's unit table — the "missing
// data table row" case. This is the deliberate non-silent-skip behavior:
// a faction whose town hall or a worker code is absent is a setup defect the
// caller must see, not a half-spawned base.
func StartingUnits(g *litd.Game, p litd.Player, f *Faction) ([]litd.Unit, error) {
	if g == nil {
		return nil, fmt.Errorf("melee: StartingUnits nil game")
	}
	if f == nil {
		return nil, fmt.Errorf("melee: StartingUnits nil faction")
	}
	// Resolve every code up front so a missing row aborts before any spawn
	// (all-or-nothing, no partial base).
	th := g.UnitType(f.TownHall)
	if th.IsZero() {
		return nil, fmt.Errorf("melee: faction %q town_hall code %q not bound in unit table", f.Name, f.TownHall)
	}
	var worker litd.UnitType
	if f.Workers.Count > 0 {
		worker = g.UnitType(f.Workers.Code)
		if worker.IsZero() {
			return nil, fmt.Errorf("melee: faction %q worker code %q not bound in unit table", f.Name, f.Workers.Code)
		}
	}
	extra := make([]litd.UnitType, len(f.Extra))
	for i, e := range f.Extra {
		t := g.UnitType(e.Code)
		if t.IsZero() {
			return nil, fmt.Errorf("melee: faction %q extra[%d] code %q not bound in unit table", f.Name, i, e.Code)
		}
		extra[i] = t
	}

	loc := p.StartLocation()
	facing := litd.Deg(270) // WC3 convention: starting units face "south"
	out := make([]litd.Unit, 0, 1+f.Workers.Count)

	u := g.CreateUnit(p, th, loc, facing)
	out = append(out, u)
	for i := 0; i < f.Workers.Count; i++ {
		out = append(out, g.CreateUnit(p, worker, loc, facing))
	}
	for i, e := range f.Extra {
		for j := 0; j < e.Count; j++ {
			out = append(out, g.CreateUnit(p, extra[i], loc, facing))
		}
	}
	return out, nil
}

// VictoryDefeatConditions installs the standard melee last-standing rule
// over players: each time any unit dies, every listed player with zero
// remaining units is staged defeated, and if exactly one listed player still
// has units it is staged victorious. D4 keep for the MeleeVictory/Defeat
// condition BJs. Built on Game.OnEvent(EventUnitDeath), Game.AllUnits (owner
// filter), Game.Victory/Defeat — the sim's phase-6 latch makes duplicate or
// second-winner requests no-ops, so re-evaluating on every death is safe.
//
// The conditions are also evaluated once immediately, so a setup that starts
// a player with no units defeats them at t=0 (matching WC3's initial check).
func VictoryDefeatConditions(g *litd.Game, players []litd.Player) {
	if g == nil || len(players) == 0 {
		return
	}
	// Copy the slice so a later mutation by the caller cannot change the
	// installed condition set.
	roster := append([]litd.Player(nil), players...)
	eval := func() { evaluate(g, roster) }
	// Re-evaluate on each unit death, but DEFERRED to the next tick: during
	// a death event the dying unit is still live (it counts in AllUnits), so
	// an immediate check would never see a player reach zero. g.After fires
	// next tick, by which point the kill has fully resolved and the unit is
	// gone from the store — the player whose last unit just died now reads
	// zero. (Multiple deaths in a tick schedule multiple deferred checks; the
	// sim's result latch makes the repeats idempotent.)
	g.OnEvent(litd.EventUnitDeath, func(litd.Event) { g.After(time.Millisecond, eval) })
	eval() // initial check: a player listed with no units is defeated at t=0
}

// evaluate stages defeat for empty players and victory for a lone survivor.
func evaluate(g *litd.Game, players []litd.Player) {
	survivors := make([]litd.Player, 0, len(players))
	for _, p := range players {
		if p.Result() != litd.ResultPlaying {
			continue // already decided — leave the latch alone
		}
		if PlayerUnitCount(g, p) == 0 {
			g.Defeat(p, "melee: no units remaining")
			continue
		}
		survivors = append(survivors, p)
	}
	if len(survivors) == 1 {
		g.Victory(survivors[0])
	}
}

// PlayerUnitCount returns how many live units p owns, via the public
// all-units query with an owner filter. Not a hot-path call (it runs on unit
// death / at setup), so the snapshot allocation is acceptable.
func PlayerUnitCount(g *litd.Game, p litd.Player) int {
	if g == nil {
		return 0
	}
	slot := p.Slot()
	n := 0
	for range g.AllUnits(func(v litd.UnitView) bool { return v.OwnerPlayer() == slot }) {
		n++
	}
	return n
}

// Standard runs the full melee setup for every Setup: starting resources,
// starting units, then the shared victory/defeat conditions over all listed
// players. D4 keep for MeleeInitVictoryDefeat + the standard melee init
// sequence. It returns the first StartingUnits error (a missing unit table
// row), having applied resources and spawned units for the setups processed
// before it — the error is surfaced, never swallowed, so the caller can
// abort match start.
func Standard(g *litd.Game, setups []Setup) error {
	if g == nil || len(setups) == 0 {
		return nil
	}
	players := make([]litd.Player, 0, len(setups))
	for _, s := range setups {
		StartingResources(g, s.Player, s.Faction)
		if _, err := StartingUnits(g, s.Player, s.Faction); err != nil {
			return err
		}
		players = append(players, s.Player)
	}
	VictoryDefeatConditions(g, players)
	return nil
}
