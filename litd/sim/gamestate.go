package sim

const (
	// ResultPlaying is the zero state: no terminal outcome latched.
	ResultPlaying uint8 = iota
	ResultWon
	ResultLost
	ResultLeft
)

const (
	// EvVictory fires when a player first transitions to ResultWon.
	// Arg carries the player slot index. Event IDs 9/10 are already
	// occupied by resource events, so victory/defeat use the next free
	// built-in IDs after missile presentation events.
	EvVictory uint16 = 24
	// EvDefeat fires when a player first transitions to ResultLost or
	// ResultLeft. Arg carries the player slot index; read Result(p) to
	// distinguish lost vs left.
	EvDefeat uint16 = 25
)

// PlayerResult returns the immutable terminal result for player, or
// ResultPlaying when the player is still active or out of range.
func (w *World) PlayerResult(player uint8) uint8 {
	if player >= MaxPlayers {
		return ResultPlaying
	}
	return w.results[player]
}

// SetVictory stages a player victory request for phase-6 resolution.
// It returns false for invalid players, already-terminal players, or
// when another terminal request for the player is already staged.
func (w *World) SetVictory(player uint8) bool { return w.stageResult(player, ResultWon) }

// SetDefeat stages a player defeat request for phase-6 resolution.
func (w *World) SetDefeat(player uint8) bool { return w.stageResult(player, ResultLost) }

// SetLeft stages a player-left request for phase-6 resolution.
func (w *World) SetLeft(player uint8) bool { return w.stageResult(player, ResultLeft) }

func (w *World) stageResult(player, result uint8) bool {
	if player >= MaxPlayers || result <= ResultPlaying || result > ResultLeft {
		return false
	}
	if w.results[player] != ResultPlaying || w.resultPending[player] != ResultPlaying {
		return false
	}
	if result == ResultWon && w.hasWinner() {
		return false
	}
	w.resultPending[player] = result
	return true
}

func (w *World) hasWinner() bool {
	for player := 0; player < MaxPlayers; player++ {
		if w.results[player] == ResultWon || w.resultPending[player] == ResultWon {
			return true
		}
	}
	return false
}

func (w *World) resolveMatchResults() {
	// One-pass tie rule: resultPending holds only the first terminal
	// request staged for a player during this tick; later same-tick
	// requests for that player are rejected by stageResult. Phase 6
	// resolves all pending players in ascending player index before the
	// event ring is flushed, so event order is deterministic.
	for player := uint8(0); player < MaxPlayers; player++ {
		result := w.resultPending[player]
		if result == ResultPlaying {
			continue
		}
		w.resultPending[player] = ResultPlaying
		if w.results[player] != ResultPlaying {
			continue
		}
		w.results[player] = result
		switch result {
		case ResultWon:
			w.Emit(Event{Kind: EvVictory, Arg: int64(player)})
		case ResultLost, ResultLeft:
			w.Emit(Event{Kind: EvDefeat, Arg: int64(player)})
		}
	}
}
