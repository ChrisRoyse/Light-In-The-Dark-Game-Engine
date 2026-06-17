package litd

// Victory/defeat lifecycle surface (jass-mapping/game-state-and-melee.md).
// The API layer owns presentation-only details such as defeat messages;
// the deterministic source of truth is the sim World's player result
// store, resolved in phase 6 and exposed through Player.Result.

import (
	"log"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// MatchResult is the public terminal result of a player slot.
type MatchResult uint8

const (
	// ResultPlaying means no terminal outcome has latched for the
	// player.
	ResultPlaying MatchResult = MatchResult(sim.ResultPlaying)
	// ResultWon means the player has won the match.
	ResultWon MatchResult = MatchResult(sim.ResultWon)
	// ResultLost means the player has lost the match.
	ResultLost MatchResult = MatchResult(sim.ResultLost)
	// ResultLeft means the player left the match.
	ResultLeft MatchResult = MatchResult(sim.ResultLeft)
)

// Result returns the player's current terminal result. Invalid players
// return ResultPlaying and report in debug mode. JASS: player game
// result state.
func (p Player) Result() MatchResult {
	if !p.Valid() {
		p.g.reportInvalid("Player.Result")
		return ResultPlaying
	}
	return MatchResult(p.g.w.PlayerResult(uint8(p.idx)))
}

// Victory stages p as the match winner. The sim resolves the request in
// its deterministic phase-6 result pass; duplicate, terminal, or
// second-winner requests are ignored by that store. No-op for an
// invalid player. JASS: CustomVictoryBJ/CachePlayerHeroData +
// RemovePlayer(PLAYER_GAME_RESULT_VICTORY).
// JASS: CustomVictoryBJ, CustomVictoryOkBJ, CustomVictoryQuitBJ, CustomVictorySkipBJ
func (g *Game) Victory(p Player) {
	if !g.ownsPlayer(p) {
		g.reportInvalid("Game.Victory")
		return
	}
	g.w.SetVictory(uint8(p.idx))
}

// Defeat stages p as defeated. msg is presentation-only and deliberately
// does not enter sim state, hashes, or saves; until UI defeat dialogs
// exist, accepted non-empty messages are routed to the process log.
// No-op for an invalid player. JASS: CustomDefeatBJ.
// JASS: CustomDefeatBJ, CustomDefeatLoadBJ, CustomDefeatQuitBJ, CustomDefeatReduceDifficultyBJ, CustomDefeatRestartBJ
func (g *Game) Defeat(p Player, msg string) {
	if !g.ownsPlayer(p) {
		g.reportInvalid("Game.Defeat")
		return
	}
	if !g.w.SetDefeat(uint8(p.idx)) {
		return
	}
	if msg != "" {
		log.Printf("litd: defeat player=%d reason=%q", p.idx, msg)
	}
}

// EndMatch stages defeat for every player still playing. Existing or
// pending terminal results are left intact by the sim's latch rules.
// The score-screen/dialog presentation of WC3 EndGame is handled by UI
// code later; this method only writes deterministic match state.
// JASS: EndGame
func (g *Game) EndMatch() {
	if g == nil || g.w == nil {
		return
	}
	for p := uint8(0); p < sim.MaxPlayers; p++ {
		g.w.SetDefeat(p)
	}
}

// OnVictory registers a victory-event handler. It is a typed wrapper
// around OnEvent(EventVictory, ...).
func (g *Game) OnVictory(handler func(Event), opts ...EventOption) Subscription {
	return g.OnEvent(EventVictory, handler, opts...)
}

// OnDefeat registers a defeat-event handler. It is a typed wrapper
// around OnEvent(EventDefeat, ...).
func (g *Game) OnDefeat(handler func(Event), opts ...EventOption) Subscription {
	return g.OnEvent(EventDefeat, handler, opts...)
}

func (g *Game) ownsPlayer(p Player) bool {
	return g != nil && g.w != nil && p.g == g && g.playerValid(p.idx)
}
