//! #1463 defect 1: attack-event hashing is O(1) per tick and attack memory is
//! bounded, without weakening lockstep determinism.
//!
//! Source of truth: `FixedTickSim` in-memory state (persistent digest, total,
//! bounded retention window) plus a persisted FSV JSON report read back.

use std::time::Instant;

use crate::{
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    sim::{ATTACK_EVENT_RETENTION, FixedTickSim, SimBodyId, SimConfig},
    sim_replay::SimSnapshot,
};
use serde_json::{Value, json};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

/// A two-body sim with `count` attacks recorded at tick 0. `first_damage`
/// overrides the very first attack's damage so two otherwise-identical
/// sequences can be made to differ only in an attack that will be evicted.
fn sim_with_attacks(count: u64, first_damage: i64) -> (FixedTickSim<2>, SimBodyId, SimBodyId) {
    let config = SimConfig::new(64).unwrap();
    let mut sim = FixedTickSim::<2>::new(config, 1463).unwrap();
    let attacker = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let target = sim.spawn_body(v(1, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    for i in 0..count {
        let damage = if i == 0 {
            first_damage
        } else {
            (i % 97) as i64 + 1
        };
        sim.record_attack_at(0, attacker, target, damage).unwrap();
    }
    (sim, attacker, target)
}

fn window_len(sim: &FixedTickSim<2>) -> usize {
    sim.attack_events().count()
}

/// Median-of-3 wall-clock nanos to compute `state_hash` `iterations` times.
fn hash_time_nanos(sim: &FixedTickSim<2>, iterations: u32) -> u128 {
    let mut samples = [0_u128; 3];
    for sample in &mut samples {
        let start = Instant::now();
        let mut acc = 0_u64;
        for _ in 0..iterations {
            acc ^= sim.state_hash();
        }
        // Prevent the loop from being optimized away.
        assert_ne!(acc, 0xDEAD_BEEF_DEAD_BEEF);
        *sample = start.elapsed().as_nanos();
    }
    samples.sort_unstable();
    samples[1]
}

#[test]
fn issue1463_attack_hashing_is_o1_and_memory_is_bounded() {
    // N far exceeds the retention window so eviction is exercised.
    const N: u64 = 50_000;
    assert!(N as usize > ATTACK_EVENT_RETENTION);

    // --- Determinism: identical sequences -> identical persistent state. ---
    let (sim_a, _, _) = sim_with_attacks(N, 1);
    let (sim_a2, _, _) = sim_with_attacks(N, 1);
    assert_eq!(sim_a.state_hash(), sim_a2.state_hash());
    assert_eq!(sim_a.attack_events_digest(), sim_a2.attack_events_digest());
    assert_eq!(sim_a.attack_event_count(), N as usize);

    // --- Bounded memory: window capped, but the total keeps the full count. ---
    let window_after = window_len(&sim_a);
    assert_eq!(window_after, ATTACK_EVENT_RETENTION);
    assert_eq!(sim_a.attack_event_count(), N as usize);

    // --- Full-history coverage despite the bounded window (the key proof). ---
    // sim_b differs from sim_a ONLY in attack #0, which both sims evict (N > cap).
    // Their retained windows are byte-for-byte identical, yet their hashes differ:
    // the persistent digest carries the evicted attack, so lockstep replay still
    // detects divergence in history outside the window.
    let (sim_b, _, _) = sim_with_attacks(N, 2);
    let windows_identical = sim_a.attack_events().eq(sim_b.attack_events());
    assert!(
        windows_identical,
        "retained windows must be identical (only the evicted attack #0 differs)"
    );
    assert_ne!(
        sim_a.attack_events_digest(),
        sim_b.attack_events_digest(),
        "digest must reflect the evicted attack — full-history determinism"
    );
    assert_ne!(sim_a.state_hash(), sim_b.state_hash());

    // --- Snapshot restore reconstructs the digest+total, not from the window. ---
    let snapshot = SimSnapshot::capture(&sim_a);
    assert_eq!(snapshot.attack_events_total, N);
    assert_eq!(snapshot.attack_events.len(), ATTACK_EVENT_RETENTION);
    let restored = snapshot.restore::<2>().unwrap();
    assert_eq!(restored.state_hash(), sim_a.state_hash());
    assert_eq!(restored.attack_event_count(), N as usize);
    assert_eq!(
        restored.attack_events_digest(),
        sim_a.attack_events_digest()
    );

    // --- O(1) hash time: cost at 100 vs 50k attacks is ~flat (not O(N)). ---
    let (sim_small, _, _) = sim_with_attacks(100, 1);
    let small_ns = hash_time_nanos(&sim_small, 200_000);
    let large_ns = hash_time_nanos(&sim_a, 200_000);
    // Under the old O(total) loop, 50k attacks would be ~500x slower per hash.
    // A 10x ceiling cleanly separates O(1) from O(N) while tolerating noise.
    if small_ns > 0 {
        assert!(
            large_ns <= small_ns.saturating_mul(10),
            "state_hash scaled with attack count: 100 attacks={small_ns}ns, 50k attacks={large_ns}ns"
        );
    }

    // --- Edge cases: exactly-at, one-over, and empty. ---
    let exact = edge_exact_boundary();
    let over = edge_one_over_boundary();
    let empty = edge_empty();

    let report = json!({
        "schema_version": 1,
        "issue": 1463,
        "defect": "attack_event_hashing_and_memory",
        "status": "passed",
        "source_of_truth": "FixedTickSim in-memory state + persisted JSON report",
        "primary": {
            "attacks_recorded": N,
            "retention_cap": ATTACK_EVENT_RETENTION,
            "window_after": window_after,
            "total_after": sim_a.attack_event_count(),
            "digest_a": format!("{:016x}", sim_a.attack_events_digest()),
            "digest_b_evicted_attack_differs": format!("{:016x}", sim_b.attack_events_digest()),
            "windows_identical": windows_identical,
            "hash_ns_100_attacks": small_ns,
            "hash_ns_50k_attacks": large_ns,
            "restored_hash_matches": restored.state_hash() == sim_a.state_hash()
        },
        "edge_cases": {
            "exactly_at_retention": exact,
            "one_over_retention": over,
            "empty": empty
        }
    });
    let run = begin_issue_run("issue1463_attack_retention").unwrap();
    let report_path = run.report_path().to_path_buf();
    let readback = run.write_report(&report).unwrap();
    assert_eq!(readback.json["status"], "passed");
    assert_eq!(
        readback.json["primary"]["window_after"],
        ATTACK_EVENT_RETENTION
    );
    assert_eq!(readback.json["primary"]["total_after"], N);
    println!(
        "ISSUE1463_ATTACK_RETENTION_FSV path={} bytes={} sha256={} window={} total={} hash_100={}ns hash_50k={}ns",
        report_path.display(),
        readback.bytes.len(),
        readback.sha256,
        window_after,
        N,
        small_ns,
        large_ns
    );
}

/// Exactly `ATTACK_EVENT_RETENTION` attacks: window holds the full history, so
/// folding the window reproduces the digest.
fn edge_exact_boundary() -> Value {
    let (sim, _, _) = sim_with_attacks(ATTACK_EVENT_RETENTION as u64, 1);
    assert_eq!(window_len(&sim), ATTACK_EVENT_RETENTION);
    assert_eq!(sim.attack_event_count(), ATTACK_EVENT_RETENTION);
    // Restore round-trips (window == full history here).
    let restored = SimSnapshot::capture(&sim).restore::<2>().unwrap();
    assert_eq!(restored.state_hash(), sim.state_hash());
    json!({
        "attacks": ATTACK_EVENT_RETENTION,
        "window": window_len(&sim),
        "total": sim.attack_event_count(),
        "restored_hash_matches": true
    })
}

/// One over the cap: the oldest attack is evicted; total still counts it and the
/// digest still reflects it.
fn edge_one_over_boundary() -> Value {
    let count = ATTACK_EVENT_RETENTION as u64 + 1;
    let (sim, _, _) = sim_with_attacks(count, 1);
    assert_eq!(window_len(&sim), ATTACK_EVENT_RETENTION);
    assert_eq!(sim.attack_event_count(), count as usize);
    // Same sequence but the evicted first attack differs -> different hash.
    let (other, _, _) = sim_with_attacks(count, 2);
    assert_ne!(sim.state_hash(), other.state_hash());
    json!({
        "attacks": count,
        "window": window_len(&sim),
        "total": sim.attack_event_count(),
        "evicted_attack_still_hashed": true
    })
}

/// Empty: no attacks -> zero digest, zero total, empty window.
fn edge_empty() -> Value {
    let (sim, _, _) = sim_with_attacks(0, 1);
    assert_eq!(window_len(&sim), 0);
    assert_eq!(sim.attack_event_count(), 0);
    assert_eq!(sim.attack_events_digest(), 0);
    json!({ "attacks": 0, "window": 0, "total": 0, "digest": "0000000000000000" })
}
