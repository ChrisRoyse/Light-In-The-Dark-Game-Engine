package litd

// Live melee-AI wiring (M5.5; resolves the production-wiring gap #404 names).
//
// #281 wired the GENERIC AI surface (AttachAI over AIView/AICommander), but the
// real RTS brain — litd/ai/melee.Controller, which runs economy, build order,
// production, and attack waves — needs the full melee.Bridge
// (ai.AIView + ai.EconomyControl + ai.ProductionControl + ai.WaveSource). Until
// now aiBridge implemented only AIView/AICommander, so the melee controller
// could run *only in tests* (its own simBridge), never against a production
// litd/api.Game. This file closes that: aiBridge now satisfies melee.Bridge by
// delegating to the deterministic sim (mirroring the test simBridge exactly, so
// behavior is identical), and AttachMeleeAI installs the controller on the live
// AI domain. This is the prerequisite for #404 part 3 (auto-record an AI match
// to .litdreplay), #210 (real full-match replay), and #212 (acceptance vs AI).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai/melee"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// aiWU builds a sim world-unit position from integer coordinates (the units the
// melee controller speaks; the sim is fixed-point internally).
func aiWU(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }

// --- ai.EconomyControl -----------------------------------------------------

// AssignHarvest assigns up to count idle workers of player to resource,
// incrementally, returning how many were newly assigned (sim-authoritative).
func (b *aiBridge) AssignHarvest(player, resource, count int) int {
	return b.g.w.HarvestAssign(uint8(player), resource, count)
}

// HarvestersOn reports how many of player's workers currently gather resource.
func (b *aiBridge) HarvestersOn(player, resource int) int {
	return b.g.w.HarvestersOn(uint8(player), resource)
}

// PlaceBuilding runs the sim's deterministic placement search around (cx,cy) for
// typeID and issues the build; false on no site / no idle builder (a recorded
// no-op, never a panic).
func (b *aiBridge) PlaceBuilding(player, typeID int, cx, cy int32) bool {
	_, _, ok := b.g.w.PlaceBuildingNear(uint8(player), uint16(typeID), aiWU(cx, cy))
	return ok
}

// --- ai.ProductionControl --------------------------------------------------

// TrainForPlayer admits one intent to train typeID for player; the sim chooses
// the producer. Returns the chosen building index and a Train* reason; a non-OK
// reason yields (-1, reason) and is a deterministic no-op.
func (b *aiBridge) TrainForPlayer(player, typeID int) (int, int) {
	bid, reason := b.g.w.TrainForPlayer(uint8(player), uint16(typeID))
	if reason != sim.TrainOK {
		return -1, int(reason)
	}
	return int(bid.Index()), int(reason)
}

// TrainInProgress counts player's producers whose head slot is typeID.
func (b *aiBridge) TrainInProgress(player, typeID int) int {
	return b.g.w.PlayerTrainInProgress(uint8(player), uint16(typeID))
}

// TrainQueued counts player's queued-behind-head slots of typeID.
func (b *aiBridge) TrainQueued(player, typeID int) int {
	return b.g.w.PlayerTrainQueued(uint8(player), uint16(typeID))
}

// --- ai.WaveSource ---------------------------------------------------------

// EligibleUnits appends player's living units of typeID to dst in ascending
// entity-id order (AppendAllUnits is id-ordered) and returns the grown slice.
func (b *aiBridge) EligibleUnits(player, typeID int, dst []int32) []int32 {
	w := b.g.w
	b.waveScratch = w.AppendAllUnits(b.waveScratch[:0])
	for _, id := range b.waveScratch {
		or := w.Owners.Row(id)
		ur := w.UnitTypes.Row(id)
		if or != -1 && ur != -1 && int(w.Owners.Player[or]) == player && int(w.UnitTypes.TypeID[ur]) == typeID {
			dst = append(dst, int32(uint32(id)))
		}
	}
	return dst
}

// UnitPos returns a unit's integer world position and whether it is alive.
func (b *aiBridge) UnitPos(id int32) (int32, int32, bool) {
	w := b.g.w
	e := sim.EntityID(uint32(id))
	if !w.Ents.Alive(e) {
		return 0, 0, false
	}
	tr := w.Transforms.Row(e)
	if tr == -1 {
		return 0, 0, false
	}
	p := w.Transforms.Pos[tr]
	return int32(p.X.Floor()), int32(p.Y.Floor()), true
}

// OrderMoveTo issues a move order toward (x,y) — the wave gather step.
func (b *aiBridge) OrderMoveTo(id, x, y int32) {
	b.g.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: aiWU(x, y)}, false)
}

// OrderAttackTo issues the wave launch step. Realized as move-to-target so the
// idle stance acquires on arrival — the same faithful realization the test
// simBridge and the wave adapter use (pursuit is #150/#380).
func (b *aiBridge) OrderAttackTo(id, x, y int32) {
	b.g.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: aiWU(x, y)}, false)
}

// Compile-time proof the production bridge now satisfies the melee controller's
// full requirement — the gap #404 named ("aiBridge does not implement
// melee.Bridge") is closed.
var _ melee.Bridge = (*aiBridge)(nil)

