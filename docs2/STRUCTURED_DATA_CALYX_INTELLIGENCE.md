# STRUCTURED_DATA_CALYX_INTELLIGENCE — Recovery Index

> **THIS IS NOT THE ORIGINAL DOCUMENT.** The original
> `docs2/STRUCTURED_DATA_CALYX_INTELLIGENCE.md` — the "authoritative design +
> doctrine + roadmap" behind the structured-data program — was a working draft
> that was **never committed to git** (`git log --all -- <path>` is empty on
> every branch and remote) and was subsequently lost from the only disk that
> held it. Recovery was attempted and exhausted (working tree, reflog, stash,
> `git fsck --dangling`, all worktrees, machine-wide filesystem search); see
> issue #1465 for the full forensic record. Its content has NOT been
> fabricated or reconstructed from memory here — this file is an honest index
> to where the surviving knowledge actually lives.

## Canonical surviving sources

The doctrine and context that the lost spec defined survive, verbatim, in two
GitHub issues (both written from the original document while it still existed):

| Source | Role | Status |
|---|---|---|
| **#1348** | DOCTRINE: structured data is association-native — ingest ALL engine structured data into Calyx (single source of truth), capture every association, extract maximum intelligence, lower to the hot path | OPEN (standing doctrine) |
| **#1349** | CONTEXT: the program's background, constraints, and vocabulary | companion to #1348 |
| `docs2/BUILDING_ON_CALYX.md` | Tracked, binding handbook for all Calyx work (atoms -> associations -> bits -> kernel -> compose; embed latent / encode explicit; lower to hot path). Domain-neutral doctrine; does NOT contain the atomic-issue roadmap | tracked on origin/main |

Related note: #1348/#1349 also referenced `CALYX_UNIVERSAL_GUIDE.md` (under
`docs2/`), which is likewise lost (never committed). The file the issues call
`CALYX_COMPLETE_DOCUMENTATION.md` survives as
`Calyx/docs2/calyxdocs/CALYX_COMPLETE_DOCUMENTATION.md` (the bare `docs2/`
prefix in the issue text is imprecise).

## The executed roadmap: atomic issues #1351-#1422

The lost spec mapped the program to 72 atomic issues. Those issues exist,
carry their own full specifications, and are the real record of what "done"
meant. All 72 are CLOSED (verified via `gh issue list`, 2026-07-11). The
epic was #1350 (CLOSED).

Track key: A=ingest foundation, L=store migration, B=lenses, C=panels,
D=associations, E=information measures, F=kernel, G=guards, H=oracle/self-
optimization, I=semantic surfaces, J=lowering/determinism, K=integration/
acceptance.

