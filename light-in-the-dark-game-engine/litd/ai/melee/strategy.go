// Package melee is the reference melee AIController (#282; D4: AI strategy is
// content, not API). It dogfoods the AI-domain native families (#277–281) —
// economy/harvest, build orders, unit production, attack waves — with ZERO
// imports of litd/sim or litd/api: it speaks only the litd/ai capability
// surfaces (AIView + the EconomyControl/ProductionControl/WaveSource native
// families), so it is provably inside the R-EXEC-3 isolation boundary. A sim
// adapter satisfies those surfaces at the integration boundary (in the harness
// / api), never here.
//
// Strategy — build orders, economy ramp, army size, wave timing — is data: a
// per-faction TOML table consumed by the one Go controller. Switching Vigil to
// Unbound changes table values, never controller code.
package melee

import (
	"fmt"

	"github.com/BurntSushi/toml"
)

// Strategy is one faction's melee plan. Unit references are integer type ids
// (the def indices the match registers), so the controller never needs a
// sim-side name table and stays decoupled from litd/sim.
type Strategy struct {
	Name    string      `toml:"name"`
	Economy EconomyPlan `toml:"economy"`
	Army    ArmyPlan    `toml:"army"`
	Waves   WavePlan    `toml:"waves"`
	Build   []BuildStep `toml:"build"`
}

// EconomyPlan is the harvester ramp. The per-difficulty percentages scale the
// worker and army targets (the AIDifficulty knob, data-driven — no branching
// controller logic): easy fields a smaller economy, insane a larger one.
type EconomyPlan struct {
	GoldWorkers int `toml:"gold_workers"`
	WoodWorkers int `toml:"wood_workers"`
	EasyPct     int `toml:"easy_pct"`
	NormalPct   int `toml:"normal_pct"`
	InsanePct   int `toml:"insane_pct"`
}

// ArmyPlan is the standing-army target. SoldierType is the trained unit's type
// id; Maintain is the population the controller keeps producing toward.
type ArmyPlan struct {
	SoldierType int `toml:"soldier_type"`
	Maintain    int `toml:"maintain"`
}

// WavePlan is the attack trigger: launch a wave once Size soldiers are ready.
type WavePlan struct {
	Size int `toml:"size"`
}

// BuildStep is one structure quota in the build order, in priority order.
type BuildStep struct {
	Type  int `toml:"type"`
	Count int `toml:"count"`
}

// Difficulty tiers (mirror the public Difficulty enum values without importing
// the api: this layer is sim/api-free).
const (
	DiffEasy   = 0
	DiffNormal = 1
	DiffInsane = 2
)

// EconPct returns the economy/army scaling percentage for difficulty d,
// defaulting to 100 when a tier is left unset in the table.
func (s *Strategy) EconPct(d int) int {
	var p int
	switch d {
	case DiffEasy:
		p = s.Economy.EasyPct
	case DiffInsane:
		p = s.Economy.InsanePct
	default:
		p = s.Economy.NormalPct
	}
	if p <= 0 {
		return 100
	}
	return p
}

// scale applies a percentage to n, rounding down, with a floor of 1 when n>0 so
// a difficulty knob never zeroes out a non-empty target.
func scale(n, pct int) int {
	if n <= 0 {
		return 0
	}
	v := n * pct / 100
	if v < 1 {
		v = 1
	}
	return v
}

// GoldWorkerTarget / WoodWorkerTarget / ArmyTarget apply the difficulty scale.
func (s *Strategy) GoldWorkerTarget(d int) int { return scale(s.Economy.GoldWorkers, s.EconPct(d)) }
func (s *Strategy) WoodWorkerTarget(d int) int { return scale(s.Economy.WoodWorkers, s.EconPct(d)) }
func (s *Strategy) ArmyTarget(d int) int       { return scale(s.Army.Maintain, s.EconPct(d)) }

// LoadStrategy reads a faction TOML file. Fail-closed: a missing file, parse
// error, or a strategy that is structurally unusable (no army type, empty build
// order) is an error, never a silent default.
func LoadStrategy(path string) (*Strategy, error) {
	var s Strategy
	md, err := toml.DecodeFile(path, &s)
	if err != nil {
		return nil, fmt.Errorf("melee: load strategy %q: %w", path, err)
	}
	if undec := md.Undecoded(); len(undec) > 0 {
		return nil, fmt.Errorf("melee: strategy %q has unknown keys: %v", path, undec)
	}
	if err := s.validate(); err != nil {
		return nil, fmt.Errorf("melee: strategy %q invalid: %w", path, err)
	}
	return &s, nil
}

// LoadStrategyBytes parses a strategy from an in-memory TOML blob (tests / edge
// edits). Same fail-closed validation as LoadStrategy.
func LoadStrategyBytes(blob []byte) (*Strategy, error) {
	var s Strategy
	md, err := toml.Decode(string(blob), &s)
	if err != nil {
		return nil, fmt.Errorf("melee: decode strategy: %w", err)
	}
	if undec := md.Undecoded(); len(undec) > 0 {
		return nil, fmt.Errorf("melee: strategy has unknown keys: %v", undec)
	}
	if err := s.validate(); err != nil {
		return nil, fmt.Errorf("melee: strategy invalid: %w", err)
	}
	return &s, nil
}

func (s *Strategy) validate() error {
	if s.Name == "" {
		return fmt.Errorf("missing name")
	}
	if s.Army.Maintain <= 0 {
		return fmt.Errorf("army.maintain must be > 0")
	}
	if s.Waves.Size <= 0 {
		return fmt.Errorf("waves.size must be > 0")
	}
	if len(s.Build) == 0 {
		return fmt.Errorf("build order is empty")
	}
	for i, b := range s.Build {
		if b.Count <= 0 {
			return fmt.Errorf("build[%d] count must be > 0", i)
		}
	}
	return nil
}
