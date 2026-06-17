package melee

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"

// Controller is the one data-driven melee AI. Every faction runs THIS code over
// a different Strategy table. It drives the native families each tick:
//
//	economy : top up harvesters toward the (difficulty-scaled) targets.
//	build   : advance the build order one placement per tick (BuildOrder).
//	army    : maintain the standing army toward the target (ProduceTarget logic
//	          against ProductionControl).
//	waves   : once enough soldiers are ready and no wave is in flight, stage one
//	          rolling attack wave at the enemy base; the WaveManager gathers,
//	          launches, and prunes it. A wiped/finished wave frees the trigger so
//	          the rebuilt army launches the next — wave after wave until a side
//	          can no longer field one.
//
// It speaks only litd/ai surfaces (AIView + the native-family controls); it has
// no reference to the sim, to the map-script domain, or to the other player —
// isolation is a property of its dependencies (R-EXEC-3).

// Bridge is the full capability set a melee controller needs from the
// integration layer: the read view plus the three native-family control
// surfaces. One sim adapter satisfies it per player at the boundary.
type Bridge interface {
	ai.AIView
	ai.EconomyControl
	ai.ProductionControl
	ai.WaveSource
}

// Config carries the per-match setup a controller cannot derive from the AI
// surfaces alone: which player it is, its difficulty knob, the resource ids for
// gold/wood, and the staging (own) and target (enemy base) points. These are
// match facts, not sim state — supplied at construction, never read from the sim.
type Config struct {
	Self       int
	Difficulty int
	GoldID     int
	WoodID     int
	GatherX    int32
	GatherY    int32
	EnemyX     int32
	EnemyY     int32
	// FormationRadius / GatherTicks tune the wave manager.
	FormationRadius int32
	GatherTicks     uint32
}

// Controller is one player's melee AI.
type Controller struct {
	strat *Strategy
	cfg   Config

	br Bridge
	bo *ai.BuildOrder
	wm *ai.WaveManager

	// observable counters for FSV (no sim peeking required to read them).
	firstWaveTick uint32
	wavesLaunched int
	lastWaveID    uint32
}

// NewController binds a controller for cfg.Self running strat over the bridge.
// The build order is seeded from the strategy table; the wave manager is bound
// to the same bridge (its WaveSource side).
func NewController(strat *Strategy, cfg Config, br Bridge) *Controller {
	items := make([]ai.BuildItem, 0, len(strat.Build))
	for _, b := range strat.Build {
		items = append(items, ai.BuildItem{TypeID: b.Type, Count: b.Count})
	}
	bo := ai.NewBuildOrder(br, cfg.Self, cfg.GatherX, cfg.GatherY, items)
	fr := cfg.FormationRadius
	if fr <= 0 {
		fr = 96
	}
	gt := cfg.GatherTicks
	if gt == 0 {
		gt = 8
	}
	wm := ai.NewWaveManager(br, fr, gt)
	return &Controller{strat: strat, cfg: cfg, br: br, bo: bo, wm: wm}
}

// Step runs one AI tick. Wrap it with ai.NewFuncController to install it on a
// domain context. It pulls `now` from the view, so it needs no scheduler import.
func (c *Controller) Step() {
	now := c.br.Now()
	self := c.cfg.Self
	d := c.cfg.Difficulty

	// 1. Economy: top up harvesters toward the difficulty-scaled targets. The
	//    Economy layer assigns only the shortfall (idle workers), never churns.
	econ := ai.NewEconomy(c.br, self)
	if g := c.strat.GoldWorkerTarget(d); g > 0 {
		econ.SetHarvest(c.cfg.GoldID, g)
	}
	if w := c.strat.WoodWorkerTarget(d); w > 0 {
		econ.SetHarvest(c.cfg.WoodID, w)
	}

	// 2. Build order: one placement attempt per tick (retries a blocked item).
	c.bo.Tick()

	// 3. Army: maintain the standing army toward target. done = completed
	//    soldiers from the view; pending = in training/queued.
	soldier := c.strat.Army.SoldierType
	done := c.br.UnitCount(self, soldier)
	pv := ai.NewProductionView(c.br, self)
	target := c.strat.ArmyTarget(d)
	for have := done + pv.Pending(soldier); have < target; have++ {
		if _, reason := c.br.TrainForPlayer(self, soldier); reason != ai.TrainOK {
			break // refused (no producer / no gold / queue full): stop this tick
		}
	}

	// 4. Waves: one rolling wave. When none is in flight and enough soldiers are
	//    ready, stage a fresh wave that drafts all available soldiers and
	//    attack-moves the enemy base. The manager handles gather→launch→prune.
	if c.wm.ActiveWaves() == 0 && done >= c.strat.Waves.Size {
		id := c.wm.Stage(self, c.cfg.GatherX, c.cfg.GatherY, c.cfg.EnemyX, c.cfg.EnemyY,
			[]ai.Quota{{TypeID: soldier, Count: 10_000}}, now)
		if id != 0 {
			c.wavesLaunched++
			c.lastWaveID = id
			if c.firstWaveTick == 0 {
				c.firstWaveTick = now
			}
		}
	}
	c.wm.Tick(now)
}

// --- FSV read surface (no sim access required) ---

// FirstWaveTick is the tick the controller staged its first wave (0 if none).
func (c *Controller) FirstWaveTick() uint32 { return c.firstWaveTick }

// WavesLaunched counts the waves staged so far.
func (c *Controller) WavesLaunched() int { return c.wavesLaunched }

// BuildComplete reports whether the whole build order has been placed.
func (c *Controller) BuildComplete() bool { return c.bo.Complete() }

// BuildIssued returns placements issued for build item i.
func (c *Controller) BuildIssued(i int) int { return c.bo.Issued(i) }

// ActiveWaves exposes the wave manager's live wave count.
func (c *Controller) ActiveWaves() int { return c.wm.ActiveWaves() }
