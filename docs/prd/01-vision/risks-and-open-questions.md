# Risks and Open Questions — Expanded

> Expands [PRD §8 (Risks)](../../PRD.md#8-risks) and [PRD §9 (Open Questions)](../../PRD.md#9-open-questions).
> Each risk gains **detection signals** (how we notice it materializing early) and a
> **trigger point** (the concrete condition that activates the mitigation/fallback). Each
> open question gains **decision criteria**, a **deadline milestone** (per
> [Milestones](../09-roadmap/milestones.md)), and a **recommended default** to adopt if the
> deadline arrives without a stronger answer.

---

## 1. Risks

### R1 — G3N glTF gaps (skinning edge cases, extensions)

*Likelihood: Medium · Impact: High*

**Mitigation (per PRD).** Core-profile-only assets (R-FMT-1); vendored engine with loader
patches; fallback to a [qmuntal/gltf](https://github.com/qmuntal/gltf) parser feeding G3N
meshes directly.

**Detection signals.**
- Asset-validation CLI flags KHR extensions in candidate CC0 packs during M0 ingestion —
  early census of how much re-export work the asset corpus needs.
- M0 smoke test: load and play every animation clip of every ingested model headlessly
  (scene-graph level) and on-screen; any wrong bind pose, missing skin weights, or silent
  animation failure is a signal.
- Loader warnings/ignored-property logs in G3N elevated to structured telemetry during
  asset ingestion, so "loaded but partially" is visible rather than silent.

**Trigger points.**
- *Patch trigger:* any core-profile GLB from a validated pack misrendering → patch the
  vendored loader (`repoes/engine`) within the current milestone; the asset is never
  "fixed" by editing it away from core profile.
- *Fallback trigger:* if by **end of M4** loader patches have consumed > 2 weeks cumulative
  effort or a core-profile skinning case remains broken, switch model parsing to the
  qmuntal/gltf front-end feeding G3N mesh/skeleton structures, keeping G3N for rendering only.

### R2 — No GPU instancing in G3N → draw-call ceiling

*Likelihood: Medium · Impact: Medium*

**Mitigation (per PRD).** Shared-material batching and static terrain chunk merging first;
GPU instancing patch in the vendored fork if the budget is missed (R-RND-3 investigation
scheduled in M3–M4).

**Detection signals.**
- Draw-call counter in the render benchmark scene (see [Tooling §4](../09-roadmap/tooling.md))
  tracked per CI run from M4; a rising trend toward the 300-call budget is the early signal,
  not the breach itself.
- Frame-time breakdown showing CPU-side draw submission (not GPU work) as the bottleneck on
  the reference machine — the signature of a draw-call-bound renderer.

**Trigger points.**
- *Investigation trigger (scheduled):* M3/M4 boundary — spike the instancing patch on the
  vendored fork regardless of current numbers, so the option is costed before it is needed.
- *Implementation trigger:* benchmark scene at max army size exceeds **250 draw calls**
  (early-warning threshold below the 300 budget) after batching/merging are fully applied →
  land the instancing patch inside M4.
- *Escalation:* if instancing cannot be made to work in the vendored fork by end of M4,
  reduce per-faction material variety (deepen atlas sharing) to buy headroom — an asset-side
  lever that needs no engine change.

### R3 — Float non-determinism across CPUs

*Likelihood: Medium · Impact: High*

**Mitigation (per PRD).** M1 spike; fixed-point fallback decided before any sim code is
written (R-SIM-2).

**Detection signals.**
- M1 spike harness: 10k-tick state-hash comparison across the full CI matrix
  (Windows/Linux/macOS × amd64/arm64) and across compiler versions — divergence anywhere is
  the signal.
- Post-M1 permanent guard: cross-platform hash-comparison job stays in CI forever
  (G5.2 in [Goals](./goals-and-non-goals.md)); any future divergence bisects to the
  offending change.
- Static lint for unordered float reductions and `math` calls outside the approved
  deterministic math package.

**Trigger points.**
- *Decision trigger (scheduled):* end of M1, hard deadline — see Open Question Q1 below.
- *Fallback trigger:* **any** cross-platform hash divergence in the ordered-float spike →
  adopt fixed-point (`int32` 16.16 or `int64` 32.32) for all gameplay math. One divergent
  run is sufficient; this risk is not negotiated down.
- *Late-discovery trigger:* divergence found after M3 (sim built) → treated as a release
  blocker; the deterministic math package boundary means the representation can be swapped
  without rewriting gameplay systems.

### R4 — API surface underestimation (natives needing engine features G3N lacks)

*Likelihood: High · Impact: Medium*

The PRD names fog of war, minimap, and selection circles as known custom shader/render-pass
work scheduled inside M4. The residual risk is the *unknown* natives that imply engine
features not yet costed.

**Detection signals.**
- M2 manifest classification is the census: `jassgen` requires every function to carry an
  `engineFeature` tag where it implies renderer/platform capability (fog, minimap, weather,
  terrain deformation, ubersplats, cinematics, camera shake…). The audit report's
  feature-tag rollup is the complete exposure list — produced **before** sim/render
  implementation locks its design.
- During M5 implementation, any native whose canonical implementation stalls on a missing
  engine capability is logged against its manifest entry; a growing "blocked" count is the
  trend signal.

**Trigger points.**
- *Scheduling trigger:* M2 audit rollup shows engine-feature work exceeding the M4 plan →
  re-plan M4 scope immediately (extend M4 or move features to an explicit M5 sub-phase)
  rather than discovering it mid-M5.
- *Tombstone trigger:* a native's required engine feature is judged out of v1 scope → it is
  explicitly tombstoned (`deferred-v2`) in the manifest with sign-off — the G1 "no silent
  drop" rule applies; capability deferral is a recorded decision, never an implementation
  accident.

### R5 — WC3 IP proximity

*Likelihood: Low · Impact: High*

**Mitigation (per PRD).** Only API *shape* ported; all implementations, data, and assets
original/CC0; no Blizzard formats or content (see NG1/NG2 in
[Goals and Non-Goals](./goals-and-non-goals.md#2-non-goals-v1--expanded)).

**Detection signals.**
- CI license/provenance scans (G4.1–G4.3): any asset without provenance, any non-GLB model,
  any disallowed license in the dependency graph fails the build.
- PR review checklist item for anything touching `data/` or `assets/`: provenance recorded,
  no Blizzard-derived numbers or text.
- Periodic repo audit (each milestone close) grepping for Blizzard trademark strings in
  shipped artifacts and user-facing docs.

**Trigger points.**
- *Hygiene trigger:* any scan hit → remove/replace before merge; no exceptions process exists.
- *Naming trigger:* if the Q2 decision (below) ever revisits JASS-name aliases, legal review
  of the alias table precedes shipping it.
- *External trigger:* any contact or claim from rights holders → freeze releases, counsel
  review; the mitigation posture (shape-only port, original everything) is designed so this
  conversation is survivable, not so it never happens.

### R6 — G3N project staleness

*Likelihood: Medium · Impact: Medium*

**Mitigation (per PRD).** Vendored in-repo (`repoes/engine`); we own maintenance.

**Detection signals.**
- Quarterly upstream check: commit activity, open-issue triage latency, and whether our
  patches (loader fixes, instancing) have an upstream path or are diverging permanently.
- Build health on new Go releases and new OS versions (CI runs a "next Go version" job):
  vendored-fork breakage on toolchain updates is the practical cost of staleness.
- Count of local patches carried in the fork; a steadily growing patch stack measures how
  much engine maintenance we have absorbed.

**Trigger points.**
- *Absorption trigger:* upstream unresponsive to a patch we need for > 1 milestone → stop
  waiting; treat the fork as the permanent home of that change and document it in the fork's
  patch log.
- *Strategic trigger:* if by **M6** the fork carries major subsystem rewrites (renderer
  internals, not just loader fixes), schedule a v2 evaluation of the abstraction boundary —
  R-API-6 (zero G3N types in the public API) exists precisely so this evaluation is possible
  without breaking users.

---

## 2. Open Questions

### Q1 — Fixed-point vs ordered-float for sim math

**STATUS: DECIDED 2026-06-11 — fixed-point `int64` 32.32. See [decisions.md](./decisions.md#d-2026-06-11-1-q1--sim-math-fixed-point-int64-3232).**

**Deadline milestone:** **M1** (the milestone exists to answer this; it blocks all of M3).

**Decision criteria.**
1. *Determinism (gate, non-negotiable):* bit-identical 10k-tick state hashes across the full
   OS/arch CI matrix. Ordered-float is eliminated by a single divergence (see R3).
2. *Performance:* candidate must keep the worst-case tick ≤ 10 ms budget on the reference
   CPU; measure both representations in the spike benchmark.
3. *Ergonomics:* cost of the deterministic math package boundary in gameplay-code
   readability (fixed-point requires a `Fixed` type discipline; ordered-float requires
   reduction-order discipline plus lint).
4. *Range/precision:* map coordinates, DPS accumulators, and long-match timers must fit
   without overflow — evaluate 16.16 vs 32.32 if fixed-point wins.

**Recommended default:** **fixed-point (`int64` 32.32)**. Ordered-float must *prove*
cross-platform identity to be chosen; fixed-point is deterministic by construction, and the
generous 32.32 format sidesteps most precision-tuning. Adopt it unless the spike shows
ordered-float is both provably stable on all targets and meaningfully faster.

### Q2 — JASS-flavored aliases vs purely idiomatic Go naming

**STATUS: DECIDED 2026-06-11 — idiomatic only. See [decisions.md](./decisions.md#d-2026-06-11-2-q2--naming-idiomatic-go-only-no-jass-aliases).**

**Deadline milestone:** **M2** (the public API spec is the sign-off artifact; naming is part
of it).

**Decision criteria.**
1. *G2 minimality:* aliases roughly double the symbol count modders see — directly against
   the smallest-API goal and the G2.1 type/symbol ceilings.
2. *Migration cost evidence:* does a JASS→Go lookup table actually suffice? Test with a
   sample port of a small published JASS map's logic during M2; if the table-driven port is
   painless, aliases buy nothing.
3. *IP posture:* a shipped alias layer that mirrors Blizzard naming wholesale weakens the
   "shape only, idiomatic re-expression" position (R5).
4. *Maintenance:* aliases must be generated from the manifest or they will drift.

**Recommended default:** **idiomatic only** (the PRD draft position), with the generated
JASS→Go mapping table shipped in the docs as the migration aid. Revisit only if M2's sample
port shows the table is insufficient in practice.

### Q3 — Terrain: heightmap mesh vs hex/square tile meshes

**STATUS: DECIDED 2026-06-11 — tile meshes for v1. See [decisions.md](./decisions.md#d-2026-06-11-3-q3--terrain-tile-meshes-kaykit-style-for-v1).**

**Deadline milestone:** **M4 design phase** (terrain rendering and the pathfinding grid's
visual counterpart must be settled before M4 implementation starts; pathfinding in M3 stays
on the abstract WC3-style grid either way, R-SIM-5).

**Decision criteria.**
1. *Asset availability (zero-cost rule, G4):* tiles are abundant and ready
   (KayKit Hexagon/Builder packs); heightmap terrain needs custom texturing/cliff work — who
   pays that cost in art time?
2. *Performance:* static chunk merging cost and draw-call impact of each approach in the M4
   benchmark scene; tiles merge trivially, heightmap chunks need LOD-less low-poly tuning.
3. *WC3 fidelity:* cliffs, ramps, and walkability semantics — can the tile approach express
   WC3-style cliff levels acceptably (KayKit hexagon cliffs suggest yes, with a different look)?
4. *Sim coupling:* whichever is chosen must not leak into `litd/sim` — the sim sees the grid
   and walkability, never the mesh.

**Recommended default:** **tile meshes (KayKit style)** for v1. Asset availability and the
zero-cost goal dominate; the locked camera makes the tile aesthetic read well; heightmap
terrain can be a v2 renderer feature behind the same sim-side grid abstraction without
breaking the API.

### Q4 — `commonai.d.ts` AI natives: port in v1 or defer?

**STATUS: DECIDED 2026-06-11 — defer to v2. See [decisions.md](./decisions.md#d-2026-06-11-4-q4--commonai-natives-defer-to-v2).**

**Deadline milestone:** **M2** (the manifest must classify every `commonai` symbol either
way; the classification *is* the decision record).

**Decision criteria.**
1. *Execution-model cost:* JASS AI runs in isolated script contexts with no shared globals
   and command-stack messaging — R-EXEC-3 confirms porting it means building a second
   sandboxed scheduler domain, not just more functions.
2. *v1 product need:* the M6 vertical slice is a playable skirmish (build, train, fight,
   win/lose); a scripted/simple opponent can be written against the ordinary public API
   without the `commonai` domain.
3. *G1 accounting:* deferral must be loss-free on paper — every `commonai` native still gets
   a manifest entry, tombstoned `deferred-v2`, so the audit stays complete.
4. *API stability:* would deferring force breaking changes later? The isolated-domain design
   (own scheduler, message-queue boundary) means v2 AI bolts on beside the v1 API rather
   than reshaping it.

**Recommended default:** **defer to v2** (the PRD draft position). Classify and tombstone
all `commonai` natives in the M2 manifest with reason `deferred-v2`; satisfy M6's opponent
need with a Go-scripted skirmish AI built on the public API. Criterion 4's bolt-on property
makes deferral cheap to reverse.

### Q5 — Networked multiplayer (added 2026-06-11)

**STATUS: DECIDED 2026-06-11 — committed, deterministic lockstep, milestone M7. See [decisions.md](./decisions.md#d-2026-06-11-5-new--q5--multiplayer-committed-lockstep-milestone-m7).**

Feasibility: yes — lockstep is the WC3 model and the architecture is already shaped for it
(deterministic sim, command-stream input, state hashing). M7 adds transport, lobby,
input-delay/stall handling, and hash-based desync detection. G5.3 replay verification is the
M6 exit gate proving lockstep readiness.

---

## 3. Review cadence

Risks and open questions are reviewed at every milestone close
([Milestones](../09-roadmap/milestones.md)): detection signals checked, trigger points
evaluated, and any question whose deadline milestone is closing gets its decision recorded
in this directory as a dated decision note. New risks discovered mid-milestone are added
here first, then folded into the PRD at its next revision.
