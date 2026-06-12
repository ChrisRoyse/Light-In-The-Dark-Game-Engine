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

## D-2026-06-11-3 (Q3) — ~~Terrain: tile meshes for v1~~ SUPERSEDED by D-2026-06-11-7

**Decision.** v1 terrain is square-grid tile meshes in the KayKit visual style, chunk-merged
per the batching plan. Heightmap-with-cliffs terrain is a v2 renderer candidate behind the
same sim-side grid abstraction (the sim sees walkability and cliff levels, never the mesh).

**Rationale.** Zero-cost asset availability dominates (G4): the CC0 tile corpus is ready
today, heightmap texturing/cliff art has no free source. The locked RTS camera makes the
tile aesthetic read well. The rendering spec's five M4 flip criteria
([terrain.md](../05-rendering/terrain.md)) stay as the escape hatch if tiles fail in
practice.

## D-2026-06-11-4 (Q4) — ~~`commonai` natives: defer to v2~~ SUPERSEDED by D-2026-06-11-6

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

# Decision Record — 2026-06-11 (second session: feature-scope decisions)

Owner-decided via structured questions. Standing directive from the owner: **features are
not cut or deferred because they are hard** — deferral requires a product reason, and
"hard" alone is not one. Two earlier same-day decisions are reversed accordingly.

## D-2026-06-11-6 — `commonai`: FULL v1 port (supersedes D-2026-06-11-4)

The JASS AI domain ships in v1: second sandboxed scheduler domain, isolated script contexts
(no shared globals with map scripts), command-stack messaging per R-EXEC-3, all ~123
`common.ai` natives plus the AI-related `common.j` natives mapped canonically (no
`deferred-v2` tombstones for capability reasons). Scheduled as its own milestone **M5.5**
after the core API (M5), before the vertical slice — M6's melee opponent runs on the real
AI domain, not a Go stopgap.

## D-2026-06-11-7 — Terrain: heightmap + cliffs in v1 (supersedes D-2026-06-11-3)

WC3-fidelity terrain ships in M4: heightmap mesh, discrete cliff levels with ramps, texture
splatting, chunked rendering aligned to the pathing grid. We accept authoring the terrain
art ourselves; the generative asset pipeline (D-2026-06-11-12) covers splat textures and
cliff texture sets. The sim-side grid abstraction is unchanged (R-SIM-5; the sim never sees
the mesh). Tile-mesh rendering remains possible later behind the same abstraction, but is
no longer the v1 plan. [terrain.md](../05-rendering/terrain.md) flips its recommendation.

## D-2026-06-11-8 — Lua scripting in v1 (M5)

Deterministic embedded Lua (gopher-lua family, audited for determinism) bound to the
canonical API, with bindings **generated from `api-manifest.json`** alongside the Go API in
M5. Worlds are runtime-loadable: creators and AI coding agents author without a Go
toolchain or recompile. Go remains the systems language; Lua is the creation surface.

## D-2026-06-11-9 — Full save/load in v1

Mid-game saves are v1 scope. The cooperative scheduler is designed **serializable from day
one (M3)**: suspended script coroutines, timers, and event subscriptions all serialize into
the save format. Replays (command streams) remain a separate, complementary mechanism.
This constrains the M1/M3 scheduler design choice toward a serializable representation
(stackless/state-machine coroutines or fully descriptive suspension records).

## D-2026-06-11-10 — World Editor: committed milestone M8

In-engine visual editor after multiplayer: terrain sculpt/paint, unit/doodad placement,
map metadata, save to the world archive format. Trigger-GUI authoring comes later;
Lua covers logic until then. Campaign flow UI also lands here (D-2026-06-11-15).

## D-2026-06-11-11 — Web/WASM target: no (desktop only stands)

NG3 unchanged. Re-examinable post-v1; no spike scheduled.

## D-2026-06-11-12 — Asset gaps: generative pipeline at asset-build time

Portraits, spell VFX textures, voice lines, UI icons with no CC0 source are produced by a
**build-time generative pipeline** (image models, TTS), curated by hand, committed as
ordinary owned assets with provenance entries. Zero runtime AI (G4.6 intact). The pipeline
is tooling (`tools/assetgen`), documented in 06-assets.

## D-2026-06-11-13 — Doodads: full WC3 parity

Doodads get optional handles: scripts can show/hide, animate, and reposition scenery
(`SetDoodadAnimation` analogues map canonically, not position-addressed workarounds).
Render-only remains the default storage mode until a doodad is first addressed by script
(promotion on first touch), so the zero-cost case stays zero-cost.

## D-2026-06-11-14 — World sharing: open archive format in v1, hosted hub later

