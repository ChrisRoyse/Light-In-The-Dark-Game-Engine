# Timer Wheel — Public API & Lua Binding

> Value-type signatures, no G3N types (R-API-1..6). Two surfaces, identical semantics:
> Go (engine + Go scripting) and Lua (the surface AI agents most often target).

---

## 1. Go API (`litd/api`)

### 1.1 Handle

```go
// Timer is a value handle over a sim TimerID. Stale handle ⇒ all methods no-op.
type Timer struct { /* g *Game; id sim.TimerID */ }

func (t Timer) Valid() bool       // generation-checked liveness
func (t Timer) Cancel()           // idempotent
func (t Timer) Remaining() int    // fires left (TimerCount), or -1 for loop, 0 for done
func (t Timer) Paused() bool
func (t Timer) SetPaused(p bool)  // pause/resume without losing phase
```

### 1.2 Serializable creation (the path gameplay/abilities MUST use)

```go
// Continuation form — survives save/load (R-TMR-2).
func (g *Game) AfterCont(d time.Duration, c Cont, p Payload) Timer            // single
func (g *Game) LoopCont(d time.Duration, c Cont, p Payload) Timer             // loop
func (g *Game) CountCont(d time.Duration, n int, c Cont, p Payload) Timer     // count

// Cont names a continuation registered at world setup. Payload is the [4]int64 bundle.
type Cont uint16
type Payload struct{ A, B, C, D int64 } // packs EntityIDs/refs/scalars

// Owner binding (R-TMR-6): owned timers auto-cancel when the owner unit dies.
func (g *Game) AfterContOwned(d time.Duration, owner Unit, c Cont, p Payload) Timer
func (g *Game) LoopContOwned(d time.Duration, owner Unit, c Cont, p Payload) Timer
```

Registering a continuation (Go side, at setup):

```go
const ContSpawnWave Cont = /* assigned by the registry */

g.RegisterCont(ContSpawnWave, func(g *api.Game, p api.Payload) {
    spawner := g.UnitByID(sim.EntityID(p.A))
    waveKind := uint16(p.B)
    // ... spawn logic, possibly schedules the next wave via LoopCont
})
```

### 1.3 Convenience closure form (save-UNSAFE, transient only — R-TMR-8)

```go
// Sugar. Holds a Go closure in a non-serialized side table. A pending closure timer is
// DROPPED on load (deterministically logged). Use only for UI/debug, never gameplay.
func (g *Game) After(d time.Duration, fn func()) Timer
func (g *Game) Every(d time.Duration, fn func(t Timer)) Timer
```

A `go vet`-style lint (`presentlint`-adjacent) flags `After/Every` use inside
`litd/sim`-adjacent gameplay packages and ability templates.

## 2. Lua binding (`litd/luabind`)

The Lua surface is intentionally tiny and continuation-by-name. Because Lua functions are
themselves serialized by the gopher-lua persister (the coroutine path), the Lua binding
can offer a **function callback that is genuinely save-safe**, unlike the Go closure form,
by routing through the scheduler's serialized coroutine machinery.

```lua
-- single shot: run fn after 2.0 seconds (40 ticks)
local t = After(2.0, function()
    Spawn("skeleton", spawnPoint)
end)

-- loop: every 0.5s until cancelled
local t = Every(0.5, function(self)
    PulseDamage(centerPoint, 200, 250)  -- example
end)

-- count: 5 times, every 0.3s (the fireworks/seed-ring pattern)
local t = Times(0.3, 5, function(self, i)   -- i = 1..5
    SpawnProjectile("spark", RingPoint(centerPoint, 200, i * 72))
end)

CancelTimer(t)          -- idempotent
TimerRemaining(t)       -- int, or -1 for loop
SetTimerPaused(t, true)
```

> **Why the Lua callback is save-safe but the Go closure is not.** Lua callbacks become
> persisted coroutine continuations via the gopher-lua deterministic fork (the same
> mechanism that already serializes `PolledWait`). Go closures have no such persister.
> This asymmetry is deliberate: the AI-authoring surface (Lua) gets the ergonomic *and*
> robust path; Go gets the explicit continuation API for engine-level code.

### Owner binding in Lua

```lua
-- timer tied to a unit; auto-cancels when the unit dies
local t = EveryOwned(spawner, 10.0, function()
    RespawnCamp(spawner)
end)
```

## 3. Mapping to WC3 / JASS

| JASS | PRD2 |
|------|------|
| `TimerStart(t, timeout, periodic, code)` | `Every`/`After`/`LoopCont` with quantized `time.Duration` |
| `TimerGetRemaining` | `Timer.Remaining()` / `TimerRemaining(t)` |
| `PauseTimer` / `ResumeTimer` | `Timer.SetPaused(true/false)` |
| `DestroyTimer` | `Timer.Cancel()` / `CancelTimer(t)` |
| anonymous-function timer (save-unsafe in JASS too) | Lua function form (save-safe here) or `After` Go sugar (save-unsafe) |

## 4. Examples that motivated the design

### Enemy spawner (tutorial: enemy spawner)
```lua
-- when a camp is cleared, respawn it after 10 seconds, owned by the spawner region marker
EveryOwned(campMarker, 10.0, function() SpawnCamp(campMarker) end)
```

### Telegraphed boss skill (tutorial: boss state machine)
```lua
-- warn now, detonate after 3 seconds at the cast point
SpawnProjectile("warning", castPoint)
After(3.0, function()
    SpawnProjectile("explosion", castPoint)
    DamageArea(castPoint, 300, 500, DAMAGE_TRUE)
end)
```

### Channel with periodic ticks + hard stop
```lua
local pulses = Times(0.5, 6, function(self, i)
    HealArea(casterPoint, 250, 80)
end)
-- cancel early if the caster is interrupted
OnEvent(EVENT_ABILITY_INTERRUPT, function() CancelTimer(pulses) end)
```
