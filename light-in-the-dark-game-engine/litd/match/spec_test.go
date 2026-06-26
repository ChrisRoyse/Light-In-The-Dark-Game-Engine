package match_test

// Full State Verification for the MatchSpec loader (#635, ultimate-test-plan
// Phase 0). SoT = the parsed MatchSpec value + the verbatim rejection messages.
// Happy path: load a valid match.toml and assert every field matches the file
// byte-for-byte. Edges: missing players, unknown race (ValidateRaces), and a
// non-positive time limit — each a loud refusal with no struct mutation.

import (
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/match"
)

const happySpec = `
seed = 42
victory = "score"
time_limit_ticks = 12000

[[players]]
slot = 1
race = "vigil"
controller = "cpu"
difficulty = "insane"
ai_strategy = "data/ai/vigil.toml"

[[players]]
slot = 0
race = "unbound"
controller = "cpu"
difficulty = "normal"
ai_strategy = "data/ai/unbound.toml"
`

func TestMatchSpecHappyPathFSV(t *testing.T) {
	spec, err := match.LoadMatchSpec([]byte(happySpec))
	if err != nil {
		t.Fatalf("LoadMatchSpec(valid) raised: %v", err)
	}
	t.Logf("FSV happy-path struct dump:\n%s", spec.Dump())

	// Every field matches the file byte-for-byte.
	if spec.Seed != 42 {
		t.Fatalf("seed=%d, want 42", spec.Seed)
	}
	if spec.Victory != match.VictoryScore {
		t.Fatalf("victory=%s, want score", spec.Victory)
	}
	if spec.TimeLimitTicks != 12000 {
		t.Fatalf("time_limit_ticks=%d, want 12000", spec.TimeLimitTicks)
	}
	if len(spec.Players) != 2 {
		t.Fatalf("players=%d, want 2", len(spec.Players))
	}
	// Roster is sorted by slot ascending (deterministic) — unbound(slot 0) first.
	if spec.Players[0].Slot != 0 || spec.Players[0].Race != "unbound" {
		t.Fatalf("players[0] = slot %d race %q, want slot 0 race unbound", spec.Players[0].Slot, spec.Players[0].Race)
	}
	if spec.Players[0].Controller != match.ControllerCPU || spec.Players[0].Difficulty != api.DifficultyNormal {
		t.Fatalf("players[0] controller/difficulty = %s/%d, want cpu/normal", spec.Players[0].Controller, int(spec.Players[0].Difficulty))
	}
	if spec.Players[0].AIStrategy != "data/ai/unbound.toml" {
		t.Fatalf("players[0] ai_strategy=%q", spec.Players[0].AIStrategy)
	}
	if spec.Players[1].Slot != 1 || spec.Players[1].Race != "vigil" {
		t.Fatalf("players[1] = slot %d race %q, want slot 1 race vigil", spec.Players[1].Slot, spec.Players[1].Race)
	}
	if spec.Players[1].Difficulty != api.DifficultyInsane {
		t.Fatalf("players[1] difficulty=%d, want insane(%d)", int(spec.Players[1].Difficulty), int(api.DifficultyInsane))
	}

	// time_limit_ticks default applies when the key is absent.
	noLimit := `
victory = "hall"
[[players]]
slot = 0
race = "vigil"
controller = "user"
`
	spec2, err := match.LoadMatchSpec([]byte(noLimit))
	if err != nil {
		t.Fatalf("LoadMatchSpec(no time limit) raised: %v", err)
	}
	if spec2.TimeLimitTicks != match.DefaultTimeLimitTicks {
		t.Fatalf("absent time_limit_ticks defaulted to %d, want %d", spec2.TimeLimitTicks, match.DefaultTimeLimitTicks)
	}
	t.Logf("FSV default time limit: absent → %d ticks (20 in-game min)", spec2.TimeLimitTicks)

	// ValidateRaces happy: every roster race is known.
	if err := spec.ValidateRaces([]string{"vigil", "unbound"}); err != nil {
		t.Fatalf("ValidateRaces(known) raised: %v", err)
	}
	t.Logf("FSV ValidateRaces: vigil+unbound both in data tables → OK")
}

