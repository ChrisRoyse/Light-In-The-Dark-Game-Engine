package ai

// Production/training natives (#277; jass-mapping/ai-natives.md build/train
// family; execution-model.md §6). The common.ai production family —
// SetProduce / TrainUnits / GetUnitCount(Done) class — expressed as AICommander
// training INTENTS plus AIView production-state queries.
//
// The model is WC3's AI production model, not the player's: the AI names a unit
// TYPE to train; the sim chooses the concrete producer building deterministically
// (litd/sim TrainForPlayer — least-loaded, ties to lowest entity id) and owns
// the queue. The AI never selects a building or mutates a queue (R-EXEC-3) — it
// issues a typed intent into the command stream and reads back counts. This
// keeps production authoritative, replayable sim state: a train intent the
// player cannot afford is recorded in the stream like any other and is a
// deterministic no-op at apply time (the sim changes nothing), observable by
// the AI only through the production counts.
//
// Three production states are distinguishable, matching GetUnitCount vs
// GetUnitCountDone:
//   - queued     — waiting behind a producer's head slot   (ProductionView.Queued)
//   - in-progress — the head slot, training right now        (ProductionView.InProgress)
//   - done       — a completed unit entity in the world      (View.OwnUnitCountDone, #274)

// Train refusal reasons — the AI-facing mirror of litd/sim's Train* reason
// codes (same values; the sim is the source of truth). A reason != TrainOK
// means the intent was a no-op: nothing in the sim changed.
const (
	TrainOK           = 0
	TrainNoProducer   = 1 // player has no producer that can train this type (or none with queue space)
	TrainUnknownType  = 2
	TrainNotTrainable = 3
	TrainQueueFull    = 4
	TrainTechLocked   = 5
	TrainNoFood       = 6
	TrainNoResources  = 7
)

// ProductionControl is the sim-authoritative production surface the AI domain
// drives. *sim.World is adapted to it at the integration boundary (the methods
// here take ints; the sim's take uint8/uint16). It does exactly two things:
// admit a training intent (the sim picks the producer) and report production
// counts — no building selection or queue mutation is exposed to the AI.
type ProductionControl interface {
	// TrainForPlayer admits one intent to train typeID for player; the sim
	// chooses the producer. Returns the chosen building id (-1 if none could be
	// chosen) and a Train* reason. A non-OK reason is a deterministic no-op.
	TrainForPlayer(player, typeID int) (building, reason int)
	// TrainInProgress counts the player's producers whose head slot is typeID.
	TrainInProgress(player, typeID int) int
	// TrainQueued counts the player's queued-behind-head slots of typeID.
	TrainQueued(player, typeID int) int
}

// TrainIntent builds the typed training command for typeID. The Commander
// stamps the issuing player authoritatively, so the controller leaves Player at
// zero. This is the single train-one-unit intent (WC3 TrainUnit analogue).
func TrainIntent(typeID int) AICommand {
	return AICommand{Kind: CmdTrain, A: int32(typeID)}
}

// TrainUnits issues count training intents for typeID through commander — the
// AICommander analogue of WC3 TrainUnits. Each intent is an independent command
// recorded in the stream and applied independently (the sim picks a producer
// and gates each one), so an unaffordable tail is a run of deterministic no-ops
// the AI observes via the production counts, not a partial batch failure.
func TrainUnits(commander AICommander, typeID, count int) {
	for i := 0; i < count; i++ {
		commander.Issue(TrainIntent(typeID))
	}
}

// ProduceTarget tops production up toward a target population of typeID — the
// WC3 SetProduce "maintain N" analogue. It counts what already exists or is
// coming (done completed units + pending = in-progress + queued) and issues
// exactly enough intents to reach target, never more. Idempotent at the target:
// calling it when done+pending >= target issues nothing. Returns the number of
// intents issued. `done` is the completed-entity count from the AIView entity
// query (View.OwnUnitCountDone), which production state alone cannot see.
func ProduceTarget(commander AICommander, pv ProductionView, typeID, done, target int) int {
	have := done + pv.Pending(typeID)
	n := target - have
	if n <= 0 {
		return 0
	}
	TrainUnits(commander, typeID, n)
	return n
}

// ApplyTrain executes one drained CmdTrain against the sim-authoritative
// production control: it routes the intent's type to TrainForPlayer, which
// selects the producer and admits (or refuses) it. A non-CmdTrain command is
// not this layer's concern and is reported as TrainNotTrainable with no effect.
// Returns the chosen building id (-1 if none) and the Train* reason.
func ApplyTrain(ctrl ProductionControl, c AICommand) (building, reason int) {
	if c.Kind != CmdTrain {
		return -1, TrainNotTrainable
	}
	return ctrl.TrainForPlayer(int(c.Player), int(c.A))
}

// ProductionView is one player's read of production state — the in-progress and
// queued counts of a unit type. Completed units are counted by the AIView entity
// query (View.OwnUnitCountDone); a controller combines the two for the full
// queued/in-progress/done picture.
type ProductionView struct {
	ctrl   ProductionControl
	player int
}

// NewProductionView binds a production read for player over the sim control.
func NewProductionView(ctrl ProductionControl, player int) ProductionView {
	return ProductionView{ctrl: ctrl, player: player}
}

// InProgress is the count of this player's producers actively training typeID
// (the head slot) right now.
func (p ProductionView) InProgress(typeID int) int { return p.ctrl.TrainInProgress(p.player, typeID) }

// Queued is the count of this player's queued-behind-head slots of typeID.
func (p ProductionView) Queued(typeID int) int { return p.ctrl.TrainQueued(p.player, typeID) }

// Pending is everything coming but not yet a unit: in-progress + queued.
func (p ProductionView) Pending(typeID int) int { return p.InProgress(typeID) + p.Queued(typeID) }