| # | State | Title |
|---|---|---|
| 1351 | CLOSED | [CALYX][A] Canonical deterministic byte-encoding for structured records -> content-addressed Input(Modality::Structured) |
| 1352 | CLOSED | [CALYX][A] Structured schema catalog — enumerate every engine structured entity+field with field-type tags |
| 1353 | CLOSED | [CALYX][A] Embed-vs-encode decision engine — classify every field latent(embed) vs explicit(encode) -> auto panel construction |
| 1354 | CLOSED | [CALYX][A] Structured ingest pipeline — rows -> constellations via a structured panel (idempotent, Ledger-provenanced) |
| 1355 | CLOSED | [CALYX][A] Bulk/streaming world ingest — whole project/world into Aster |
| 1356 | CLOSED | [CALYX][A] Incremental re-ingest / hot-reload on edit — editor writes update constellations (panel-versioned backfill) |
| 1357 | CLOSED | [CALYX][L] Migrate editor shell-state + preferences + history/undo off JSON into Calyx (Aster CF + MVCC) |
| 1358 | CLOSED | [CALYX][L] Migrate object/gameplay data (units/items/abilities/upgrades/constants) off local stores into Calyx |
| 1359 | CLOSED | [CALYX][L] Migrate terrain/world/nav/foliage stores off RON/JSON into Calyx |
| 1360 | CLOSED | [CALYX][L] Migrate triggers/story/quest/dialogue/VFX/sound defs off local stores into Calyx |
| 1361 | CLOSED | [CALYX][L] Migrate AI data (behavior trees/perception/controllers/skills) off local stores into Calyx |
| 1362 | CLOSED | [CALYX][L] Migrate project/social/multiplayer/moderation/content-trust stores off local stores into Calyx |
| 1363 | CLOSED | [CALYX][L] Side-store eradication FSV — grep-gate proving zero structured writes outside Calyx |
| 1364 | CLOSED | [CALYX][B] ScalarFieldLens + normalization variants (raw/min-max/z-score/log/rank-quantile) |
| 1365 | CLOSED | [CALYX][B] CategoricalOneHotLens — enum/category -> OneHot Dense(k) |
| 1366 | CLOSED | [CALYX][B] OrdinalLens — ordered category -> normalized scalar position |
| 1367 | CLOSED | [CALYX][B] BooleanLens — bool field -> Dense(1) |
| 1368 | CLOSED | [CALYX][B] RecordVectorLens — full numeric record -> normalized Dense(n) tabular feature vector |
| 1369 | CLOSED | [CALYX][B] HashedFeatureLens — high-cardinality id/category -> hashing-trick signed Dense(d) |
| 1370 | CLOSED | [CALYX][B] Text-label dual-encoding — embed descriptions/prose on structured records AS AN EXTRA slot (keep exact encoders) |
| 1371 | CLOSED | [CALYX][B] ReferenceGraphLens — foreign-key/relation fields -> graph structural signature |
| 1372 | CLOSED | [CALYX][B] SetBagLens — inventory/loot/tags -> sparse multi-hot slot |
| 1373 | CLOSED | [CALYX][B] Temporal lens wiring (E2/E3/E4) on event/spawn/quest timestamps |
| 1374 | CLOSED | [CALYX][B] DerivedFeatureLens — engineered cross-field features as explicit frozen lenses |
| 1375 | CLOSED | [CALYX][B] CommissionedEntityEmbeddingLens — learned entity embedding of high-card categoricals from the project corpus |
| 1376 | CLOSED | [CALYX][B] Lens-count maximizer/enumerator — max diverse lens set + DDA yield report |
| 1377 | CLOSED | [CALYX][C] game-unit panel template (>=10 diverse lenses + 3 temporal) |
| 1378 | CLOSED | [CALYX][C] game-item panel template |
| 1379 | CLOSED | [CALYX][C] game-ability panel template |
| 1380 | CLOSED | [CALYX][C] game-terrain/tile/doodad panel template |
| 1381 | CLOSED | [CALYX][C] game-event/ECA-trigger panel template |
| 1382 | CLOSED | [CALYX][C] game-quest/faction/dialogue panel template |
| 1383 | CLOSED | [CALYX][C] game-economy/loot panel template |
| 1384 | CLOSED | [CALYX][C] Panel capability-gate wiring (apply_capability_gate admit/park/retire) on every game panel |
| 1385 | CLOSED | [CALYX][D] Weave within-record cross-terms (agreement/delta/interaction/concat) across all structured slots + abundance report |
| 1386 | CLOSED | [CALYX][D] Agreement graph over structured fields — which stats/fields co-vary |
| 1387 | CLOSED | [CALYX][D] Inter-entity association graph — between-record kNN AssocGraph (unit-item-ability-quest-faction) |
| 1388 | CLOSED | [CALYX][D] Temporal cross-terms / lead-lag between game event series |
| 1389 | CLOSED | [CALYX][D] Blind-spot detection across structured lenses |
| 1390 | CLOSED | [CALYX][D] Reactive triggers on structured ingest (NewRegion/EventRecurs/DriftDetected) -> editor notifications |
| 1391 | CLOSED | [CALYX][E] Grounded anchors for game data — define AnchorKinds + anchoring pipeline |
| 1392 | CLOSED | [CALYX][E] Bits-per-lens (KSG MI) per outcome; park <0.05, drop >0.6 corr |
| 1393 | CLOSED | [CALYX][E] Panel sufficiency per domain I(panel;anchor)>=H(anchor) + deficit routing -> propose_lens |
| 1394 | CLOSED | [CALYX][E] Total correlation & n_eff — effective non-redundant lens count; redundancy pruning |
| 1395 | CLOSED | [CALYX][E] Transfer entropy — directional temporal causality among game event series |
| 1396 | CLOSED | [CALYX][E] Interaction information — synergy/redundancy among stat/field triples |
| 1397 | CLOSED | [CALYX][E] Periodicity + recurrence hazard — periodic events + overdue-event hazards |
| 1398 | CLOSED | [CALYX][E] Bayesian posteriors — small-sample balance/consistency (Gamma-Poisson / Beta-Bernoulli) |
| 1399 | CLOSED | [CALYX][F] Grounding kernel over the game corpus (~1% MFVS, kernel-as-index) |
| 1400 | CLOSED | [CALYX][F] kernel_answer / define over structured world data — grounded design Q&A |
| 1401 | CLOSED | [CALYX][F] Grounding-gap report — which entities/zones lack anchors |
| 1402 | CLOSED | [CALYX][F] Multi-scope kernels — per-zone / per-faction / per-project |
| 1403 | CLOSED | [CALYX][G] Fail-closed OOD guard on AI/agent-authored structured content (per-slot Gtau) |
| 1404 | CLOSED | [CALYX][G] Per-slot conformal tau calibration for structured panels |
| 1405 | CLOSED | [CALYX][G] Identity-lock canonical entities (anti stat/lore drift) |
| 1406 | CLOSED | [CALYX][H] Oracle consequence prediction (butterfly) on mechanics — change stat X -> downstream effects |
| 1407 | CLOSED | [CALYX][H] reverse_query — abductive balance debugging (what causes this outcome) |
| 1408 | CLOSED | [CALYX][H] complete() — impute missing structured fields from trusted region (editor autofill) |
| 1409 | CLOSED | [CALYX][H] Time-of-next-occurrence prediction for world/economy events |
| 1410 | CLOSED | [CALYX][H] propose_lens autonomous loop — deficit-driven auto-proposal of new structured lenses |
| 1411 | CLOSED | [CALYX][H] Anneal self-optimization of structured panels + intelligence objective J / growth-curve / intelligence_report |
| 1412 | CLOSED | [CALYX][H] super_intelligence 6-tier readiness predicate per game domain |
| 1413 | CLOSED | [CALYX][I] Multi-lens semantic search over structured entities (find-similar units/items/abilities) |
| 1414 | CLOSED | [CALYX][I] Semantic authoring ops (neighbors/agree/disagree/define) surfaced in editor |
| 1415 | CLOSED | [CALYX][I] Structured dedup on import (TCT cosine) |
| 1416 | CLOSED | [CALYX][J] Structured-intelligence lowering valve — freeze kernels/bits/associations/imputed values/predictions into fingerprinted deterministic data |
| 1417 | CLOSED | [CALYX][J] Provenance + reproduce for every lowered structured value (Ledger) |
| 1418 | CLOSED | [CALYX][J] Determinism-boundary FSV — prove the hot tick/render never call Calyx; consume only lowered data |
| 1419 | CLOSED | [CALYX][K] MCP tool surface for structured intelligence (ingest/measure/associate/bits/kernel/oracle/guard) |
| 1420 | CLOSED | [CALYX][K] Editor intelligence surfaces (abundance/bits/associations/kernel/what-if panels over structured data) |
| 1421 | CLOSED | [CALYX][K] Capability-coverage audit — every Calyx subsystem + all 44 MCP tools -> a live structured-data use, fail-closed if idle |
| 1422 | CLOSED | [CALYX][K] End-to-end acceptance — ingest real game data -> associations -> bits -> kernel -> oracle -> lowered deterministic data, FSV-in-reality |

## Root-cause and prevention

The loss happened because a binding document existed only on one disk and was
never committed — an on-disk existence check would have passed while the file
was one failure from gone. The structural fix is the doc-reference gate
`scripts/check-doc-refs.ps1` (wired into `scripts/check-format.ps1`), which
fails any commit where a git-tracked markdown file references a
`docs2/`, `docs/`, or `prompts/` path that is not itself **git-tracked**
(tracked-ness, not existence, is the invariant). See #1465.
