# Timers — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §4.4 R-EXEC-5](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~20** | timer CRUD, start/pause/resume, elapsed/remaining/timeout queries (timer-dialog natives counted under [ui-frames-and-dialogs](ui-frames-and-dialogs.md)) |
| `blizzard.j` BJs | **~22** | `CreateTimerBJ`, `StartTimerBJ`, `GetLastCreatedTimerBJ`, polled-wait plumbing |

## Representative JASS signatures

```jass
native CreateTimer          takes nothing returns timer
native DestroyTimer         takes timer whichTimer returns nothing
native TimerStart           takes timer whichTimer, real timeout, boolean periodic, code handlerFunc returns nothing
native PauseTimer           takes timer whichTimer returns nothing
native ResumeTimer          takes timer whichTimer returns nothing
native TimerGetElapsed      takes timer whichTimer returns real
native TimerGetRemaining    takes timer whichTimer returns real
native TimerGetTimeout      takes timer whichTimer returns real
constant native GetExpiredTimer takes nothing returns timer

function CreateTimerBJ takes boolean periodic, real timeout returns timer
function StartTimerBJ takes timer t, boolean periodic, real timeout returns timer
function GetLastCreatedTimerBJ takes nothing returns timer
function PolledWait takes real duration returns nothing
```

## Canonical Go surface

```go
type Timer struct{ /* opaque handle into sim scheduler */ }

// One-shot and periodic collapse onto two clear entry points (the "complex version"
// is TimerStart; the boolean flag splits into named constructors for readability):
func (g *Game) After(d time.Duration, f func()) Timer   // one-shot
func (g *Game) Every(d time.Duration, f func(t Timer)) Timer // periodic

func (t Timer) Pause()
func (t Timer) Resume()
func (t Timer) Stop()                 // DestroyTimer; GC owns the handle
func (t Timer) Elapsed() time.Duration
func (t Timer) Remaining() time.Duration
func (t Timer) Timeout() time.Duration
```

`GetExpiredTimer` (thread-local "current timer" accessor) disappears: the periodic
callback receives its `Timer` as a parameter — same capability, no hidden global,
consistent with R-EXEC-4's elimination of implicit current-element state.

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthroughs dropped | `DestroyTimerBJ`, `PauseTimerBJ` → `Timer.Stop()`/`Pause()` |
| **D2** | create+start convenience BJs collapse | `CreateTimerBJ(periodic, timeout)` + `StartTimerBJ` → `g.After`/`g.Every` (creation and start are one act; `GetLastCreatedTimerBJ` side-channel deleted — constructor returns the timer) |
| **D3** | no Loc variants in this category | — |
| **D4** | `PolledWait` (busy-poll wait that degrades gracefully under game-speed changes) kept once | `helpers.Wait(d)` — documented in [triggers-and-events](triggers-and-events.md) |
| **D5** | getter family stays as three typed getters (already minimal) | `Elapsed`/`Remaining`/`Timeout` |

`TriggerRegisterTimerEvent`/`TriggerRegisterTimerExpireEvent` duplication with
`TimerStart` collapses: **timer callbacks are the canonical path**; the event-bus route
(`OnEvent(EventTimerExpired, WithTimer(t))`) is tombstoned as superseded.

## Subsystem dependencies

- **sim** (primary): the timer wheel is sim-owned and tick-driven. All timeouts quantize to 50 ms sim ticks (R-EXEC-5) — `time.Duration` in signatures is converted to ticks at call time, never wall clock (R-SIM-2).
- **render**: none. (Timer *dialogs* — the on-screen countdown window — are UI; see [ui-frames-and-dialogs](ui-frames-and-dialogs.md).)
- **asset**: none.

## Porting hazards

1. **`time.Duration` is a UX convenience, not a precision promise.** Internally ticks only. Document that `After(75*time.Millisecond)` fires at tick boundary (next multiple of 50 ms). Sub-tick timing must not exist (R-EXEC-5).
2. **Expiry ordering**: multiple timers expiring on the same tick must fire in deterministic order (creation sequence number, not heap-pop order ties). This is a classic replay-divergence bug.
3. **Callbacks are sim-context code**: they run inside the tick on the cooperative scheduler (R-EXEC-1), may call `helpers.Wait`, and must not allocate steady-state (R-GC-2: timer entries pooled).
4. **Pause/Resume vs game pause**: WC3 timers freeze with the game clock but `PolledWait` interacts with game speed. Define LitD rule: all timers follow sim ticks; game pause stops the sim, hence all timers — one rule, no special cases.
5. **Handle reuse**: WC3 maps leak/destroy timers aggressively and rely on handle-id recycling tricks (hashtable keys). LitD `Timer` handles are GC-managed and never recycled into a different identity — document this as an intentional semantic upgrade.
