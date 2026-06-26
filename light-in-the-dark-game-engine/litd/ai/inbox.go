package ai

// Typed command-stack messaging — the inbound half (#273; sim-delegation fix
// #379). The AI player's command stack is authoritative, deterministic **sim
// state** (litd/sim/ai.go #257): a per-player LIFO stack that is hashed and
// saved for replay (R-SIM-2/6). The AI domain does NOT own a second copy — it
// accesses its mailbox through CommandStackSource, a small interface the sim
// satisfies. This keeps exactly one source of truth, so an AI's pending
// commands survive a save and replay bit-identically.
//
// Semantics are WC3-faithful (verified against jassdoc; Hive "Intermediate AI
// concepts"; common.ai:135-138; PRD execution-model.md §6 "command stack") and
// enforced at the sim SoT: a LIFO stack — CommandAI pushes onto the top,
// GetLastCommand reads the top (most recent), PopLastCommand removes it exposing
// older commands. A ported WC3 script pushing (A,B,C) then reading
// GetLastCommand sees C.

// CommandStackSource is the sim-owned per-player AI command stack. *sim.World
// satisfies it via PushAICommand / AICommandCount / LastAICommand / PopAICommand
// (litd/sim/ai.go). The interface is the AI domain's only handle on its mailbox;
// the storage, bounds (the sim's cap), state-hash, and save/restore all live in
// the sim, so there is no second representation to keep in sync.
type CommandStackSource interface {
	// PushAICommand pushes (command, data) onto player's stack; false on a bad
	// player or full stack (WC3 CommandAI / the map-side delivery).
	PushAICommand(player uint8, command, data int32) bool
	// AICommandCount returns player's stack depth (WC3 CommandsWaiting).
	AICommandCount(player uint8) int
	// LastAICommand returns the top (command, data) without removing it; ok is
	// false when empty (WC3 GetLastCommand / GetLastData).
	LastAICommand(player uint8) (command, data int32, ok bool)
	// PopAICommand removes the top; false when already empty (WC3 PopLastCommand).
	PopAICommand(player uint8) bool
}

// InboxMsg is one integer-pair command — the typed value WC3's CommandAI passed
// (command + data). A convenience for callers that prefer a struct over the
// loose pair; never a handle, copies by value.
type InboxMsg struct {
	Command int32
	Data    int32
}

// CommandStack is one AI player's typed view of its sim-owned command stack. It
// is a tiny value (source + player id) holding no storage of its own; construct
// one per player at AI setup. The WC3-native verbs are in inbox_natives.go.
type CommandStack struct {
	src    CommandStackSource
	player uint8
}

// NewCommandStack binds a typed view to player's stack on src (the sim).
func NewCommandStack(src CommandStackSource, player int) CommandStack {
	return CommandStack{src: src, player: uint8(player)}
}

// Player returns the player id this view addresses.
func (c CommandStack) Player() int { return int(c.player) }
