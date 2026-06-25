// Package campaignrun is the runtime campaign orchestrator (#534): it plays a
// campaign's missions in order, threading one persistent D-15 store across them so
// a hero earned in one mission carries into the next. It is the layer that turns
// the campaign carry-over machinery (litd/campaign + luabind.RunCampaignHook) and
// the mission world archives into an actually-played-through campaign, instead of
// the test-level hand-wiring that drove the hook before.
//
// The loop per mission: load the world, inject the running campaign store (so the
// mission sees the carry the previous mission's hook committed), step the sim until
// the local player reaches a terminal result (the world calls Game_Victory/Defeat),
// run the campaign on-complete/on-fail hook against that store (committing the carry
// for the next mission), then snapshot the store forward. worldhost.Load builds a
// fresh store per world, so the store is threaded by serialize-out / load-in — the
// same Storage Save/Load the mid-campaign save path uses.
package campaignrun

import (
	"bytes"
	"fmt"
	"io/fs"
	"os"
	"path"
	"strings"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

// Options configures a campaign run.
type Options struct {
	Seed       int64 // sim seed for each mission world (deterministic)
	Budget     int64 // Lua instruction budget per world (0 -> a sane default)
	MaxTicks   int   // safety cap on ticks per mission (0 -> DefaultMaxTicks)
	PlayerSlot int   // the local player whose result ends a mission

	// EngineVersion, when non-empty, is checked against each archived mission's
	// engine-range (a shipped campaign refuses to run on an incompatible engine).
	// Empty skips only the semver gate — the archive's content hash is always
	// fully verified — which is the convenient dev-directory posture.
	EngineVersion string
}

// DefaultMaxTicks bounds a single mission so a world that never resolves cannot
// hang the run. 50 ms/tick * 12000 = 10 minutes of mission time.
const DefaultMaxTicks = 12000

// DefaultBudget is the per-world Lua instruction budget when Options.Budget == 0.
const DefaultBudget = 50_000_000

// MissionOutcome is the recorded result of one played mission.
type MissionOutcome struct {
	MissionID string
	Result    api.MatchResult
	Won       bool
	Ticks     int // ticks stepped before the result latched (or the cap)
}

// Result is the outcome of a whole campaign run.
type Result struct {
	Missions  []MissionOutcome
	Completed bool   // every mission in order reached a Won result
	StoreBlob []byte // the final campaign store (serialized) — the carried state
}

// Run plays def's missions in declared order (each mission's `requires` must name
// only earlier-completed missions), threading one store across them. archivesRoot
// is the directory whose subdirectories are the mission worlds named by each
// mission's Archive; hooks is the filesystem holding the campaign hook script
// (main.lua defining the on-complete/on-fail functions). It stops at the first
// mission the player does not win (Completed=false) and returns what was played.
func Run(def campaign.Definition, archivesRoot string, hooks fs.FS, opts Options) (Result, error) {
	if archivesRoot == "" {
		return Result{}, fmt.Errorf("campaignrun: empty archives root")
	}
	if hooks == nil {
		return Result{}, fmt.Errorf("campaignrun: nil hooks filesystem")
	}
	if opts.Budget == 0 {
		opts.Budget = DefaultBudget
	}
	if opts.MaxTicks == 0 {
		opts.MaxTicks = DefaultMaxTicks
	}

	var res Result
	var carry []byte // serialized campaign store threaded between missions
	completed := map[string]bool{}

	for _, m := range def.Missions {
		// Guard the requires graph: declared order must be a valid play order.
		for _, req := range m.Requires {
			if !completed[req] {
				return res, fmt.Errorf("campaignrun: mission %q requires %q, not completed before it", m.ID, req)
			}
		}

		host, err := loadMission(path.Join(archivesRoot, m.Archive), opts.EngineVersion, opts.Seed, opts.Budget)
		if err != nil {
			return res, fmt.Errorf("campaignrun: load mission %q: %w", m.ID, err)
		}

		// Inject the running campaign store so the mission sees prior carry BEFORE
		// its first tick.
		if carry != nil {
			if err := host.Game.Storage().Load(bytes.NewReader(carry)); err != nil {
				host.Close()
				return res, fmt.Errorf("campaignrun: inject carry into %q: %w", m.ID, err)
			}
		}

		// Step until the local player resolves, or the safety cap.
		ticks, result := stepUntilTerminal(host.Game, opts.PlayerSlot, opts.MaxTicks)
		won := result == api.ResultWon
		res.Missions = append(res.Missions, MissionOutcome{
			MissionID: m.ID, Result: result, Won: won, Ticks: ticks,
		})

		outcome := campaign.OutcomeFail
		if won {
			outcome = campaign.OutcomeComplete
		}
		// Run the campaign hook against the mission's store; on success it commits
		// the carry (and mission flags) for the next mission.
		if _, err := luabind.RunCampaignHook(host.Game.Storage(), def, hooks, m.ID, m.ID, outcome,
			luabind.CampaignHookOptions{}); err != nil {
			host.Close()
			return res, fmt.Errorf("campaignrun: hook for %q (%s): %w", m.ID, outcome, err)
		}

		// Snapshot the store forward.
		var buf bytes.Buffer
		if err := host.Game.Storage().Save(&buf); err != nil {
			host.Close()
			return res, fmt.Errorf("campaignrun: snapshot store after %q: %w", m.ID, err)
		}
		carry = buf.Bytes()
		host.Close()

		if !won {
			// Stop the run at the first loss; the campaign is not completed.
			res.StoreBlob = carry
			return res, nil
		}
		completed[m.ID] = true
	}

	res.Completed = true
	res.StoreBlob = carry
	return res, nil
}

// loadMission loads one mission world, transparently handling both shipped
// archives and dev directories so a campaign authored as loose directories and the
// same campaign packed to `.litdworld` archives play identically (#312, D-14). The
// candidate is the manifest's archive path joined to the campaign root; it resolves
// in this order: an explicit `.litdworld` file (or a regular file at the exact
// name) loads through the verified archive path; a `<candidate>.litdworld` sibling
// loads the packed form when the manifest names the bare mission; otherwise a
// directory loads through the dev path. A name that resolves to nothing fails loud.
func loadMission(candidate, engineVersion string, seed, budget int64) (*worldhost.Host, error) {
	if strings.HasSuffix(candidate, ".litdworld") {
		return worldhost.LoadArchive(candidate, engineVersion, seed, budget)
	}
	if fi, err := os.Stat(candidate); err == nil {
		if fi.IsDir() {
			return worldhost.Load(candidate, seed, budget)
		}
		// A regular file at the exact archive name is a packed world.
		return worldhost.LoadArchive(candidate, engineVersion, seed, budget)
	}
	// The bare name did not resolve — prefer a packed sibling before giving up, so
	// a manifest can name "kindle" and ship "kindle.litdworld".
	if _, err := os.Stat(candidate + ".litdworld"); err == nil {
		return worldhost.LoadArchive(candidate+".litdworld", engineVersion, seed, budget)
	}
	return nil, fmt.Errorf("no mission world at %q (tried it as a directory, as an archive, and as %q)", candidate, candidate+".litdworld")
}

// stepUntilTerminal advances g one tick at a time until the player in slot resolves
// to a non-playing result or the tick cap is hit, returning the ticks stepped and
// the final result.
func stepUntilTerminal(g *api.Game, slot, maxTicks int) (int, api.MatchResult) {
	for i := 1; i <= maxTicks; i++ {
		g.Advance(1)
		if r := g.Player(slot).Result(); r != api.ResultPlaying {
			return i, r
		}
	}
	return maxTicks, g.Player(slot).Result()
}