// AttachMeleeAI installs the litd/ai/melee RTS controller as a live computer
// player for p, driven by the deterministic second AI domain (R-EXEC-3) exactly
// like AttachAI — but with the full economy/production/waves brain instead of a
// generic AIController. cfg.Self and cfg.Difficulty are overridden from p and d
// so they cannot disagree with the attachment. A nil strat detaches (mirrors
// AttachAI(nil)). Attaching to a defeated player is a no-op. Re-attaching
// replaces any prior context wholesale. The controller's running plan state
// (build order, wave roster) round-trips via Controller.Save/Load for mid-match
// save/load (D-9); the strategy table + bridge are reconstructed by the caller
// before load, the same re-install contract the AI domain already uses.
func (g *Game) AttachMeleeAI(p Player, strat *melee.Strategy, cfg melee.Config, d Difficulty) {
	if !p.Valid() {
		g.reportInvalid("Game.AttachMeleeAI")
		return
	}
	idx := uint8(p.idx)
	if r := g.w.PlayerResult(idx); r == sim.ResultLost || r == sim.ResultLeft {
		return // defeated player: no-op (parity with AttachAI)
	}
	g.w.AttachAI(idx, uint8(d)) // replay-safe sim flags (difficulty, AI-owned)
	if g.replayDrive {
		// Replay-apply mode: the recorded command stream stands in for the live
		// controller. Keep the sim AI flags above (they are hashed — parity with
		// the recorded run), but attach NO controller; SetReplayDrive's OnAIPhase
		// hook applies the stream instead.
		return
	}
	g.ensureAIDomain()
	g.aiDomain.RemovePlayer(int(idx)) // replace any prior context wholesale
	if strat == nil {
		delete(g.meleeControllers, idx)
		return
	}
	cfg.Self = int(idx)
	cfg.Difficulty = int(d) // Difficulty Easy/Normal/Insane == melee Diff consts 0/1/2
	br := &aiBridge{g: g, player: idx}
	// When recording (RecordReplay), the controller drives a tap that records its
	// decisions into the production replay format; the AI domain still reads and
	// issues through the raw bridge (which carries the AICommander Issue surface
	// the recorder's embedded interface does not), and the recorder delegates
	// every call unchanged, so the sim is unperturbed (#404).
	var ctrlBridge melee.Bridge = br
	if g.replayRecording {
		ctrlBridge = melee.NewRecordingBridge(br, &g.replayLog)
	}
	c := melee.NewController(strat, cfg, ctrlBridge)
	if g.meleeControllers == nil {
		g.meleeControllers = make(map[uint8]*melee.Controller)
	}
	g.meleeControllers[idx] = c
	g.aiDomain.AddPlayer(int(idx), br, br, ai.NewFuncController(c.Step))
}

// RecordReplay turns on production replay recording (#404): every melee AI
// attached after this call taps its bridge, recording its economy/production/
// wave decisions into the replay format. ReplayCommands returns the accumulated
// stream; BuildReplay wraps it into a verifiable *sim.Replay. Recording is
// non-intrusive — the tap delegates unchanged, so the sim hashes identically.
func (g *Game) RecordReplay() {
	g.replayRecording = true
	g.replayLog = g.replayLog[:0]
}

// ReplayCommands returns the recorded production command stream (nil if
// RecordReplay was never called). The slice is owned by the Game — copy before
// retaining across further recording.
func (g *Game) ReplayCommands() []sim.ReplayCommand { return g.replayLog }

// BuildReplay assembles the recorded stream into a *sim.Replay ready to Encode
// to a .litdreplay, stamped with the tick count run so far. Seed/MapHash/
// Fingerprint are left 0 for the caller to fill from the match setup (the same
// posture the rest of the format uses until the map format lands) — the
// recording replays on an identically-constructed world, not by reseeding from
// the header.
func (g *Game) BuildReplay() *sim.Replay {
	return &sim.Replay{
		Version:  sim.ReplayFormatVersion,
		Interval: sim.DefaultCheckpointInterval,
		Ticks:    g.w.Tick(),
		Commands: append([]sim.ReplayCommand(nil), g.replayLog...),
	}
}

// SetReplayDrive puts the Game into replay-apply mode — the mirror of
// RecordReplay. After this call, every AttachMeleeAI keeps the sim AI flags (so
// the replay world hashes identically to the recorded one) but registers NO live
// controller; instead the recorded command stream cmds is applied each AI
// sub-phase at its recorded tick, via the same OnAIPhase hook the live domain
// uses. This is the commands-only determinism model (#404): a real AI match
// reproduces bit-identically with the controllers detached (#649, over the
// shipped world). cmds must be tick-ascending (the recorder's natural order).
// Call BEFORE the world's setup script runs so AttachMeleeAI observes the mode.
func (g *Game) SetReplayDrive(cmds []sim.ReplayCommand) {
	g.replayDrive = true
	g.replayApply = cmds
	g.replayApplyNext = 0
	resolve := melee.EntityResolver(g.w)
	prev := g.w.OnAIPhase
	g.w.OnAIPhase = func(tick uint32) {
		if prev != nil {
			prev(tick)
		}
		tk := g.w.Tick()
		for g.replayApplyNext < len(g.replayApply) && g.replayApply[g.replayApplyNext].Tick == tk {
			g.replayApply[g.replayApplyNext].Apply(g.w, resolve)
			g.replayApplyNext++
		}
	}
}

// ReplayApplied reports how many recorded commands the replay drive has applied
// so far — len(stream) at a faithful end-of-run means the tick addressing lined
// up (no dropped/extra commands).
func (g *Game) ReplayApplied() int { return g.replayApplyNext }
