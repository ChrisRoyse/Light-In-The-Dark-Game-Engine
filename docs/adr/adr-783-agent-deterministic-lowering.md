# ADR 783: AI Agents Lower To Deterministic Server Inputs

## Status

Accepted.

## Context

Issue #783 asked whether real-time AI-agent control can coexist with
deterministic netplay and replay. Doctrine #781 requires agents to think
out-of-band and mutate gameplay only through deterministic lowered orders.
Doctrine #1318 later made all multiplayer server-authoritative: players and
agents submit inputs; the server owns state and validates hashes.

## Decision

AI agents never write directly into `FixedTickSim` or render state.

Agents may prompt, reason, and use Calyx outside the deterministic hot path.
The only gameplay object they may commit is a fingerprinted `SimOrder` whose
source is `SimOrderSourceKind::LoweredAgent`. Human input uses the same order
queue with `SimOrderSourceKind::Human`.

The authoritative server consumes those orders over the same deterministic
fixed-point sim core used for replay:

- `crates/litd-engine/src/sim.rs` owns fixed-point state and FNV-1a hashing.
- `crates/litd-engine/src/sim_order.rs` owns ordered, fingerprinted inputs.
- `crates/litd-engine/src/sim_replay.rs` owns snapshot/record replay checks.
- `crates/litd-engine/src/calyx_agent_runtime.rs` activates agents out-of-band
  and emits lowered-agent orders only.
- `crates/litd-engine/src/net_authority.rs` keeps server state authoritative
  while clients submit inputs only.

Slow or missing agent decisions must not block determinism. They resolve as no
new input for that tick, or as a later fingerprinted order when admitted by the
server/order valve.

## Consequences

- Real-time AI and deterministic replay are compatible.
- Agent creativity is outside the hot path; sim mutation is deterministic.
- Human and AI orders share ordering, fingerprints, replay, pruning, and server
  validation.
- Client or agent attempts to forge position, HP, inventory, or divergent state
  are rejected by the authoritative server boundary.
- Future #965 Immortal/GM work should lower grants, quests, possession, event
  triggers, and RP enforcement to explicit deterministic orders or ledgers.

## Verification

Run:

```powershell
cargo run --locked -p litd-engine --features fsv --example issue783_agent_lowering_boundary_fsv --quiet
```

The run writes `target/fsv/issue783_agent_lowering_boundary/runs/<run>/manual-fsv-report.json`
plus `latest-manifest.json`. Manual FSV must inspect the persisted report,
hashes, replay record, and server-authority proof before #783 is closed.
