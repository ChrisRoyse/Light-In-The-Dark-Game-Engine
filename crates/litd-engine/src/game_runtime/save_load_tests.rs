//! Deterministic unit coverage for the #850 / #848 game-runtime systems.

use crate::{
    game_package::RuntimeCampaignHeroSlot,
    math::FixedVec3,
    sim::SimConfig,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};

use super::{
    CampaignKvStore, CampaignUnitSnapshot, GameRuntime, GameRuntimeError, GameSaveFile,
    HeroProgress, RuntimeEnvOrder, RuntimeEnvOrderKind, TimeOfDayConfig, TimeOfDayState,
    hero_carryover::{persist_hero_carryover, rehydrate_hero_carryover},
};

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn tod() -> TimeOfDayState {
    TimeOfDayState::new(
        TimeOfDayConfig {
            day_length_ticks: 100,
            base_sight_radius_mm: 12_000,
            night_sight_penalty_permille: 500,
        },
        0,
        0,
    )
    .expect("valid time-of-day config")
}

fn runtime() -> GameRuntime<4, 32> {
    let config = SimConfig::new(64).expect("valid sim config");
    let kv = CampaignKvStore::new("campaign.first_flame", 1).expect("valid kv scope");
    GameRuntime::new(config, 0x850, tod(), kv).expect("runtime builds")
}

#[test]
fn kv_store_round_trips_typed_values() {
    let mut kv = CampaignKvStore::new("campaign.first_flame", 2).unwrap();
    kv.put_integer("flags", "gold", 1500).unwrap();
    kv.put_text("flags", "hero_name", "Ashbearer").unwrap();
    kv.put_unit(
        "units",
        "boss",
        CampaignUnitSnapshot {
            unit_key: "unit.warlord".to_string(),
            level: 7,
            experience: 4200,
            hp_milli: 950_000,
            item_keys: vec!["item.blade".to_string()],
        },
    )
    .unwrap();

    assert_eq!(kv.get_integer("flags", "gold").unwrap(), 1500);
    assert_eq!(kv.get_text("flags", "hero_name").unwrap(), "Ashbearer");
    assert_eq!(kv.get_unit("units", "boss").unwrap().level, 7);
    assert_eq!(kv.len(), 3);
}

#[test]
fn kv_store_type_mismatch_and_missing_are_typed() {
    let mut kv = CampaignKvStore::new("campaign.first_flame", 1).unwrap();
    kv.put_integer("flags", "gold", 10).unwrap();

    let mismatch = kv.get_text("flags", "gold").unwrap_err();
    assert!(matches!(mismatch, GameRuntimeError::KvTypeMismatch { .. }));
    assert_eq!(mismatch.code(), "LITD_GAME_RUNTIME_KV_TYPE");

    let missing = kv.get_integer("flags", "absent").unwrap_err();
    assert!(matches!(missing, GameRuntimeError::KvKeyMissing { .. }));
    assert_eq!(missing.code(), "LITD_GAME_RUNTIME_KV_MISSING");
}

#[test]
fn kv_store_hash_is_order_independent_of_insertion() {
    let mut a = CampaignKvStore::new("c", 1).unwrap();
    a.put_integer("x", "a", 1).unwrap();
    a.put_integer("x", "b", 2).unwrap();
    let mut b = CampaignKvStore::new("c", 1).unwrap();
    b.put_integer("x", "b", 2).unwrap();
    b.put_integer("x", "a", 1).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn day_night_progression_alters_sight_radius_deterministically() {
    let mut state = tod();
    // Midnight: darkness at maximum -> penalty halves the base radius.
    assert_eq!(state.tick_in_day, 0);
    assert!(state.is_night());
    assert_eq!(state.sight_radius_input_mm(), 6_000);

    // Advance to noon (tick 50): full daylight -> base radius restored.
    for _ in 0..50 {
        state.advance_tick().unwrap();
    }
    assert_eq!(state.time_permille(), 500);
    assert!(!state.is_night());
    assert_eq!(state.sight_radius_input_mm(), 12_000);

    // A second identical progression reproduces the same hash exactly.
    let mut replay = tod();
    for _ in 0..50 {
        replay.advance_tick().unwrap();
    }
    assert_eq!(state.state_hash(), replay.state_hash());
}

#[test]
fn weather_change_order_round_trips_through_runtime() {
    let mut rt = runtime();
    assert_eq!(rt.time_of_day().weather_ordinal, 0);
    rt.enqueue_env_order(RuntimeEnvOrder::new(
        3,
        1,
        RuntimeEnvOrderKind::SetWeather {
            ordinal: 1,
            intensity_permille: 700,
        },
    ))
    .unwrap();
    rt.run_ticks(4).unwrap();
    assert_eq!(rt.time_of_day().weather_ordinal, 1);
    assert_eq!(rt.time_of_day().weather_intensity_permille, 700);
}

#[test]
fn set_time_of_day_order_is_applied() {
    let mut rt = runtime();
    rt.enqueue_env_order(RuntimeEnvOrder::new(
        2,
        1,
        RuntimeEnvOrderKind::SetTimeOfDay { tick_in_day: 50 },
    ))
    .unwrap();
    rt.run_ticks(2).unwrap();
    // Applied at tick 2 (set to 50) then advanced one tick to 51.
    assert_eq!(rt.time_of_day().tick_in_day, 51);
}

/// Build a runtime with a body and a stream of scheduled orders, run to `to_tick`.
fn seeded_runtime() -> GameRuntime<4, 32> {
    let mut rt = runtime();
    let body = rt
        .sim_mut()
        .spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))
        .unwrap();
    rt.kv_mut().put_integer("progress", "chapter", 1).unwrap();
    for tick in 1..=10 {
        rt.enqueue_order(SimOrder::new(
            tick,
            SimOrderSource::human(1),
            tick,
            body,
            SimOrderKind::AddVelocity(v(1, 2, 3)),
        ))
        .unwrap();
    }
    rt.enqueue_env_order(RuntimeEnvOrder::new(
        7,
        1,
        RuntimeEnvOrderKind::SetWeather {
            ordinal: 2,
            intensity_permille: 400,
        },
    ))
    .unwrap();
    rt
}

