# DEC-957: World Model = Hybrid, Persistent-First — Persistent Authoritative Online World is PRIMARY; Lockstep Match Netplay is a Bounded Later Mode

- **Status:** DECIDED (recorded on #957, 2026-07-11; this document is the durable record)
- **Type:** decision (doctrine #800 §5 — research-first)
- **Decides:** the topology and build order for online play — persistent authoritative
  online world (always-on ORPG/MUD) **vs** match-based lockstep netplay (#85)
- **Refines:** #1318 (CLOSED doctrine: server-authoritative for ALL multiplayer). #1318
  pinned *who owns truth*; DEC-957 pins *which world model is primary and built first*,
  and demotes lockstep from foundation to bounded later mode.
- **Consistent with:** #995 (core-design doctrine: multiplayer/PvP/full-loot/RP-enforced,
  day one — not a Phase 2 bolt-on)
- **Shapes:** #943, #952, #954, and all of Phase 10 — #1037, #1155, #1471, #1472
  (confirmed open and confirmed dependent; see §5)

## 1. Context

Parent #945 benchmarks Carrion Fields: a **persistent shared world** — one living world,
250+ areas / 23k rooms, permadeath, full-loot PvP, a scarcity economy, and mobs/items that
repop on timers whether or not any given player is logged in. The engine's base netplay
(#85) is **match-based lockstep**: N peers, a fixed session, deterministic simulation of
identical command streams, session ends, state discarded.

These are not two flavours of the same thing. They are different answers to the question
"where does world state live, and for how long?" — and the answer dictates the shape of
persistence, authority, anti-cheat, backup, and hosting for everything built on top.

### What the design phases already presuppose

Three open epics were authored against the persistent model and are unbuildable without it:

| Epic | Presupposition it bakes in |
|---|---|
| **#945** (MUD-grade systemic depth) | zone resets/repop on wall-clock timers, permadeath, PK, persistent world — state must advance with no player present |
| **#972** (scarcity economy & full-loot PvP) | a **global** finite-quantity ledger (#973: legendary = 1-2 in the ENTIRE world) — a world-wide invariant across all sessions |
| **#984** (Carrion Fields full content build) | a single living world that content is *authored into* and that outlives any session |

Lockstep can express none of these. A lockstep match has no state before it starts and
none after it ends; there is no "the world" for a global scarcity cap to be global *over*,
no wall-clock for a repop timer to advance against, and no durable character for
permadeath to be permanent *to*. Building lockstep-first and retrofitting persistence
would strand #945 / #972 / #984 behind a rewrite of the authority and persistence layer.

## 2. Options Matrix

| Criterion | A. Lockstep-first (persistence retrofitted later) | B. Persistent-first, lockstep as a bounded later mode | C. Persistent-only (drop lockstep entirely) |
|---|---|---|---|
| Expresses always-on economy (#972/#973 global scarcity ledger) | NO — no cross-session world for a global cap to hold over; ledger would be per-match, i.e. meaningless | YES — server owns the ledger; caps are world invariants audited live and against snapshots (#1471/#1472) | YES |
| Expresses permadeath / durable characters (#952, #943) | NO — a match has no durable character; "permanent" death lasts until the session ends | YES — characters are server-owned rows with a lifecycle independent of any connection | YES |
| Expresses living world (#954 resets/repop, #984) | NO — no wall-clock tick without a session | YES — the server ticks the world whether or not anyone is watching | YES |
| Cheat/authority posture | WEAK for an open PvP economy — lockstep peers each hold full state; a dupe or a hacked client is a *world*-destroying event, not a match-losing one | STRONG — server owns state, clients send inputs; matches #1318 doctrine exactly | STRONG |
| Cost of the *other* mode later | HIGH — persistence, authority, and anti-cheat are foundation layers, not features; retrofitting them rewrites the sim boundary | LOW — the deterministic sim core, command-stream encoding, and state hashing that lockstep needs are the SAME primitives the authoritative server needs for replay/divergence detection (#907/#1472); a skirmish mode is an additional session type over an existing sim, not a new foundation | N/A (lockstep never built) |
| Optional skirmish / RTS-match sessions (WC3 parity) | native | YES, later — as **server-validated lockstep** (SC2/AoE4 model): lockstep as an internal bandwidth technique under server state-hash validation, per #1318 | NO — loses a genuine WC3-parity mode with a cheap incremental cost |
| Determinism work is wasted? | n/a | NO — determinism (#1463 boundary, WASM fuel metering per DEC-1474, replay hashes) is required by the persistent server for replay, forensics (#907) and divergence response (#1472) regardless | Determinism still required, same reason |

### Why not each runner-up

- **Lockstep-first (A):** it is the cheaper *first* milestone and it is the wrong one.
  Persistence, server authority, and economy integrity are foundation, not features:
  every phase after #945 assumes them, and #995 already declares multiplayer/PvP/full-loot
  core and day-one, not Phase 2. Choosing A means the Carrion Fields phases wait on a
  later rewrite of the layer they all sit on.
- **Persistent-only (C):** honest and simple, but it discards a real WC3-parity mode
  (bounded skirmish matches) whose marginal cost is genuinely low, *because* the
  deterministic sim, command encoding, and state hashing it needs are already paid for by
  the persistent server's replay/anti-cheat requirements. Deleting lockstep buys nothing
  that persistent-first does not already buy.

## 3. Decision

**Hybrid, persistent-first.** (Recorded by operator on #957, 2026-07-11.)

1. **The persistent authoritative online world is the PRIMARY and FIRST-BUILT mode.**
   A single living world per server: server-authoritative state, durable characters,
   permadeath, full-loot, a scarcity economy, and world time that advances across
   sessions. Topology is **self-hosted player-run servers** (#1037): anyone hosts a
   200-500-player world on their own machine; others connect via localhost / LAN / SSH
   tunnel / direct IP / invite; the admin moderates and sets per-server RP policy.

2. **Lockstep match netplay is a BOUNDED LATER MODE, explicitly NOT the foundation.**
   It survives as an *optional skirmish session type* (#85, M7) and — per #1318 — as an
   internal RTS-match bandwidth technique under **server state-hash validation**
   (SC2/AoE4 model), never as the authority model. Nothing in the persistent world may
   be made to depend on lockstep semantics.

3. **Authority is not negotiable per mode.** In both modes the server owns state and
   clients send inputs (#1318). Lockstep is a transport/compute optimisation inside a
   server-validated session, never a peer-authority arrangement.

4. **Build order is binding.** Persistent-world work (persistence, authority, world tick,
   economy ledger, backup, anti-cheat) precedes skirmish/lockstep polish. An issue may
   not be scheduled ahead of the persistent-world layer it depends on merely because
   lockstep would land it sooner.

### What this means concretely

- Durable state lives on the server, not in client save files. Client-side save files or
  "load codes" are **not** an authority mechanism for online characters (see #943, §5).
- The world ticks on wall-clock time on the server, with no player required (#954).
- The global scarcity ledger is a server-side world invariant, continuously auditable
  (#973 → #1471 integrity audit, #1472 dupe detection).
- Determinism work is preserved and reused, not discarded: fixed-point sim (#1463),
  deterministic scripting (DEC-1474 / docs2/decisions/DEC-1474-script-escape-hatch.md),
  and replay/state hashing serve the persistent server's replay-forensics (#907) and
  hash-divergence response (#1472) paths, and are exactly what a later server-validated
  lockstep skirmish mode needs.

## 4. Shape of the Two Modes

```
                          deterministic sim core (fixed-point #1463, WASM DEC-1474)
                          command encoding + state hashing (replay, #907)
                                        |
              +-------------------------+--------------------------+
              |                                                    |
   PRIMARY / FIRST-BUILT                              BOUNDED LATER MODE
   Persistent authoritative world                     Skirmish / RTS match
   (always-on ORPG/MUD, #1037)                        (#85, M7)
              |                                                    |
   server owns durable state:                         server-validated lockstep:
     - characters (#943)                                - peers exchange commands
     - permadeath/corpses (#952)                        - server validates state hashes
     - zones, resets, repop (#954)                      - divergence => flagged (#1472)
     - global scarcity ledger (#973)                    - session state is DISCARDED
     - world clock advances w/o players                   at match end
              |                                                    |
   clients send INPUTS ONLY (#1318)                   clients send INPUTS ONLY (#1318)
              |
   backup/snapshot/restore + integrity audit (#1471)
   anti-cheat / dupe detection / enforcement (#1472)
              |
   published, discovered, hosted, joined via the ecosystem hub (#1155)
```

The horizontal bar matters: **one deterministic sim core serves both.** That is why the
hybrid is cheap. What is *not* shared — persistence, authority, world clock, economy
ledger, backup, enforcement — is exactly the set of things lockstep cannot provide, and
exactly the set this decision puts first.

## 5. Confirmed Downstream Impact

Every issue below was read with `gh issue view` on 2026-07-11 and confirmed **OPEN** and
**genuinely dependent** on this decision.

| Issue | State | How DEC-957 shapes it |
|---|---|---|
| **#943** persistent ORPG character save/progression | OPEN (M16) | Directly shaped. The issue offers "save files and/or load-code (hashtable-equivalent)". Under DEC-957 the **server** is the authority for online characters — durable server-side rows, not client-held save files or load-codes, which are tamperable and cannot carry permadeath or a global ledger. Client-side codes may remain for single-player/offline only. |
| **#952** permadeath, corpse-looting, aging/lifespan | OPEN (M16) | Directly shaped. Permadeath is only meaningful against durable server-owned characters; a lockstep match cannot make death permanent. Corpse/full-loot drops must be settled server-side against the scarcity ledger. |
| **#954** zone/area world model + resets/repop | OPEN (M16) | Directly shaped. Timed mob/item resets require a server world clock that advances with no player present — a persistent-mode-only capability. Instancing decisions hang off the persistent topology. |
| **#1037** [EPIC] Self-hosted servers | OPEN (M7) | **Hardest dependency — its body literally states it "answers topology decision #957".** DEC-957 is its charter: player-run 200-500p persistent worlds, authoritative sim, admin moderation, per-server RP policy. Unblocked by this decision. |
| **#1155** [EPIC] Game ecosystem hub | OPEN (M9) | Shaped, primarily *through* #1037 (its body: "Builds on self-hosted servers (#1037)"). What the hub publishes/discovers/joins is a **persistent world instance**, not a match lobby — so its discovery, hosting and join flows are specified by DEC-957. Dependency is real but transitive; it is not blocked on DEC-957 directly. |
| **#1471** [P1][SERVER] backup/snapshot/restore + scarcity-ledger integrity audit | OPEN | Directly shaped — the issue **exists only because** state is persistent. Crash-consistent snapshots of a live authoritative world, restore divergence semantics, and ledger invariant audits are all persistent-mode concepts. Lockstep-first would leave this untriggered until a rewrite. |
| **#1472** [P1][NET][ECON] anti-cheat & economy-integrity enforcement | OPEN | Directly shaped, and it spans both modes: dupe detection against the **global** scarcity ledger and server-side input hardening are persistent-world enforcement, while its "lockstep-hash divergence response" item is exactly the **server-validated** lockstep posture DEC-957 §3.2/#1318 mandate. |

Also confirmed related and consistent, no change required:

- **#85** (netplay FSV, 2-machine LAN match, identical hashes) — OPEN. Remains valid as
  the *bounded later mode's* gate; it is no longer the foundation milestone.
- **#1318** (server-authoritative for ALL multiplayer) — CLOSED doctrine, refined not
  contradicted: authority model unchanged, world model and build order now pinned.
- **#995** (core-design doctrine, day-one multiplayer) — OPEN, consistent.
- **#1044** (200-500 player server performance) — OPEN, the persistent server's perf
  target; presupposes this decision.

## 6. Consequences / Non-Goals

- **Non-goal:** peer-authoritative play in any mode. Rejected by #1318 and re-affirmed here.
- **Non-goal:** "make lockstep persistent." Session state in a lockstep match is discarded
  at match end by construction; persistence is the server's job, not lockstep's.
- **Consequence:** M7 ("Multiplayer lockstep") is a *misleading milestone name* for its
  actual persistent-world content (#1037 sits in it). The name is historical; the content
  is governed by this decision, not by the label.
- **Consequence:** any future issue that proposes client-authoritative durable state, or
  that makes a Carrion Fields system (#945/#972/#984 lineage) depend on lockstep session
  semantics, contradicts binding doctrine and must be re-scoped or rejected loud.

## 7. Sources

- Repo doctrine: #800 (research-first / FSV resolution gate), #995 (core game design),
  #1318 (server-authoritative for ALL multiplayer, CLOSED).
- Prior decision-doc convention: docs2/decisions/DEC-1474-script-escape-hatch.md,
  docs2/decisions/DEC-1526-repo-size-runbook.md.
- Operator statements on #957 (2026-06/2026-07): multiplayer is core/day-one; topology is
  self-hosted player-run servers (#1037); persistent authoritative online-world is the
  model, lockstep survives only as server-validated RTS-match technique.
- Carrion Fields (the benchmark, #945): persistent shared world, 250+ areas / 23k rooms,
  permadeath, full-loot PK, timed resets — https://www.carrionfields.net/
- Server-validated lockstep precedent (RTS matches under an authoritative validator):
  StarCraft II / Age of Empires IV network models, as cited in #1318.
