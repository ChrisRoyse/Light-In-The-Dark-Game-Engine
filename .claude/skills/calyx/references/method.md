# Calyx method -- depth reference

Distilled from `docs2/BUILDING_ON_CALYX.md`. That handbook is authoritative; this is the
working summary. Do not contradict the handbook.

## The foundational principle

Building on Calyx means committing to first principles. Decompose the ENTIRE project into its
atomic, irreducible, independently-measurable units, then compute ALL associations among them
from the ground up -- every base relationship, not a sampled subset. Records are inert; the
complete web of associations among the atoms IS the raw material of intelligence. Bits,
sufficiency, the kernel, the guard, and the oracle are all ways of measuring, distilling, and
composing that ground-up association structure. Never shortcut the decomposition; partial
association structure yields partial, ungrounded intelligence.

## The loop (kernel-first)

1. **Decompose** -- reduce to atoms: entities, every field, explicit-vs-latent, groundable
   outcomes. Ask relentlessly "can this be split into a more primitive, independently-measurable
   thing?" Each atom -> a candidate lens.
2. **Associate** -- instrument maximally, then weave EVERY base association: within-record
   cross-terms (agreement/delta/interaction/concat), the between-record kNN graph, and temporal
   lead/lag. Read the agreement graph -- it is the map.
3. **Differentiate** -- bits per lens/association; prune redundancy; find synergy (interaction
   information) and causality (transfer entropy); check sufficiency `I(panel;outcome) >= H(outcome)`.
4. **Distill -> Compose** -- derive the kernel at any scope, prove it explains the corpus, then
   build index / answer / guard / prediction / imputation / forecast / self-optimize FROM it.
5. **Feedback** -- deficits name missing bits -> propose a lens -> re-weave -> re-kernel.

## Embed vs encode (the core decision, handbook section 5)

For any input: do I derive meaning by embedding raw bytes, or by treating the explicit fields
directly?

- **Unstructured (text/image/audio/video/code/protein/dna/molecule):** meaning is LATENT in the
  bytes -> you MUST use a learned embedder lens; association = nearness in embedding space
  (cosine/ANN, fused via RRF).
- **Structured (numbers/categories/booleans/ordinals/timestamps/references):** meaning is already
  EXPLICIT -> you MUST NOT run a learned embedder over it. Encode deterministically and let Calyx
  measure association in BITS (KSG MI, total correlation, transfer entropy). Embedding an
  already-meaningful value destroys the exact auditable value and adds noise -- forbidden.
- **Text labels on structured data (the exception):** when a record/field/category carries prose
  (a description, flavor, docstring, or a generated one-line row summary), embed THAT as an
  ADDITIONAL slot alongside the exact encoders. How much prose to embed depends on the data: a
  bare number -> don't embed; a symbol with rich meaning -> embed its description; a field already
  prose -> embed it.
- **Hybrid records** carry both kinds of slot at once (no-flatten). Fuse per-slot search via RRF;
  measure both structured and embedded slots in bits about the SAME grounded anchor so they live
  on one axis; cross-terms between a structured slot and an embedding slot detect
  "text-vs-numbers" mismatch; the kernel and oracle span all slot kinds.

Deterministic structured encoders (each a frozen lens -> a slot): scalar normalization variants
(raw/min-max/z-score/log/rank-quantile/robust/winsorized), cyclic, binning, one-hot, ordinal,
boolean/multi-hot, feature hashing, count/frequency, target/mean (grounded, held-out), set/bag
sparse, record vector, derived cross-field features, interaction crosses, relational aggregations,
reference->graph structural signature, path/hierarchy, between-record graph, temporal lenses,
recurrence series, lag/delta.

## Key constants (Calyx defaults, tunable)

| Item | Value |
|---|---|
| Association yield per input | `N + C(N,2) + 1`, `C(N,2) = N(N-1)/2` |
| Differentiation contract | signal >= ~0.05 bits; max pairwise corr <= ~0.6; >= ~50 samples |
| Panel sufficiency | `I(panel;anchor) >= H(anchor)` (no slack) |
| Diversity | `n_eff` via total correlation `TC = sum H(slot_k) - H(panel)` |
| Kernel selection | greedy `0.40*degree + 0.40*betweenness + 0.20*groundedness`; ~1% FVS; recall gate ~0.95 |
| Fusion | Reciprocal Rank Fusion, K = 60; BM25 k1~1.2, b~0.75 |
| Guard | per-slot cosine >= conformal tau; all-required or k-of-n; cold-start tau ~0.7 |
| Grounded confidence | `n/(n+1)` (never reaches 1) |

## Lowering (the real-time boundary)

Everything above is authoring/agent/editor-time knowledge. The deterministic sim tick and render
frame MUST NOT block on Calyx. Freeze the needed intelligence (kernels, bits, association graphs,
imputed values, predictions) into fingerprinted, reproducible, Ledger-provenanced deterministic
data via the lowering valve, and consume only that on the hot path. Prove the hot path reads only
lowered data.

## Verify against the bytes

A return value is a claim; the source of truth is the bytes. Read the stored Aster rows / Ledger
entries / lowered artifacts and compare observed == expected. A green test is supporting evidence,
not proof (issue #800 doctrine).