func TestMatchSpecRejectionsFSV(t *testing.T) {
	// Edge 1: missing players → loud error, no struct returned.
	noPlayers := `seed = 1
victory = "score"`
	spec, err := match.LoadMatchSpec([]byte(noPlayers))
	t.Logf("EDGE missing-players: spec=%v err=%v", spec, err)
	if err == nil || spec != nil {
		t.Fatalf("missing players: want (nil, error), got (%v, %v)", spec, err)
	}
	if !strings.Contains(err.Error(), "no players") {
		t.Fatalf("missing-players error does not say so: %q", err.Error())
	}

	// Edge 2: race not in data tables → ValidateRaces names the bad race.
	badRace := `victory = "score"
[[players]]
slot = 0
race = "gnoll"
controller = "user"
`
	s2, err := match.LoadMatchSpec([]byte(badRace))
	if err != nil {
		t.Fatalf("badRace structural load should pass (race existence is ValidateRaces): %v", err)
	}
	err = s2.ValidateRaces([]string{"vigil", "unbound"})
	t.Logf("EDGE unknown-race: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "gnoll") {
		t.Fatalf("ValidateRaces(unknown) want error naming 'gnoll', got %v", err)
	}

	// Edge 3: time_limit_ticks = 0 → rejected with reason.
	zeroLimit := `victory = "score"
time_limit_ticks = 0
[[players]]
slot = 0
race = "vigil"
controller = "user"
`
	spec, err = match.LoadMatchSpec([]byte(zeroLimit))
	t.Logf("EDGE zero-time-limit: spec=%v err=%v", spec, err)
	if err == nil || spec != nil || !strings.Contains(err.Error(), "time_limit_ticks must be > 0") {
		t.Fatalf("time_limit_ticks=0 want rejection, got (%v, %v)", spec, err)
	}

	// Edge 3b: negative time limit → rejected (not silently wrapped).
	negLimit := strings.Replace(zeroLimit, "time_limit_ticks = 0", "time_limit_ticks = -5", 1)
	spec, err = match.LoadMatchSpec([]byte(negLimit))
	t.Logf("EDGE negative-time-limit: spec=%v err=%v", spec, err)
	if err == nil || spec != nil {
		t.Fatalf("negative time_limit_ticks want rejection, got (%v, %v)", spec, err)
	}

	// Extra edges proving fail-closed breadth (slot range, dup slot, bad enum,
	// cpu-without-strategy, unknown key).
	cases := []struct {
		name, toml, want string
	}{
		{"out-of-range slot", `victory="score"
[[players]]
slot=99
race="vigil"
controller="user"`, "out of range"},
		{"duplicate slot", `victory="score"
[[players]]
slot=0
race="vigil"
controller="user"
[[players]]
slot=0
race="unbound"
controller="user"`, "duplicate slot"},
		{"bad victory", `victory="conquest"
[[players]]
slot=0
race="vigil"
controller="user"`, "unknown victory"},
		{"bad controller", `victory="score"
[[players]]
slot=0
race="vigil"
controller="alien"`, "unknown controller"},
		{"cpu without strategy", `victory="score"
[[players]]
slot=0
race="vigil"
controller="cpu"`, "no ai_strategy"},
		{"unknown key", `victory="score"
mystery=1
[[players]]
slot=0
race="vigil"
controller="user"`, "unknown spec keys"},
	}
	for _, c := range cases {
		spec, err := match.LoadMatchSpec([]byte(c.toml))
		t.Logf("EDGE %s: err=%v", c.name, err)
		if err == nil || spec != nil {
			t.Fatalf("%s: want rejection, got (%v, %v)", c.name, spec, err)
		}
		if !strings.Contains(err.Error(), c.want) {
			t.Fatalf("%s: error %q missing %q", c.name, err.Error(), c.want)
		}
	}
}
