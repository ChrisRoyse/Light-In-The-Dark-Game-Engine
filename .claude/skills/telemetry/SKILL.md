---
name: telemetry
description: "Wire and query the offline replay & telemetry analytics loop (Assay/Lodestar/Oracle) in this Rust/wgpu engine. Use when recording a replay, measuring balance from matches, diagnosing a desync, forecasting crash recurrence, adding a telemetry event, or building the balance-tuning loop. Offline/read-only. Keywords: telemetry, replay, Assay, Lodestar, Oracle, balance, win rate, desync, forensics, crash recurrence, #907, #1680."
---

# Wire and query telemetry / replay analytics (Assay / Lodestar / Oracle)

Issue #907 built an OFFLINE, read-only analytics loop over real match replays.
Doctrine (#907, #777 sec 6): nothing here runs inside `World.Step()` or the render
frame. Every replay is re-executed through the deterministic `SimReplayRecord` path
(hash-verified) BEFORE anything is stored; outcomes are derived from the actual
executed attack events, never from caller claims. Learned recommendations re-enter
the game only via the lowering valve as fingerprinted TOML -- analytics never write
live game state.

All code lives in `crates/litd-engine/src/calyx_replay_analytics*.rs`, persisted in
Calyx-backed Aster vaults (`EngineCalyxDatabase`, `calyx_persistence`). The end-to-end
harness is `crates/litd-engine/examples/issue907_replay_analytics_fsv.rs` -- read it
first; it exercises ingest + all three passes + reopen-readback.

## 0. The three subsystems (what each does)

- **Assay** = grounded/measured BALANCE analytics. Measures, in BITS, which
  unit/ability stats actually predict win/loss across a replay batch (anchor
  `match_outcome_win_loss`) -- measured balance, not intuition.
  `calyx_replay_analytics_assay.rs`.
- **Lodestar** = DESYNC forensics. Compares two divergent replays tick-by-tick and
  reports the first divergent tick, per-body deltas, and whether the order log
  diverged. `calyx_replay_analytics_desync.rs`. (The code names the API `desync`;
  "Lodestar" is the subsystem name from #907.)
- **Oracle** = crash/anomaly RECURRENCE forecasting over crash-event telemetry
  streams; predicts the next likely recurrence or rejects on insufficient history.
  `calyx_replay_analytics_oracle.rs` (backed by `EngineOracleService` from
  `live_world_forecast`).

## 1. Run the whole loop (the FSV harness)

```
cargo run -p litd-engine --features fsv --example issue907_replay_analytics_fsv
```
Artifacts land under the run dir: `engine-aster-vault/` and `oracle-aster-vault/`
(the Aster vaults = the source of truth) plus `manual-fsv-report.json`. The final
step REOPENS the vault from disk and re-reads every row -- command success alone is
not proof.

## 2. Record a replay (produce the input)

Analytics ingest `SimReplayRecord`s (`crates/litd-engine/src/sim_replay.rs`:
`to_ron_string` / `replay::<MAX_BODIES,MAX_ORDERS>()`). You get them by capturing a
play session in the editor:
`crates/litd-editor/src/play_session_capture.rs` -- `capture_play_session(...)`
writes a ledger (schema `litd.play_session.capture.v1`) whose
`PlaySessionReplayProof.record` is a `SimReplayRecord`. Examples:
`crates/litd-editor/examples/issue856_replay_record_playback_fsv.rs`,
`session_capture_fsv.rs`, `issue788_agent_replay_fsv.rs`.

## 3. Ingest replays and run each pass (real APIs)

Ingest a batch (`calyx_replay_analytics.rs`). Each replay is re-executed;
win/loss + the tick-stamped event log come from that execution:
```rust
let batch = store_replay_match_batch::<MB, MO>(&database, batch_id, &matches)?;
// read back: read_replay_match_batch(&database, batch_id)
```
`ReplayMatchRecord { match_id, replay, subject_bodies, opponent_bodies, stats_milli }`.
Stored payload: `outcome`, `subject_damage_fp`, `opponent_damage_fp`, `tick_events`,
`replay_ron_sha256`, `initial_/final_state_hash`. Row ids: `replay_match_row_id`,
`replay_batch_row_id`. Schemas: `litd.engine.replay_analytics.match.v1` / `.batch.v1`.

Assay (which stats predict win/loss, in bits):
```rust
let assay = measure_replay_balance_assay(&database, batch_id)?; // read: read_replay_balance_assay
```
`ReplayBalanceAssayReadback.payload.ranked` -> `ReplayBalanceStatReport { stat_key,
bits, ci_low, ci_high, probe_accuracy }`. Anchor `match_outcome_win_loss`, schema
`litd.engine.replay_analytics.balance_assay.v1`. A driver stat carries strictly more
bits than a decoy (see the harness `verify_measured_balance`).

Lodestar / desync forensics (two replays -> divergence):
```rust
let d = run_desync_forensics::<MB, MO>(&database, case_id, &replay_a, &replay_b)?;
// read: read_desync_forensics(&database, case_id)
```
`DesyncForensicsPayload`: `verdict` (`DesyncVerdict::{Clean, Diverged, InitialStateDivergence}`),
`compared_ticks`, `first_divergent_tick`, `divergent_order_ticks`,
`divergent_tick_hash_a/b`, `order_log_divergence`, per-body `DesyncBodyDelta`.

Oracle (crash-recurrence forecast over a crash-event stream):
```rust
let mut oracle = EngineOracleService::open_with_clock(cfg, clock)?;
let f = forecast_crash_recurrence(&mut oracle, &database, forecast_id,
                                  crash_signature, &events, now_secs)?;
// read: read_crash_recurrence(&database, forecast_id)
```
`CrashRecurrencePayload`: `predicted_next_secs`, `support`, `rejection_reason`
(e.g. `InsufficientHistory` when history is too thin). Unsigned crash events fail loud.

## 4. Add a NEW telemetry signal end-to-end

Telemetry today is (a) the tick-stamped attack-event log extracted from the
executed replay (`final_snapshot.attack_events` -> `ReplayTickEvent`), and (b) the
structured `stats_milli` map on each `ReplayMatchRecord` (milli-units, e.g.
`"damage_milli" -> 4200`). To add a balance-relevant signal:

1. **Emit / capture in the sim.** For an event-shaped signal, record it in the
   sim's attack/event log so it appears in the replay's `final_snapshot`
   (`sim.rs` / `sim_types.rs`). For a per-match stat, populate a new key in
   `ReplayMatchRecord.stats_milli` at capture time.
2. **Schema / ingestion.** New event fields flow through `execute_and_derive`
   (`calyx_replay_analytics.rs:238`) into `ReplayMatchStoredPayload`
   (bump `REPLAY_MATCH_SCHEMA` if the shape changes). A new `stats_milli` key needs
   no schema bump -- the batch stat roster is validated for consistency across the
   batch (`validate_batch`).
3. **Visible in analysis.** Assay reads every `stats_milli` key, so a new stat is
   automatically ranked by predictive bits in the next
   `measure_replay_balance_assay`. Confirm it via reopen-readback.

## 5. Balance-tuning loop (#1680) -- keep short

Authored ability tables (editor #961) -> playtest / agent-ladder replays ->
Assay per-matchup/per-level-band win rates -> tune the tables -> re-run. The
framework (CF levers + 3D-only levers + matchup web + live tuning + agent PK
ladder regression) is specified in `gh issue view 1680`. Read it for the levers;
do not restate them here.

## 6. Desync workflow -- what to collect for Lodestar

On a divergence between two peers, collect both peers' `SimReplayRecord`s (same
`run_ticks`, same body count -- forensics REJECTS incomparable inputs) and run
`run_desync_forensics`. The classic root cause is a corrupted input order (off-by-
one damage), which surfaces as `Diverged` at the first divergent tick with
`order_log_divergence = true`. See the harness `corrupted_peer_replay` for how a
divergent timeline is constructed and hash-verified.

## Planned / not-built-yet (do NOT invent commands for these)
- **Agent PK ladder as a nightly regression suite** (#1680 item 5, enabled by
  #980): PLANNED. `computer_opponents.rs` + `issue852_computer_opponents_fsv.rs`
  exist, but there is no nightly cross-matchup ladder harness feeding Assay yet.
- **A live opt-in telemetry ingestion daemon** (#254): the #907 modules ingest
  REPLAYS (re-executed offline), not a live event stream from shipped games. If you
  need live-game telemetry ingestion, treat it as not-in-this-module.
- **Balance dashboard + outlier-drift alerts** (#1680 item 4, #799): PLANNED --
  Assay produces the ranked bits; the dashboard/alerting layer over it is not built.
