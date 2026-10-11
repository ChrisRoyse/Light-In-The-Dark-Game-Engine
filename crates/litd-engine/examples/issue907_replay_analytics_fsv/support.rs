//! Support for the #907 FSV: deterministic real-match generation and
//! telemetry fixtures. Every replay here is genuinely executed through the
//! deterministic sim; expected hashes come from actual runs.

use std::collections::BTreeMap;

use litd_engine::{
    calyx_observability::{EngineTelemetryEvent, EngineTelemetryEventKind},
    calyx_replay_analytics::ReplayMatchRecord,
    math::FixedVec3,
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord, SimSnapshot},
};

pub const MB: usize = 2;
pub const MO: usize = 16;
pub const RUN_TICKS: u64 = 8;
pub const MATCH_COUNT: usize = 80;
pub const OPPONENT_ATTACKS: u64 = 3;
pub const OPPONENT_DAMAGE_FP: i64 = 700;
pub const DRIVER_STATS: [&str; 2] = ["damage_milli", "attack_speed_milli"];
pub const DECOY_STATS: [&str; 4] = [
    "armor_milli",
    "cosmetic_milli",
    "economy_milli",
    "speed_milli",
];

/// Deterministic xorshift64* stream so the whole FSV is reproducible.
pub struct DeterministicStream {
    state: u64,
}

impl DeterministicStream {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn in_range(&mut self, low: i64, high: i64) -> i64 {
        assert!(high > low, "in_range requires high > low");
        let span = (high - low) as u64;
        low + (self.next_u64() % span) as i64
    }
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

/// Build one REAL executed match. The subject's scripted attack plan derives
/// ONLY from `damage_milli` and `attack_speed_milli`; the decoy stats are
/// carried but never influence the sim — measured balance must find exactly
/// that.
pub fn build_match(index: usize, stream: &mut DeterministicStream) -> ReplayMatchRecord {
    // Odd damage times an odd attack count can never tie the opponent's even
    // total (3 x 700 = 2100), so every generated match has a decisive outcome.
    let damage_milli = 201 + 2 * stream.in_range(0, 500);
    let attack_speed_milli = stream.in_range(0, 1250);
    let attack_count = 1 + 2 * (attack_speed_milli / 417) as u64;
    let mut stats_milli = BTreeMap::new();
    stats_milli.insert("damage_milli".to_string(), damage_milli);
    stats_milli.insert("attack_speed_milli".to_string(), attack_speed_milli);
    for decoy in DECOY_STATS {
        stats_milli.insert(decoy.to_string(), stream.in_range(0, 2000));
    }

    let config = SimConfig::new(32).expect("sim config");
    let seed = 907_000 + index as u64;
    let mut sim = FixedTickSim::<MB>::new(config, seed).expect("sim");
    let subject = sim
        .spawn_body(v(0, 0, 0), v(1, 0, 0), v(0, 0, 0))
        .expect("subject");
    let opponent = sim
        .spawn_body(v(6, 0, 0), v(-1, 0, 0), v(0, 0, 0))
        .expect("opponent");
    let initial = SimSnapshot::capture(&sim);
    let orders = match_orders(subject, opponent, damage_milli, attack_count);
    let mut queue = SimOrderQueue::<MO>::default();
    for order in &orders {
        queue.enqueue(*order).expect("enqueue");
    }
    let expected = queue
        .run_ticks(&mut sim, RUN_TICKS)
        .expect("run")
        .state_hash;
    ReplayMatchRecord {
        match_id: format!("match-{index:03}"),
        replay: SimReplayRecord {
            schema_version: SIM_REPLAY_SCHEMA_VERSION,
            initial_snapshot: initial,
            orders,
            run_ticks: RUN_TICKS,
            expected_final_hash: expected,
        },
        subject_bodies: vec![subject.index()],
        opponent_bodies: vec![opponent.index()],
        stats_milli,
    }
}

pub fn match_orders(
    subject: SimBodyId,
    opponent: SimBodyId,
    damage_milli: i64,
    attack_count: u64,
) -> Vec<SimOrder> {
    let mut orders = Vec::new();
    let mut sequence = 1;
    for attack in 0..attack_count {
        orders.push(SimOrder::new(
            1 + attack,
            SimOrderSource::human(1),
            sequence,
            opponent,
            SimOrderKind::ApplyAttack {
                attacker: subject,
                damage_fp: damage_milli,
            },
        ));
        sequence += 1;
    }
    for attack in 0..OPPONENT_ATTACKS {
        orders.push(SimOrder::new(
            1 + attack,
            SimOrderSource::human(2),
            sequence,
            subject,
            SimOrderKind::ApplyAttack {
                attacker: opponent,
                damage_fp: OPPONENT_DAMAGE_FP,
            },
        ));
        sequence += 1;
    }
    orders
}

pub fn crash_event(
    event_id: &str,
    signature: Option<&str>,
    observed_at_ms: u64,
) -> EngineTelemetryEvent {
    EngineTelemetryEvent {
        event_id: event_id.to_string(),
        source: "litd-game".to_string(),
        kind: EngineTelemetryEventKind::Crash,
        observed_at_ms,
        latency_ms: None,
        error_code: Some("LITD-GPU-NO-ADAPTER".to_string()),
        chain_ok: None,
        crash_signature: signature.map(str::to_string),
        agent_ood_score_milli: None,
        frame_time_ns: None,
        endpoint_ok: None,
        provenance_refs: vec!["telemetry://opt-in/issue254".to_string()],
    }
}

/// Regularly recurring crash series: one crash every `period_secs`, starting
/// at `start_secs`.
pub fn crash_series(
    signature: &str,
    start_secs: u64,
    period_secs: u64,
    count: usize,
) -> Vec<EngineTelemetryEvent> {
    (0..count)
        .map(|index| {
            crash_event(
                &format!("crash-{signature}-{index:02}"),
                Some(signature),
                (start_secs + period_secs * index as u64) * 1000,
            )
        })
        .collect()
}
