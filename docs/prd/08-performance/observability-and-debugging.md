# Observability and Debugging

> Implements D-2026-06-11-34. Companion to [budgets-and-benchmarks.md](./budgets-and-benchmarks.md)
> and [gc-discipline.md](./gc-discipline.md). Requirement IDs: R-OBS-*.

Goal: any bug reported from the field is reproducible and diagnosable from a single
bundle, and any performance regression is visible the tick it happens.

## 1. Structured logging — `litd/obs` (R-OBS-1)

- Ring buffer, fixed capacity (default 65,536 entries ≈ last ~5 min), preallocated at
  startup. **Zero allocations per log call** on tick/frame paths (R-GC-1): value-type
  fields only (int64/F64/EntityID/enum), no fmt.Sprintf in hot paths, message = interned
  ID into a string table, formatting deferred to dump time.
- Entry: `(tick, frame, level, channel, msgID, 4×int64 args)` — 64 bytes fixed.
- Levels: ERROR, WARN, INFO, DEBUG, TRACE. Release builds: ERROR/WARN live to disk file,
  everything to ring buffer. Debug builds: configurable per channel.
- Channels = subsystems: `sim.tick`, `sim.path`, `sim.combat`, `sim.sched`, `render`,
  `asset`, `lua`, `ai`, `net`, `audio`, `ui`. New subsystem ⇒ new channel (review gate).
- Errors are loud (fsv.md): every ERROR carries cause + state context; no silent
  fallbacks anywhere in the engine — a swallowed error is a P1 bug.

## 2. Perf counters (R-OBS-2)

Live counters, zero-alloc, sampled per tick/frame, ring-buffered history (10 min):

| Counter | Budget link |
|---|---|
| tick ms total + per phase (7 phases) | ≤ 10 ms (§5.3) |
| frame ms, FPS | ≥ 60/30 FPS |
| draw calls, batches, instances | ≤ 300 |
| allocs/frame, allocs/tick, heap MB | 0 / 0 / ≤ 1.5 GB |
| path expansions/tick, queue depth | ~100k budget |
| active units/missiles/buffs | 1,000/1,000 caps |
| voices active | 32 |
| net: turn RTT, input-delay turns, hash-lag | M7 |
| lua: instructions/tick per script, mem | R-SEC-1 quotas |

- **F11 overlay** renders counters in-engine (graph + current/worst). ≤ 5 draw calls.
- Counters export to benchmark harness format (benchstat-comparable) and to telemetry.

## 3. Debug-report bundle (R-OBS-3)

One keypress (F10) or `-report-on-exit` flag writes `litd-report-<buildhash>-<ts>.zip`:

| Content | Source |
|---|---|
| ring-buffer log (formatted) | R-OBS-1 |
| state dump JSON + StateHash + per-system sub-hashes | R-FSV-2 |
| **running replay (command stream from match start)** | G5.3 machinery |
| perf counter history (10 min) | R-OBS-2 |
| sysinfo: OS, GPU, driver, CPU, RAM, locale | startup probe |
| engine build hash + world archive hash | build/load info |

**The replay is the feedback loop:** determinism (G5) means the bundle reproduces the
bug bit-identically — `litd -replay report.zip` headless reruns it; sub-hash divergence
pinpoints the failing system; fix verified by replaying again. Field bug → repro → fix
without ever asking the reporter a question.

## 4. Telemetry (R-OBS-4) — opt-in, anonymous, off by default

- Perf summary per session (percentile tick/frame ms, GPU string, preset) + crash
  signature (stack hash, build hash). POST to own-site endpoint (D-22).
- No gameplay content, no PII, no world data. One-line JSON, user-inspectable before
  send in the config. Endpoint + ingestion dashboard are M6 infra (#185/#186 adjacent).

## 5. Dev tooling (R-OBS-5)

- Dev builds expose `net/http/pprof` + `runtime/trace` capture on localhost flag.
- In-game debug console (backquote): Lua REPL against the sandboxed API + obs commands
  (`obs.dump()`, `obs.counters()`, `obs.loglevel("sim.path", TRACE)`).
- Desync auto-bisect: given two diverging replays/hash traces, walk per-system
  sub-hashes to first divergent (tick, system); print both states' relevant slice.
- CI: every benchmark run uploads counter history as artifact; regression report links
  the exact counter graph.

## 6. Acceptance

- R-OBS-1..3 land with their milestones (logger M1, counters M3/M4, bundle M6) — every
  later subsystem must register channels/counters at code-review gate.
- FSV evidence for observability features is the artifact itself: a generated bundle
  opened and inspected, a forced ERROR appearing in ring + disk, a counter graph
  matching an induced spike.
