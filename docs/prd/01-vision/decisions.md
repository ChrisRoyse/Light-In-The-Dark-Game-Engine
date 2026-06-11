# Decision Record — 2026-06-11

Decisions on the open questions of [Risks and Open Questions §2](./risks-and-open-questions.md),
recorded per the review-cadence rule (§3 of that document). Owner sign-off: Paul Ascenzi.

---

## D-2026-06-11-1 (Q1) — Sim math: fixed-point `int64` 32.32

**Decision.** Adopt fixed-point `int64` 32.32 as the planning assumption for all gameplay
math. The M1 spike still runs, but its purpose shifts: it validates the fixed-point
implementation's performance against the ≤ 10 ms tick budget and calibrates range/precision,
rather than arbitrating between representations. Ordered-float is dropped as a candidate
unless M1 shows fixed-point cannot meet the tick budget — the only condition that reopens
this.

**Rationale.** Determinism is gate #1 in goal precedence, and now load-bearing twice over:
the multiplayer commitment (D-2026-06-11-5) makes cross-machine bit-identity a shipping
feature, not just a testing aid. Fixed-point is deterministic by construction on every
OS/arch; ordered-float would carry a permanent proof burden (FMA contraction, `math` package
ulp drift across architectures) for marginal benefit.

## D-2026-06-11-2 (Q2) — Naming: idiomatic Go only, no JASS aliases

**Decision.** The public API ships idiomatic Go names exclusively. The generated JASS→Go
mapping table (one row per all 2,521 source functions, from `api-manifest.json`) is the
migration aid, published with the docs. No alias layer.

**Rationale.** Aliases double the visible symbol count against G2, drift unless generated,
and weaken the shape-only IP posture (R5). M2's sample-port exercise remains as validation;
if a table-driven port proves genuinely painful, that finding reopens the question with
evidence — until then this is settled.

## D-2026-06-11-3 (Q3) — Terrain: tile meshes (KayKit style) for v1

**Decision.** v1 terrain is square-grid tile meshes in the KayKit visual style, chunk-merged
per the batching plan. Heightmap-with-cliffs terrain is a v2 renderer candidate behind the
same sim-side grid abstraction (the sim sees walkability and cliff levels, never the mesh).

**Rationale.** Zero-cost asset availability dominates (G4): the CC0 tile corpus is ready
today, heightmap texturing/cliff art has no free source. The locked RTS camera makes the
tile aesthetic read well. The rendering spec's five M4 flip criteria
([terrain.md](../05-rendering/terrain.md)) stay as the escape hatch if tiles fail in
practice.

## D-2026-06-11-4 (Q4) — `commonai` natives: defer to v2

**Decision.** All `commonai` natives (and the ~8 AI-related `common.j` natives) are
classified and tombstoned `deferred-v2` in the M2 manifest. The M6 vertical slice's opponent
is a Go-scripted skirmish AI written against the ordinary public API. The map-side AI hooks
(`StartMeleeAI` analogues) are stubbed so melee mode is structurally complete.

**Rationale.** Porting the JASS AI domain means a second sandboxed scheduler with
command-stack messaging (R-EXEC-3) — real architectural work that v1's product need does not
justify. The isolated-domain design means v2 AI bolts on beside the v1 API without breaking
it, so deferral is cheap to reverse. G1 accounting stays loss-free: every symbol is in the
manifest with a recorded reason.

## D-2026-06-11-5 (NEW — Q5) — Multiplayer: committed, lockstep, milestone M7

**Question.** Is networked multiplayer possible, and is it in scope?

**Decision.** Yes and yes. Multiplayer is **promoted from "v2 maybe" to a committed roadmap
milestone M7** (post-M6 vertical slice). Architecture: **deterministic lockstep** — the
model WC3 itself used — over the existing command-stream design:

- Each client runs the full deterministic sim; only player **commands** are exchanged
  (~bytes per player per tick), not game state. This is why an RTS with 500 units can run
  on a dial-up-era protocol — and why it costs nothing extra on our stack.
- The v1 command-stream encoding ([input.md §8](../07-platform/input.md)) is already the
  wire format: versioned, tick-stamped, fixed-point payloads, serialized before the sim
  sees anything.
- `Game.StateHash()` (R-FSV-2) doubles as the desync detector: clients exchange hashes
  every N ticks; divergence is detected immediately and bisected per-system via the
  sub-hash design ([determinism.md](../04-simulation/determinism.md)).
- M7 scope: transport (UDP with reliability layer, or QUIC), lockstep scheduler with input
  delay + stall handling, lobby/session bootstrap, 2–8 players, desync detection +
  diagnostic dump. Replay files and netplay share one format: a replay *is* the recorded
  command stream of a session.

**What changes now (v1 scope guards, renamed from NG4):**
1. NG4 is rephrased: not "no multiplayer," but "no *netcode* before M7" — determinism and
   the command stream stop being groundwork-for-someday and are now load-bearing for a
   committed feature.
2. G5 criteria gain teeth: the replay-verification gate (G5.3) is the lockstep-readiness
   proof and becomes a hard M6 exit criterion — M7 must not discover determinism debt.
3. Every design decision touching sim state or timing must answer "does this survive
   lockstep?" from M3 onward (no per-client state in sim, no local-player branches inside
   the tick, `GetLocalPlayer`-style natives confined to the render/presentation context as
   already specified in the players category mapping).

**Feasibility verdict.** Not just possible — the architecture was already shaped for it.
The marginal cost of M7 is transport + lobby + stall handling; the hard part (bit-identical
simulation) is paid for by G5 regardless.

---

## Master PRD synchronization

PRD §9 (Open Questions) and §2.2/NG4 (multiplayer non-goal) updated to reference this
record; M7 added to PRD §7. The questions section of
[risks-and-open-questions.md](./risks-and-open-questions.md) carries per-question status
pointers here.
