package litd

// Live AI-domain wiring (#281; ai-natives.md public surface; R-EXEC-3). This
// connects the frozen M5 map-script hooks (AttachAI/PauseAI/AIDifficulty) to
// the real second scheduler domain in litd/ai:
//
//   - ensureAIDomain lazily stands up the domain and installs the per-tick AI
//     sub-phase via the sim's OnAIPhase hook (fired at the tail of phase 2,
//     after the map-script scheduler — so AI decisions are deterministic sim
//     input ordered after map scripts).
//   - installAIContext builds one isolated context per player, driven by a
//     FuncController that calls the public AIController.Tick every AI tick.
//   - aiBridge is the single object handed across the boundary: it satisfies
//     BOTH the public AIView/AICommander (what the controller sees) and the
//     domain's ai.AIView/ai.AICommander (what the context requires), all backed
//     by the deterministic sim — no behavior state, only sim reads and the
//     sim-authoritative train intent.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// ensureAIDomain creates the AI domain on first use and wires the AI sub-phase
// into the sim tick. Idempotent.
func (g *Game) ensureAIDomain() {
	if g.aiDomain != nil {
		return
	}
	g.aiDomain = ai.NewDomain()
	g.aiDomain.SetDiagnostics(nil) // watchdog state still set; no stderr spam by default
	prev := g.w.OnAIPhase
	g.w.OnAIPhase = func(tick uint32) {
		if prev != nil {
			prev(tick)
		}
		g.tickAIDomain()
	}
}

// installAIContext binds an isolated context for player idx, driven by ctrl.
func (g *Game) installAIContext(idx uint8, ctrl AIController) {
	br := &aiBridge{g: g, player: idx}
	fc := ai.NewFuncController(func() { ctrl.Tick(br, br) })
	g.aiDomain.AddPlayer(int(idx), br, br, fc)
}

// tickAIDomain runs the AI sub-phase: mirror each attached player's pause flag
// into the domain (a paused context is not stepped — its wake ticks shift, they
// are not dropped), then tick every enabled context once. Deterministic: the
// domain ticks contexts in fixed insertion order.
func (g *Game) tickAIDomain() {
	if g.aiDomain == nil {
		return
	}
	for i := 0; i < sim.MaxPlayers; i++ {
		if g.aiDomain.Context(i) == nil {
			continue
		}
		if g.w.AIPaused(uint8(i)) {
			g.aiDomain.Disable(i)
		} else {
			g.aiDomain.Enable(i)
		}
	}
	g.aiDomain.Tick(g.aiBudget)
}

// aiBridge is the sim-backed capability object for one AI player. It is the
// ONLY thing an AIController can reach across the domain boundary, and it holds
// no mutable state of its own — every method is a sim read or the validated
// train intent (R-EXEC-3). One struct implements the public AIView/AICommander
// AND the domain ai.AIView/ai.AICommander.
type aiBridge struct {
	g      *Game
	player uint8
}

// --- public AIView ---------------------------------------------------------

func (b *aiBridge) Difficulty() Difficulty { return Difficulty(b.g.w.AIDifficulty(b.player)) }

// UnitCount is fog-honest by default and full-vision at the cheating tier. Own
// units always count; enemy units count only when visible to this player,
// unless difficulty is DifficultyInsane (the data-driven full-vision knob).
func (b *aiBridge) UnitCount(player, unitTypeID int) int {
	w := b.g.w
	self := int(b.player)
	full := w.AIDifficulty(b.player) >= uint8(DifficultyInsane)
	b.g.queryScratch = w.AppendAllUnits(b.g.queryScratch[:0])
	n := 0
	for _, id := range b.g.queryScratch {
		or := w.Owners.Row(id)
		ur := w.UnitTypes.Row(id)
		if or == -1 || ur == -1 {
			continue
		}
		if int(w.Owners.Player[or]) != player || int(w.UnitTypes.TypeID[ur]) != unitTypeID {
			continue
		}
		if player == self || full {
			n++
			continue
		}
		tr := w.Transforms.Row(id)
		if tr == -1 {
			continue
		}
		if w.IsVisibleToPlayer(b.player, w.Transforms.Pos[tr]) {
			n++
		}
	}
	return n
}

func (b *aiBridge) OwnUnitCount(unitTypeID int) int { return b.UnitCount(int(b.player), unitTypeID) }

// --- public AICommander ----------------------------------------------------

func (b *aiBridge) PendingCommands() int { return b.g.w.AICommandCount(b.player) }

func (b *aiBridge) LastCommand() (command, data int, ok bool) {
	c, d, ok := b.g.w.LastAICommand(b.player)
	return int(c), int(d), ok
}

func (b *aiBridge) PopCommand() bool { return b.g.w.PopAICommand(b.player) }

func (b *aiBridge) Train(unitTypeID int) bool {
	_, reason := b.g.w.TrainForPlayer(b.player, uint16(unitTypeID))
	return reason == sim.TrainOK
}

// --- domain ai.AIView ------------------------------------------------------

func (b *aiBridge) Now() uint32 { return b.g.w.Tick() }
func (b *aiBridge) Self() int   { return int(b.player) }

// --- domain ai.AICommander -------------------------------------------------

// Issue routes a typed domain command onto the sim's integer-pair AI command
// stack (the same stack CommandAI/CommandsWaiting/PopLastCommand use). The
// minimal public controller drives the sim through Train rather than Issue;
// Issue exists to satisfy the domain boundary and carries Kind→command, A→data
// (B reserved for the richer command-stream surface).
func (b *aiBridge) Issue(cmd ai.AICommand) {
	b.g.w.PushAICommand(b.player, int32(cmd.Kind), cmd.A)
}
