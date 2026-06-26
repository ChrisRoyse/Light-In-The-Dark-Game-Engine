package ai

// Command-stack natives (#276; sim-delegation fix #379; jass-mapping/
// ai-natives.md command-stack family; execution-model.md §6). The WC3
// command-stack natives as typed verbs over the sim-owned command stack
// (inbox.go / litd/sim/ai.go). Each common.ai command-stack native maps to one
// verb here, all delegating to the single authoritative sim stack — no storage,
// no serialization at this layer.
//
// Native → verb mapping (integer-pair payload, with a LastPair compatibility
// accessor for ported scripts):
//
//	CommandsWaiting()   → (CommandStack).CommandsWaiting   // stack depth
//	GetLastCommand()    → (CommandStack).GetLastCommand    // top command int
//	GetLastData()       → (CommandStack).GetLastData       // top data int
//	PopLastCommand()    → (CommandStack).PopLastCommand    // remove top
//	CommandAI(p,c,d)    → CommandAI(src, p, c, d)          // map-side push
//
// Delivery-tick contract (decided, documented, golden-tested): a command sent
// by the map script via CommandAI on tick T is visible to the AI on the SAME
// tick T. Within tick phase 2 the map-script domain runs before the AI domain
// (tick-and-scheduler.md §3.4), so every push of tick T has already landed on
// the (sim-owned) stack before the AI sub-phase reads it that same tick. No T+1
// delay, no cross-tick buffering.
//
// Empty-stack semantics (WC3-faithful, panic-free): GetLastCommand / GetLastData
// on an empty stack return 0; PopLastCommand on an empty stack is a no-op (the
// sim's PopAICommand returns false and the counter never goes negative). WC3
// scripts always guard with CommandsWaiting() != 0 before reading.

// CommandsWaiting returns the number of commands on the stack (WC3
// CommandsWaiting).
func (c CommandStack) CommandsWaiting() int { return c.src.AICommandCount(c.player) }

// GetLastCommand returns the command value of the top (most-recent) command, or
// 0 if the stack is empty (WC3 GetLastCommand).
func (c CommandStack) GetLastCommand() int {
	cmd, _, ok := c.src.LastAICommand(c.player)
	if !ok {
		return 0
	}
	return int(cmd)
}

// GetLastData returns the data value of the top command, or 0 if empty (WC3
// GetLastData).
func (c CommandStack) GetLastData() int {
	_, data, ok := c.src.LastAICommand(c.player)
	if !ok {
		return 0
	}
	return int(data)
}

// PopLastCommand removes the top (most-recent) command, exposing the next-older
// one. A no-op on an empty stack (WC3 PopLastCommand, fail-safe).
func (c CommandStack) PopLastCommand() { c.src.PopAICommand(c.player) }

// LastPair is the integer-pair compatibility accessor for ported scripts: the
// top command and data together, ok=false on an empty stack.
func (c CommandStack) LastPair() (command, data int, ok bool) {
	cmd, d, ok := c.src.LastAICommand(c.player)
	return int(cmd), int(d), ok
}

// CommandAI pushes (command, data) onto player's command stack on src — the
// map-side delivery (WC3 CommandAI). Returns true if accepted, false on a bad
// player or a full stack (the sim enforces the bound). Visible to the AI on the
// same tick (see the delivery-tick contract above).
func CommandAI(src CommandStackSource, player, command, data int) bool {
	return src.PushAICommand(uint8(player), int32(command), int32(data))
}
