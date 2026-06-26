package match

// spec.go — the declarative match descriptor (#635, ultimate-test-plan §6): the
// "custom game" spec a world ships as match.toml. It is parsed into a validated
// struct and is fail-closed: an unknown key, a missing required field, an
// out-of-range or duplicate slot, a bad enum, or a non-positive time limit is a
// hard error with a reason — never a silent default coercion (fsv.md,
// compression.md §6.1). Race existence against the world's data tables is a
// separate check (ValidateRaces) because the schema loader is data-free.
//
// Determinism: the player roster is an ordered slice keyed by slot (R-SIM-2); no
// map is ever iterated. Slot uniqueness is checked with a fixed-size bool array,
// not a map, so the loader has no map-order dependence at all.

import (
	"fmt"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// DefaultTimeLimitTicks is the never-ends backstop applied when match.toml omits
// time_limit_ticks: 20 in-game minutes at 20 ticks/s (tick = 50 ms). D4.
const DefaultTimeLimitTicks int64 = 24000

// VictoryMode is the win condition a match resolves on.
type VictoryMode uint8

const (
	// VictoryBeacon decides the match by beacon control.
	VictoryBeacon VictoryMode = iota
	// VictoryHall decides the match by town-hall destruction (last hall standing).
	VictoryHall
	// VictoryScore decides the match by the score tuple at the time limit.
	VictoryScore
)

// String renders the victory mode as its match.toml token.
func (v VictoryMode) String() string {
	switch v {
	case VictoryBeacon:
		return "beacon"
	case VictoryHall:
		return "hall"
	case VictoryScore:
		return "score"
	default:
		return "unknown"
	}
}

// parseVictory maps a match.toml token to a VictoryMode, failing closed on any
// unrecognized value.
func parseVictory(s string) (VictoryMode, error) {
	switch s {
	case "beacon":
		return VictoryBeacon, nil
	case "hall":
		return VictoryHall, nil
	case "score":
		return VictoryScore, nil
	default:
		return 0, fmt.Errorf("unknown victory %q (want beacon|hall|score)", s)
	}
}

// SlotController is who drives a player slot.
type SlotController uint8

const (
	// ControllerCPU is an autonomous AI player.
	ControllerCPU SlotController = iota
	// ControllerUser is a human player.
	ControllerUser
)

// String renders the controller as its match.toml token.
func (c SlotController) String() string {
	switch c {
	case ControllerCPU:
		return "cpu"
	case ControllerUser:
		return "user"
	default:
		return "unknown"
	}
}

// parseController maps a match.toml token to a SlotController, failing closed.
func parseController(s string) (SlotController, error) {
	switch s {
	case "cpu":
		return ControllerCPU, nil
	case "user":
		return ControllerUser, nil
	default:
		return 0, fmt.Errorf("unknown controller %q (want cpu|user)", s)
	}
}

// parseDifficulty maps a match.toml token to an api.Difficulty. An empty token
// defaults to normal; any other unrecognized value fails closed.
func parseDifficulty(s string) (api.Difficulty, error) {
	switch s {
	case "", "normal":
		return api.DifficultyNormal, nil
	case "easy":
		return api.DifficultyEasy, nil
	case "insane":
		return api.DifficultyInsane, nil
	default:
		return 0, fmt.Errorf("unknown difficulty %q (want easy|normal|insane)", s)
	}
}

// PlayerSpec is one roster entry: which faction/race plays slot Slot, who drives
// it, and (for a CPU) its difficulty and AI strategy table.
type PlayerSpec struct {
	Slot       int
	Race       string
	Controller SlotController
	Difficulty api.Difficulty
	AIStrategy string
}

// MatchSpec is the validated custom-game descriptor. Players is ordered by slot
// ascending (deterministic).
type MatchSpec struct {
	Seed           int64
	Victory        VictoryMode
	TimeLimitTicks int64
	Players        []PlayerSpec
}

// raw* are the on-disk shapes (enum fields are tokens); LoadMatchSpec converts
// them into the typed, validated MatchSpec.
type rawSpec struct {
	Seed           int64       `toml:"seed"`
	Victory        string      `toml:"victory"`
	TimeLimitTicks int64       `toml:"time_limit_ticks"`
	Players        []rawPlayer `toml:"players"`
}

type rawPlayer struct {
	Slot       int    `toml:"slot"`
	Race       string `toml:"race"`
	Controller string `toml:"controller"`
	Difficulty string `toml:"difficulty"`
	AIStrategy string `toml:"ai_strategy"`
}

// LoadMatchSpec parses and validates a match.toml blob into a MatchSpec, failing
// closed on the first defect (unknown key, missing/empty required field,
// out-of-range or duplicate slot, bad enum, non-positive time limit). It does
// NOT check that a race exists in the world's data tables — that is
// ValidateRaces, run where the data is available.
func LoadMatchSpec(blob []byte) (*MatchSpec, error) {
	var raw rawSpec
	md, err := toml.Decode(string(blob), &raw)
	if err != nil {
		return nil, fmt.Errorf("match: decode spec: %w", err)
	}
	if u := md.Undecoded(); len(u) > 0 {
		return nil, fmt.Errorf("match: unknown spec keys: %v", u)
	}

	victory, err := parseVictory(raw.Victory)
	if err != nil {
		return nil, fmt.Errorf("match: %w", err)
	}

	// time_limit_ticks: absent → default; present-but-non-positive → reject. The
	// IsDefined check is what lets us both default a missing key AND reject an
	// explicit 0/negative (a plain zero-value can't distinguish the two).
	timeLimit := DefaultTimeLimitTicks
	if md.IsDefined("time_limit_ticks") {
		if raw.TimeLimitTicks <= 0 {
			return nil, fmt.Errorf("match: time_limit_ticks must be > 0, got %d", raw.TimeLimitTicks)
		}
		timeLimit = raw.TimeLimitTicks
	}

	if len(raw.Players) == 0 {
		return nil, fmt.Errorf("match: spec has no players (need at least one)")
	}

	var seen [sim.MaxPlayers]bool
	players := make([]PlayerSpec, 0, len(raw.Players))
	for i, rp := range raw.Players {
		if rp.Slot < 0 || rp.Slot >= sim.MaxPlayers {
			return nil, fmt.Errorf("match: players[%d] slot %d out of range [0,%d)", i, rp.Slot, sim.MaxPlayers)
		}
		if seen[rp.Slot] {
			return nil, fmt.Errorf("match: players[%d] duplicate slot %d", i, rp.Slot)
		}
		seen[rp.Slot] = true
		if strings.TrimSpace(rp.Race) == "" {
			return nil, fmt.Errorf("match: players[%d] (slot %d) missing race", i, rp.Slot)
		}
		ctrl, err := parseController(rp.Controller)
		if err != nil {
			return nil, fmt.Errorf("match: players[%d] (slot %d): %w", i, rp.Slot, err)
		}
		diff, err := parseDifficulty(rp.Difficulty)
		if err != nil {
			return nil, fmt.Errorf("match: players[%d] (slot %d): %w", i, rp.Slot, err)
		}
		if ctrl == ControllerCPU && strings.TrimSpace(rp.AIStrategy) == "" {
			return nil, fmt.Errorf("match: players[%d] (slot %d) is cpu but has no ai_strategy", i, rp.Slot)
		}
		players = append(players, PlayerSpec{
			Slot:       rp.Slot,
			Race:       rp.Race,
			Controller: ctrl,
			Difficulty: diff,
			AIStrategy: rp.AIStrategy,
		})
	}
	// Deterministic order: ascending slot (the on-disk order is not trusted).
	sort.Slice(players, func(a, b int) bool { return players[a].Slot < players[b].Slot })

	return &MatchSpec{
		Seed:           raw.Seed,
		Victory:        victory,
		TimeLimitTicks: timeLimit,
		Players:        players,
	}, nil
}

// ValidateRaces fails closed if any roster race is not in known (the set of
// race/faction names the world's data tables define). The error names the first
// offending race. known is matched exactly (case-sensitive). This is the
// data-existence half of validation, kept out of the schema loader so spec.go
// stays data-free.
func (s *MatchSpec) ValidateRaces(known []string) error {
	if s == nil {
		return fmt.Errorf("match: nil spec")
	}
	set := make(map[string]bool, len(known))
	for _, r := range known {
		set[r] = true
	}
	for _, p := range s.Players {
		if !set[p.Race] {
			return fmt.Errorf("match: slot %d race %q not in data tables (known: %v)", p.Slot, p.Race, known)
		}
	}
	return nil
}

// Dump renders the spec as a stable multi-line string for FSV evidence/logs.
func (s *MatchSpec) Dump() string {
	if s == nil {
		return "<nil MatchSpec>"
	}
	var b strings.Builder
	fmt.Fprintf(&b, "MatchSpec{seed=%d victory=%s time_limit_ticks=%d players=%d}\n",
		s.Seed, s.Victory, s.TimeLimitTicks, len(s.Players))
	for _, p := range s.Players {
		fmt.Fprintf(&b, "  slot=%d race=%q controller=%s difficulty=%d ai_strategy=%q\n",
			p.Slot, p.Race, p.Controller, int(p.Difficulty), p.AIStrategy)
	}
	return b.String()
}