v1 defines a single-file world archive (zip-based: map data + Lua + custom assets +
manifest with content hashes and engine-version requirements), loadable from disk and
documented publicly. A hosted world repository with in-game browser is a committed
post-M8 follow-on (M9 candidate), and the format carries the hosting metadata from day one.

## D-2026-06-11-15 — Campaigns: persistence architecture in v1, UI at M8

Cross-map persistent state (game-cache semantics, hero carry-over) is built into the sim
and save format in v1 — retrofit is brutal, build-in is cheap. Campaign menu/mission-flow
UI ships with the M8 editor milestone.

## D-2026-06-11-16 — Replays/observers: viewer controls at M7

M6 exit: replays record and verify headlessly (CI artifact). M7 ships the in-client replay
viewer (pause/speed/free-camera/per-player perspective) and live observer slots — observers
are replay viewers at zero delay over the same machinery.

## D-2026-06-11-17 — i18n: string tables from M4, English shipped

Every user-facing string (engine UI and world-author strings) flows through locale tables
from M4 onward. v1 ships English; translations are pure data drops.

## D-2026-06-11-18 — Scale: 1,000-unit stretch target

ECS capacities, pathfinding, and budgets are provisioned for **1,000 units + 1,000
projectiles** from M3. The low-tier reference machine guarantee stays at 500 units
(existing §5.3 budgets); 1,000 becomes the recommended-spec target. Render side requires
the instancing patch to land (R2 trigger now assumed fired — plan for it in M4, not as
contingency).

## D-2026-06-11-19 — Distribution and engine license: decided at M6, repo private until then

No distribution-specific engineering before the vertical slice exists. Repo stays private;
open-sourcing question (Apache-2.0 was the staff recommendation) is bundled into the M6
distribution decision.

## D-2026-06-11-20 — Shared-world security: hard sandbox

World Lua runs in a no-io/no-os/no-net VM exposing only the game API, with per-tick
instruction and memory quotas. The quotas double as the lockstep stall guard. Worlds cannot
touch the player's machine. Non-negotiable gate for any sharing feature (M9 hub blocks on
it; disk-loaded worlds get the same sandbox from M5).

---

## Milestone impact summary (post-decisions roadmap)

| Milestone | Additions from this record |
|---|---|
| M1 | Scheduler representation must be serializable (D-9) |
| M3 | 1,000-unit capacities (D-18); serializable scheduler implementation (D-9); campaign persistence hooks (D-15) |
| M4 | Heightmap terrain (D-7); string tables (D-17); instancing patch planned-in (D-18) |
| M5 | Lua VM + generated bindings + hard sandbox (D-8, D-20); doodad handle promotion (D-13) |
| **M5.5** | **NEW: AI domain port — full `commonai` (D-6)** |
| M6 | Save/load shipping (D-9); world archive format (D-14); distribution + license decision (D-19) |
| M7 | Multiplayer (D-5) + replay viewer/observers (D-16) |
| **M8** | **NEW: World Editor + campaign UI (D-10, D-15)** |
| **M9** | **Candidate: hosted world hub (D-14), gated on sandbox (D-20)** |

---

# Decision Record — 2026-06-11 (third session: all-upfront decisions + spike results)

Owner directive: **all spikes completed and all decisions made upfront — nothing waits for
a later milestone to decide.** Product decisions by owner; technical decisions validated by
executed spikes (code in `spikes/`, results below) and sourced research.

## D-2026-06-11-21 — License: proprietary, permanently