#[test]
fn save_load_continues_deterministically_acid_test() {
    // Run to tick N, save, continue to N+K -> hash1.
    let mut original = seeded_runtime();
    original.run_ticks(4).unwrap();
    let save = original.capture_save().unwrap();
    original.run_ticks(6).unwrap();
    let continued_hash = original.combined_state_hash();

    // Reload the save, replay the same orders to N+K -> hash2. Must match exactly.
    let mut reloaded = GameRuntime::<4, 32>::restore_from_save(&save).unwrap();
    let reloaded_hash = reloaded.run_ticks(6).unwrap();
    assert_eq!(continued_hash, reloaded_hash);
}

#[test]
fn replay_determinism_same_orders_same_hash() {
    let first = {
        let mut rt = seeded_runtime();
        rt.run_ticks(10).unwrap()
    };
    let second = {
        let mut rt = seeded_runtime();
        rt.run_ticks(10).unwrap()
    };
    assert_eq!(first, second);
}

#[test]
fn save_round_trips_through_disk_and_validates() {
    let mut rt = seeded_runtime();
    rt.run_ticks(4).unwrap();
    let save = rt.capture_save().unwrap();
    let text = save.to_ron_string().unwrap();
    let parsed = GameSaveFile::from_ron_str(&text).unwrap();
    parsed.validate().unwrap();
    assert_eq!(parsed, save);
}

#[test]
fn corrupt_save_is_rejected_typed() {
    let err = GameSaveFile::from_ron_str("this is not a save file").unwrap_err();
    assert!(matches!(err, GameRuntimeError::Decode { .. }));
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_DECODE");
}

#[test]
fn tampered_fingerprint_is_rejected_typed() {
    let mut rt = seeded_runtime();
    rt.run_ticks(4).unwrap();
    let mut save = rt.capture_save().unwrap();
    // Tamper with the KV store without recomputing the fingerprint.
    save.kv.put_integer("progress", "chapter", 999).unwrap();
    let err = save.validate().unwrap_err();
    assert!(matches!(err, GameRuntimeError::FingerprintMismatch { .. }));
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_FINGERPRINT");

    // Restore must also refuse the tampered save.
    match GameRuntime::<4, 32>::restore_from_save(&save) {
        Err(GameRuntimeError::FingerprintMismatch { .. }) => {}
        Err(other) => panic!("expected FingerprintMismatch, got {other:?}"),
        Ok(_) => panic!("tampered save must not restore"),
    }
}

fn hero_slot(carries_level: bool, carries_items: bool) -> RuntimeCampaignHeroSlot {
    RuntimeCampaignHeroSlot {
        slot_id: "slot.protagonist".to_string(),
        hero_key: "hero.ashbearer".to_string(),
        carries_level,
        carries_items,
        active_through_chapter: None,
    }
}

#[test]
fn hero_carryover_persists_and_rehydrates_across_chapters() {
    let mut kv = CampaignKvStore::new("campaign.first_flame", 1).unwrap();
    let slot = hero_slot(true, true);
    let progress = HeroProgress {
        hero_key: "hero.ashbearer".to_string(),
        level: 12,
        experience: 88_000,
        hp_milli: 1_200_000,
        item_keys: vec!["item.emberblade".to_string(), "item.cloak".to_string()],
    };
    persist_hero_carryover(&mut kv, &slot, &progress).unwrap();
    let rehydrated = rehydrate_hero_carryover(&kv, &slot).unwrap();
    assert_eq!(rehydrated, progress);
}

#[test]
fn hero_carryover_honors_carry_flags() {
    let mut kv = CampaignKvStore::new("campaign.first_flame", 1).unwrap();
    let slot = hero_slot(false, false);
    let progress = HeroProgress {
        hero_key: "hero.ashbearer".to_string(),
        level: 12,
        experience: 88_000,
        hp_milli: 1_200_000,
        item_keys: vec!["item.emberblade".to_string()],
    };
    persist_hero_carryover(&mut kv, &slot, &progress).unwrap();
    let rehydrated = rehydrate_hero_carryover(&kv, &slot).unwrap();
    // Level/XP reset to baseline; items dropped; HP still carried.
    assert_eq!(rehydrated.level, 1);
    assert_eq!(rehydrated.experience, 0);
    assert!(rehydrated.item_keys.is_empty());
    assert_eq!(rehydrated.hp_milli, 1_200_000);
}

#[test]
fn hero_carryover_rejects_mismatched_hero() {
    let mut kv = CampaignKvStore::new("campaign.first_flame", 1).unwrap();
    let slot = hero_slot(true, true);
    let progress = HeroProgress {
        hero_key: "hero.other".to_string(),
        level: 1,
        experience: 0,
        hp_milli: 1,
        item_keys: Vec::new(),
    };
    let err = persist_hero_carryover(&mut kv, &slot, &progress).unwrap_err();
    assert!(matches!(err, GameRuntimeError::Invalid { .. }));
}
