# DEC-1552 — `max_move_units_per_tick` is duplicated on purpose (defense in depth)

Status: ACCEPTED (2026-07-12)
Refs: #1552, #1548, #1551, #726, docs2/BUILDING_ON_CALYX.md §7.6.1

## Context

An agent model response is gated by two independent numeric bounds on movement, both named
`max_move_units_per_tick`:

- `EngineCalyxWardPolicy.max_move_units_per_tick` (GATE 5, `calyx_ward_gate.rs`) — checked on the
  Ward **subject**: the movement claimed by the response's free text/fields, *before* lowering.
- `EngineAgentOrderEnvelope.max_move_units_per_tick` (GATE 6, `agent_response_gate_types.rs`) —
  checked on the **lowered order kind**: the actual `SimOrderKind` the proposal maps to, *after*
  Ward, immediately before the order is emitted.

#1548 noticed the redundancy: because the Ward bound fires first, the over-envelope FSV case reports
`stage=Ward` rather than `stage=Envelope`. The question raised was whether this duplication should
stay, or whether one site should be removed as dead weight.

## Decision

**Keep both. This is defense in depth, and it is now a decision rather than an accident.**

1. The two bounds guard **different inputs at different stages**: Ward measures the movement a
   subject *claims*; the envelope measures the movement an order *actually carries after lowering*.
   A bug (or future change) in the subject-construction path could make the two disagree; having the
   envelope re-check the lowered kind closes that gap.
2. The **order envelope (GATE 6) is authoritative**. It is the last deterministic bound before an
   order becomes authoritative game state, and it is the one the structural-defense doctrine
   (§7.6.1) leans on. If Ward were ever relaxed, reordered, or deleted, the envelope must still
   refuse the identical order on its own.
3. Ward's bound is a **cheap early-out and a diagnostic** (it makes the Ward slot table show the
   refusal), not a substitute for the envelope.

## Consequences / invariants

- The two values **must be kept equal** for a given deployment. The envelope must **never** be
  loosened to match a relaxed Ward policy — only the reverse is ever safe.
- Neither site may be removed without re-homing its bound onto the other stage.
- A comment at **both** definition sites points here so the redundancy is not later "cleaned up".

## Alternatives considered

- *Collapse to one bound (envelope only).* Rejected: loses the pre-lowering early-out and the Ward
  diagnostic, and couples the Ward subject path to trusting downstream lowering.
- *Collapse to one bound (Ward only).* Rejected outright: Ward is a novelty detector and is not
  load-bearing (§7.6.1); the deterministic envelope must be the authoritative bound.