The engine is **closed-source, proprietary, permanently** (supersedes the D-19 "decide at
M6" deferral and the Apache-2.0 staff recommendation). Public surface: the world archive
format specification and the Lua scripting API documentation — creators build on the
platform without engine source. All dependencies must remain permissive
(BSD/MIT/Apache) — copyleft is now a hard dependency exclusion (G4.1 allowlist tightened:
no GPL/AGPL/LGPL anywhere in the tree).

## D-2026-06-11-22 — Distribution: own site only

Downloads ship from a Light in the Dark Analytics site. No Steam, no itch.io, no GitHub
releases. Full funnel control; discovery is a marketing problem, not an engineering one.
No third-party SDKs enter the tree.

## D-2026-06-11-23 — World hub: committed M9 (no longer "candidate")

Hosted world repository + in-game browser, M9 firm. Architecture: static-friendly index
(no account needed to download), accounts/ratings later. Gated on the Lua hard sandbox
(D-20). The hub backend co-hosts the M7 session relay (D-26).

## D-2026-06-11-24 — Flagship game: real product, grows every milestone

The M6 vertical slice is **v0.1 of the actual Light in the Dark game** — not a tech demo.
Art style, factions, and lore are established now (generative pipeline, D-12) and every
milestone ships a better version of the same game. Engine and game prove each other.

## D-2026-06-11-25 — Lua VM: forked gopher-lua, vendored (spike: research memo)

`yuin/gopher-lua` wins on the three hard requirements: VM-level coroutines are plain heap
data (serializable — the only credible pure-Go option; arnodel/golua uses goroutines =
unserializable; Shopify/go-lua has no coroutines at all), `pairs()` iteration is
insertion-ordered (never ranges a Go map), and number→string is pure-Go strconv.
Vendored fork in `repoes/` (LITD-PATCH discipline) with four patches:
1. instruction-budget counter in `mainLoop` (R-SEC-1 quota + lockstep tick budget),
2. deterministic `mathlib` replacement (fixed-point/table-based; `math.random` → sim PRNG)
   — Go's `math` package is NOT cross-arch bit-identical (golang/go#20319),
3. coroutine/LState persister (frames, registry, upvalues; protos by chunk-id),
4. LState/callframe pooling (R-GC-1) + golden cross-arch determinism CI test.
Performance ~5-10× C Lua: adequate; hot paths are Go sim code.

## D-2026-06-11-26 — Transport: quic-go, star topology (spike: research memo)

M7 networking: **quic-go** (MIT, mature, RFC 9221 datagrams) over a **star topology** —
LAN: a player's engine hosts in-process; internet: the same host loop runs on a lightweight
relay co-located with the M9 hub, eliminating NAT traversal entirely (hole-punching QUIC is
still IETF-draft in 2026; pion/webrtc is the runner-up if relay economics ever fail).
Command turns every 2–4 sim ticks, adaptive input-delay buffer (start 2 turns), reliable
stream for turns + hashes, stall = pause + grace-period drop, 64-bit state hash piggybacked
~1/s. Build hash + seed exchanged at join; mismatch refuses the session.

## D-2026-06-11-27 — Spike S1 result: fixed-point int64 32.32 VALIDATED

`spikes/fixedpoint`: representative tick math (movement integration, distance/sqrt, damage
accumulation) for 2,000 entities = **182 µs/tick = 1.8% of the 10 ms budget** (float64
baseline 28 µs — the 6.5× ratio is irrelevant at this absolute cost). 10k-tick state hash
bit-stable across repeated runs; 4-hour timer and DPS accumulators exact; coordinate range
has 5 decimal orders of headroom. Zero allocs/tick. **D-1 confirmed; M1 reduces to wiring
this into CI across the OS/arch matrix.**

## D-2026-06-11-28 — Spike S2 result: stackless serializable scheduler VALIDATED

`spikes/scheduler`: scripts as descriptive suspension records (PC + locals + typed
suspension), sleep queue keyed `(wakeTick, seq)`, event waiters FIFO by seq. Full scheduler
state gob-serialized **mid-run**, restored, and advanced — traces and state bit-identical
with the uninterrupted run; resume order deterministic. **The goroutine-baton design is
dead; stackless descriptive suspension is THE scheduler design (R-SIM-6, D-9), not a
candidate.** M1's remaining scheduler work is production hardening, not design choice.

## D-2026-06-11-29 — Spike S3 result: pathfinding architecture SET

`spikes/pathfind` (512×512 grid, 20% random blockers — pathological for A*): single
long-distance path ≈ 5.1 ms / ~15.7k expansions; 1,000 simultaneous full repaths ≈ 2.3 s —
**no flat A* fits 1,000-unit worst cases in a tick.** Architecture locked: amortized
request queue with **counted expansion budget per tick** (~100k expansions ≈ 1–2 ms),
**HPA\* hierarchy** (10–50× expansion cut) mandatory not optional, **path sharing** for
group orders, **flow fields** for shared-goal moves ≥ ~40 units. Deterministic
`(f, h, seq)` tie-breaking validated (identical expansion counts across runs).

## D-2026-06-11-30 — Spike S4 result: g3n instancing patch CONFIRMED VIABLE

Vendored `repoes/engine/gls/glapi.c` already loads `glDrawArraysInstanced`,
`glDrawElementsInstanced`, `glDrawElementsInstancedBaseVertex`, `glVertexAttribDivisor`
(lines 480–530, 831–881). The LITD-PATCH adds Go-side `gls` wrappers + an `InstancedMesh`
graphic + per-instance transform/team-color attribute buffer. No GL capability risk;
scheduled M4 per D-18.

## D-2026-06-11-31 — Spike S5 result: g3n skinned GLB animation VALIDATED (risk R1)

`cmd/animtest` (kept as a permanent asset-smoke tool) loaded all 5 KayKit
Character Pack Adventures GLBs (CC0, github.com/KayKit-Game-Assets) in the vendored g3n:
**76 animation clips and a 41-joint skin per character, every model parsed, rendered
skinned + atlas-textured, and animated** — FSV evidence: 3 staggered screenshots per model,
all byte-distinct (~96k differing bytes frame 0→2 = visible arm/sword motion), characters
clearly posed (not bind-pose). The clip inventory (Idle/Walking/Running/Attack/Death/
Spellcast/Hit…) exceeds the R-AST-3 required set. R1's "patch trigger" stands for future
packs (Quaternius pack pending — Google-Drive-hosted, same Blender exporter class); the
per-pack census stays in M0 asset ingestion via `animtest`.

## D-2026-06-11-32 — Flagship game identity: "Light in the Dark" on the world of Veil

Full spec: [10-game/identity.md](../10-game/identity.md). Owner-decided after researched
synthesis of WC3 (lore structure + art direction), AoW4 (progression DNA), AoM Retold
(studied, deity layer declined):

- **Title:** Light in the Dark (game = engine = brand); v0.1 subtitle *First Flame*.
- **Premise:** sunless world of Veil; civilization huddles around dying Beacons amid the
  living dark (the Gloam). Original lore throughout (NG1 guardrail).
- **Factions:** 4 asymmetric playable (Vigil / Gloamborn / Unbound / Rootkin — defend,
  embrace, outrun, outlast the dark) + the Dark as pure unplayable antagonist (never
  gathers, never builds — the Burning Legion lesson).
- **No deity/god-power layer** (AoM mechanic declined): heroes with abilities carry the
  magic — skill trees on level-up, tiered gear, in-match **Grimoire** research tracks,
  capstone army **transformations** (AoW4 DNA, all game-layer data + scripts).
- **Light as core mechanic:** beacons/portable flame/blight as faction texture; the
  Flicker as day/night analogue; fog of war is the antagonist made visible.
- **Art direction (binding on assetgen + asset curation):** original-WC3 rules — comic-book
  fantasy, exaggerated proportions, silhouette-first, hand-painted flat-color atlas
  textures, saturated faction palettes, medieval + magic only.
- **v0.1 (M6):** Vigil + Unbound playable, one skirmish map with Beacon/Flicker, heroes +
  one grimoire track each, vs the M5.5 AI.

## D-2026-06-11-33 — Collaborative world development: source-form worlds, VCS-native

Owner requirement: multiple people must be able to work on one map/mod together
("maybe they will use GitHub or whatever"), enabling bigger community projects, at
minimum cost.

**Decision.** Worlds have two forms:

1. **Source form (the editing + collaboration format):** a plain directory —
   `world.toml` (metadata), `data/*.toml` (units/abilities/upgrades), `scripts/*.lua`,
   `map/` (terrain heightmap + entity placements in line-stable text), `assets/`
   (binary GLB/OGG/PNG referenced by manifest). Every text file written with **stable
   key ordering and line-oriented layout** so git diff/merge works cleanly; binary
   assets are whole-file replaced (standard git behavior, LFS-compatible). Any VCS or
   none — we ship no collaboration infrastructure.
2. **Archive form (`.litdworld`, the distribution format, D-14):** built from source
   form by `tools/worldpack` with **deterministic packing** (sorted entries, fixed
   timestamps) → byte-identical archive from identical source → content hashes stable
   across builders, which the M9 hub and M7 join-guard already rely on.

The M8 editor reads and writes **source form** (archive export = one button). The
engine loads both. Real-time co-editing (CRDT/OT) is explicitly **not** built — git-style
asynchronous collaboration satisfies the requirement at ~zero engineering and $0 infra
cost (cost-effectiveness directive), and the source-form decision is what a future
realtime layer would need anyway.

---

## No remaining deferred decisions

| Formerly open | Now |
|---|---|
| M1 fixed-point vs float spike | DONE — validated (D-27) |
| M1 scheduler representation | DONE — stackless (D-28) |
| Lua VM choice + determinism audit scope | DONE — gopher-lua fork, 4 patches (D-25) |
| M3 pathfinding A* vs flow fields | DONE — layered architecture (D-29) |
| M3/M4 instancing investigation | DONE — patch confirmed viable (D-30) |
| M7 transport | DONE — quic-go star (D-26) |
| M6 license decision | DONE — proprietary forever (D-21) |
| M6 distribution decision | DONE — own site (D-22) |
| M9 hub candidacy | DONE — committed (D-23) |
| Flagship game identity | DONE — real product from M6 (D-24) |
| Mid-M4 terrain fallback checkpoint | Stays as a quality gate with a pre-decided fallback (tiles), not an open decision |

---

## Master PRD synchronization

PRD §9 (Open Questions) and §2.2/NG4 (multiplayer non-goal) updated to reference this
record; M7 added to PRD §7. The questions section of
[risks-and-open-questions.md](./risks-and-open-questions.md) carries per-question status
pointers here.
