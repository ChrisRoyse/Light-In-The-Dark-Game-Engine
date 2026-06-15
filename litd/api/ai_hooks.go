package litd

// AI hooks — the map-script-side AI natives (#257; ai-natives.md, R-EXEC-3,
// D-2026-06-11-6). These are the verbs a *map script* uses to stand up,
// pause, and message a computer player. The computer player's strategy runs
// in a second sandboxed scheduler domain at M5.5; this surface is the M5
// public boundary plus the replay-safe sim state it reads/writes:
//
//   - AttachAI / PauseAI / AIDifficulty / IsAIPlayer — lifecycle + difficulty,
//   - CommandAI — the integer-pair command stack (CommandAI / CommandsWaiting /
//     GetLastCommand / PopLastCommand), drained by the AI domain at M5.5.
//
// Unit-level guard-position behavior (RemoveGuardPosition) is core sim combat,
// not an AI-domain hook, and lives on the unit surface (units.md), not here.

// Difficulty is a computer player's skill level (JASS aidifficulty /
// GetAIDifficulty). The *BJ difficulty-preset wrappers collapse onto these.
type Difficulty uint8

const (
	DifficultyEasy   Difficulty = iota // AI_DIFFICULTY_NEWBIE
	DifficultyNormal                   // AI_DIFFICULTY_NORMAL
	DifficultyInsane                   // AI_DIFFICULTY_INSANE
)

// AIView is the read-only, AI-legal query surface handed to an AIController at
// M5.5. It is fog-honest by default (porting hazard 3): an AI sees only what
// its player can see, with an explicit cheating-difficulty escape hatch added
// consciously rather than inherited from JASS. The query set (own units,
// visible enemies, the GetUnitCount* family) is implemented with the AI domain
// in M5.5; the boundary type is fixed here.
type AIView interface {
	// Difficulty reports the controller's configured difficulty.
	Difficulty() Difficulty
}

// AICommander is the intent + inbox surface handed to an AIController at M5.5.
// It carries the build/train/harvest/attack-wave intents and the receive side
// of the command stack (CommandsWaiting / GetLastCommand / PopLastCommand).
type AICommander interface {
	// PendingCommands is CommandsWaiting — the number of queued commands.
	PendingCommands() int
	// LastCommand is GetLastCommand — the top of the command stack; ok is
	// false when the stack is empty.
	LastCommand() (command, data int, ok bool)
	// PopCommand is PopLastCommand — drop the top of the command stack.
	PopCommand() bool
}

// AIController is a computer player's strategy. Tick runs inside the second
// sandboxed scheduler domain (R-EXEC-3): no shared globals with the map
// script, communication only through the typed view/commander. Dispatch is
// wired at M5.5; AttachAI records the controller now.
type AIController interface {
	Tick(view AIView, cmd AICommander)
}

// AttachAI makes player p a computer player at difficulty d, running strategy
// ai (StartMeleeAI / StartCampaignAI). A nil controller still marks the player
// AI-controlled (difficulty/flags set) — useful before a controller is bound.
// No-op on an invalid player.
func (g *Game) AttachAI(p Player, ai AIController, d Difficulty) {
	if !p.Valid() {
		g.reportInvalid("Game.AttachAI")
		return
	}
	g.w.AttachAI(uint8(p.idx), uint8(d))
	if ai != nil {
		if g.aiControllers == nil {
			g.aiControllers = make(map[uint8]AIController)
		}
		g.aiControllers[uint8(p.idx)] = ai
	}
}

// PauseAI suspends or resumes player p's computer control (PauseCompAI).
// No-op on an invalid player.
func (g *Game) PauseAI(p Player, paused bool) {
	if !p.Valid() {
		g.reportInvalid("Game.PauseAI")
		return
	}
	g.w.SetAIPaused(uint8(p.idx), paused)
}

// IsAIPlayer reports whether player p is computer-controlled.
func (g *Game) IsAIPlayer(p Player) bool {
	return p.Valid() && g.w.AIAttached(uint8(p.idx))
}

// AIDifficulty returns player p's AI difficulty (GetAIDifficulty).
func (g *Game) AIDifficulty(p Player) Difficulty {
	if !p.Valid() {
		return DifficultyEasy
	}
	return Difficulty(g.w.AIDifficulty(uint8(p.idx)))
}

// CommandAI pushes a command/data pair onto player p's AI command stack
// (CommandAI). The AI domain drains it at M5.5. No-op on an invalid player or
// a full stack. JASS returns nothing; this mirrors that.
func (g *Game) CommandAI(p Player, command, data int) {
	if !p.Valid() {
		g.reportInvalid("Game.CommandAI")
		return
	}
	g.w.PushAICommand(uint8(p.idx), int32(command), int32(data))
}
