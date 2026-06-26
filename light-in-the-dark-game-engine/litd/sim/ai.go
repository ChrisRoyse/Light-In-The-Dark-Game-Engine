package sim

// AI hook state (#257; ai-natives.md map-script side, R-EXEC-3). The
// map-script-facing AI natives — StartMeleeAI/StartCampaignAI, PauseCompAI,
// GetAIDifficulty, and the CommandAI integer-pair command stack — store their
// replay-safe inputs here. The AI *runtime* (the second sandboxed scheduler
// domain that consumes these) is M5.5; this is the deterministic sim state it
// reads, and the public-API surface that writes it.
//
// The AIController behavior is an api-layer Go interface, not stored here:
// behaviors are not deterministic state. Only difficulty (enum), the
// paused/attached flags, and the command inbox are sim state — they must be
// replay-identical (R-SIM-2), so they are hashed and saved.

// maxAIInbox caps a player's pending command stack. CommandAI past the cap is
// rejected (returns false) rather than growing unbounded (R-GC-1).
const maxAIInbox = 64

// aiCommand is one integer-pair command-stack entry (JASS CommandAI's
// command/data pair).
type aiCommand struct {
	Command int32
	Data    int32
}

// aiState is the per-player AI hook state.
type aiState struct {
	difficulty [MaxPlayers]uint8
	paused     [MaxPlayers]bool
	attached   [MaxPlayers]bool
	inbox      [MaxPlayers][]aiCommand // LIFO command stack per player
}

func validAIPlayer(p uint8) bool { return int(p) < MaxPlayers }

// AttachAI marks player p as computer-controlled at the given difficulty
// (StartMeleeAI / StartCampaignAI). No-op on a bad player.
func (w *World) AttachAI(p, difficulty uint8) {
	if !validAIPlayer(p) {
		return
	}
	w.ai.attached[p] = true
	w.ai.difficulty[p] = difficulty
}

// DetachAI clears a player's AI attachment and command stack.
func (w *World) DetachAI(p uint8) {
	if !validAIPlayer(p) {
		return
	}
	w.ai.attached[p] = false
	w.ai.paused[p] = false
	w.ai.inbox[p] = w.ai.inbox[p][:0]
}

// AIAttached reports whether player p is computer-controlled.
func (w *World) AIAttached(p uint8) bool { return validAIPlayer(p) && w.ai.attached[p] }

// AIDifficulty / SetAIDifficulty read and set a player's AI difficulty
// (GetAIDifficulty).
func (w *World) AIDifficulty(p uint8) uint8 {
	if !validAIPlayer(p) {
		return 0
	}
	return w.ai.difficulty[p]
}

func (w *World) SetAIDifficulty(p, difficulty uint8) {
	if validAIPlayer(p) {
		w.ai.difficulty[p] = difficulty
	}
}

// SetAIPaused / AIPaused suspend or resume a player's AI (PauseCompAI).
func (w *World) SetAIPaused(p uint8, paused bool) {
	if validAIPlayer(p) {
		w.ai.paused[p] = paused
	}
}

func (w *World) AIPaused(p uint8) bool { return validAIPlayer(p) && w.ai.paused[p] }

// PushAICommand enqueues a command/data pair onto player p's command stack
// (CommandAI). Returns false on a bad player or a full stack.
func (w *World) PushAICommand(p uint8, command, data int32) bool {
	if !validAIPlayer(p) || len(w.ai.inbox[p]) >= maxAIInbox {
		return false
	}
	w.ai.inbox[p] = append(w.ai.inbox[p], aiCommand{Command: command, Data: data})
	return true
}

// AICommandCount returns the number of pending commands (CommandsWaiting).
func (w *World) AICommandCount(p uint8) int {
	if !validAIPlayer(p) {
		return 0
	}
	return len(w.ai.inbox[p])
}

// LastAICommand returns the top of the command stack without removing it
// (GetLastCommand). ok is false when the stack is empty.
func (w *World) LastAICommand(p uint8) (command, data int32, ok bool) {
	if !validAIPlayer(p) || len(w.ai.inbox[p]) == 0 {
		return 0, 0, false
	}
	c := w.ai.inbox[p][len(w.ai.inbox[p])-1]
	return c.Command, c.Data, true
}

// PopAICommand removes the top of the command stack (PopLastCommand). Returns
// false when the stack was already empty.
func (w *World) PopAICommand(p uint8) bool {
	if !validAIPlayer(p) || len(w.ai.inbox[p]) == 0 {
		return false
	}
	w.ai.inbox[p] = w.ai.inbox[p][:len(w.ai.inbox[p])-1]
	return true
}
