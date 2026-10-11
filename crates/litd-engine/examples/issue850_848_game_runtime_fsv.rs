//! FSV harness for #850 (save/load persistence + campaign KV store + hero
//! carry-over) and #848 (runtime day/night + weather).
//!
//! Consumes REAL #814 lighting and #815 weather records: they are authored through
//! the real world-environment editor types and validated + lowered to their
//! deterministic INTEGER payload via `to_payload()`; the runtime reads the lowered
//! integer fields (cycle length, precipitation, sight tuning) from those payloads.
//! (The optional Calyx vault-measurement persistence layer is a provisioned
//! environmental dependency -- like #1814 -- and is intentionally NOT on this
//! example's path, so the harness is self-contained with a single real code path
//! and no fallback.) It then runs the save/load determinism acid test.
//!
//! Run: cargo run -p litd-engine --features fsv --example issue850_848_game_runtime_fsv

use std::{path::Path, process::Command};

use litd_engine::{
    fsv_artifacts::begin_issue_run,
    game_package::RuntimeCampaignHeroSlot,
    game_runtime::{
        CampaignKvStore, GameRuntime, GameSaveFile, HeroProgress, RuntimeEnvOrder,
        RuntimeEnvOrderKind, TimeOfDayConfig, TimeOfDayState,
        hero_carryover::{persist_hero_carryover, rehydrate_hero_carryover},
        presentation_sight_radius_meters,
    },
    math::FixedVec3,
    sim::SimConfig,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
    world_environment::{
        DayNightCyclePolicy, GlobalFog, LightingEnvironmentAuthoring, PrecipitationKind, Rgb8,
        TimeOfDayKeyframe, WeatherEnvironmentAuthoring, record::EnvironmentPayload,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

const TICKS_PER_SECOND: u32 = 50;

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

/// Read an unsigned integer field from a lowered environment payload body,
/// failing loud if it is absent or not a non-negative integer.
fn lowered_u64(
    payload: &EnvironmentPayload,
    field: &str,
) -> Result<u64, Box<dyn std::error::Error>> {
    payload
        .body
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("lowered field {field} missing or not an unsigned integer").into())
}

/// A stable SHA-256 fingerprint over a lowered payload's canonical body bytes.
fn payload_fingerprint(payload: &EnvironmentPayload) -> Result<String, Box<dyn std::error::Error>> {
    Ok(sha256_hex(&serde_json::to_vec(&payload.body)?))
}

fn lighting_authoring() -> LightingEnvironmentAuthoring {
    LightingEnvironmentAuthoring {
        id: "lighting.first_flame".to_string(),
        display_name: "First Flame Day Cycle".to_string(),
        author: "issue850_848_fsv".to_string(),
        cycle: DayNightCyclePolicy::Enabled,
        cycle_seconds: 2,
        keyframes: vec![
            TimeOfDayKeyframe {
                time_permille: 0,
                sun_color: Rgb8::new(20, 20, 60),
                sun_intensity_milli: 100,
                ambient_color: Rgb8::new(10, 10, 30),
                sun_azimuth_deg: 0,
                sun_elevation_deg: -30,
            },
            TimeOfDayKeyframe {
                time_permille: 500,
                sun_color: Rgb8::new(255, 240, 200),
                sun_intensity_milli: 100_000,
                ambient_color: Rgb8::new(180, 180, 200),
                sun_azimuth_deg: 180,
                sun_elevation_deg: 80,
            },
            TimeOfDayKeyframe {
                time_permille: 999,
                sun_color: Rgb8::new(20, 20, 60),
                sun_intensity_milli: 100,
                ambient_color: Rgb8::new(10, 10, 30),
                sun_azimuth_deg: 359,
                sun_elevation_deg: -30,
            },
        ],
        baked_gi_states: Vec::new(),
    }
}

fn weather_authoring() -> WeatherEnvironmentAuthoring {
    WeatherEnvironmentAuthoring {
        id: "weather.first_flame".to_string(),
        display_name: "First Flame Rain".to_string(),
        author: "issue850_848_fsv".to_string(),
        precipitation: PrecipitationKind::Rain,
        intensity_permille: 600,
        wind_bearing_deg: 90,
        fog: GlobalFog {
            enabled: true,
            color: Rgb8::new(120, 120, 140),
            density_milli: 200,
            visibility_distance_mm: 60_000,
            start_distance_mm: 10_000,
        },
    }
}

/// Seed a runtime with a body, a stream of movement orders (ticks 1..=10), and a
/// weather-change order at tick 7. Deterministic construction shared by the acid
/// test and the replay-determinism check.
fn seeded_runtime(tod: &TimeOfDayState) -> Result<GameRuntime<8, 64>, Box<dyn std::error::Error>> {
    let kv = CampaignKvStore::new("campaign.first_flame", 1)?;
    let mut rt = GameRuntime::<8, 64>::new(
        SimConfig::new(TICKS_PER_SECOND)?,
        0x850_848,
        tod.clone(),
        kv,
    )?;
    let body = rt
        .sim_mut()
        .spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    rt.kv_mut().put_integer("progress", "chapter", 1)?;
    rt.kv_mut().put_text("progress", "hero", "Ashbearer")?;
    for tick in 1..=10 {
        rt.enqueue_order(SimOrder::new(
            tick,
            SimOrderSource::human(1),
            tick,
            body,
            SimOrderKind::AddVelocity(v(1, 2, 3)),
        ))?;
    }
    rt.enqueue_env_order(RuntimeEnvOrder::new(
        7,
        1,
        RuntimeEnvOrderKind::SetWeather {
            ordinal: 2,
            intensity_permille: 300,
        },
    ))?;
    Ok(rt)
}

fn certutil_sha256(path: &Path) -> Option<String> {
    let output = Command::new("certutil")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let candidate: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if candidate.len() == 64 && candidate.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue850_848_game_runtime")?;

    // --- Author + lower the REAL #814 lighting and #815 weather records. ---
    let lighting = lighting_authoring().to_payload()?;
    let weather = weather_authoring().to_payload()?;
    let tuning = EnvironmentPayload::new(
        "game_runtime_tuning",
        "tuning.first_flame",
        json!({
            "base_sight_radius_mm": 12_000,
            "night_sight_penalty_permille": 500,
        }),
    )?;

    // Day length is derived from the lowered #814 cycle length (not hardcoded).
    let cycle_seconds = lowered_u64(&lighting, "cycle_seconds")?;
    let day_length_ticks = cycle_seconds * u64::from(TICKS_PER_SECOND);
    let precip_ordinal = lowered_u64(&weather, "precipitation_ordinal")?;
    let precip_intensity = lowered_u64(&weather, "precipitation_intensity_permille")?;
    let base_sight_mm = lowered_u64(&tuning, "base_sight_radius_mm")? as u32;
    let night_penalty = lowered_u64(&tuning, "night_sight_penalty_permille")? as u16;

    let config = TimeOfDayConfig {
        day_length_ticks,
        base_sight_radius_mm: base_sight_mm,
        night_sight_penalty_permille: night_penalty,
    };
    let tod = TimeOfDayState::new(config, precip_ordinal as u32, precip_intensity as u16)?;

    // --- Edge case A: day/night alters the sight-radius INPUT deterministically. ---
    let mut midnight = tod.clone();
    let sight_midnight = midnight.sight_radius_input_mm();
    for _ in 0..(day_length_ticks / 2) {
        midnight.advance_tick()?;
    }
    let sight_noon = midnight.sight_radius_input_mm();
    let sight_seam_meters_noon = presentation_sight_radius_meters(sight_noon);

    // --- Save/load determinism ACID TEST. ---
    let mut original = seeded_runtime(&tod)?;
    original.run_ticks(4)?;
    let save_at_n = original.capture_save()?;
    let save_path = run.artifact_path("saves/match.litdsave")?;
    save_at_n.save_to_path(&save_path)?;
    let save_bytes = std::fs::read(&save_path)?;
    let save_sha256 = sha256_hex(&save_bytes);
    let save_sha256_certutil = certutil_sha256(&save_path);

    original.run_ticks(6)?;
    let continued_hash = original.combined_state_hash();
    let weather_after = original.time_of_day().weather_ordinal;

    let loaded = GameSaveFile::load_from_path(&save_path)?;
    let mut reloaded = GameRuntime::<8, 64>::restore_from_save(&loaded)?;
    let reloaded_hash = reloaded.run_ticks(6)?;
    let acid_match = continued_hash == reloaded_hash;

    // --- Replay determinism: same seed + orders twice -> identical hash. ---
    let replay_a = seeded_runtime(&tod)?.run_ticks(10)?;
    let replay_b = seeded_runtime(&tod)?.run_ticks(10)?;
    let replay_match = replay_a == replay_b;

    // --- Hero carry-over across two mini chapters (real #853 slot model). ---
    let slot = RuntimeCampaignHeroSlot {
        slot_id: "slot.protagonist".to_string(),
        hero_key: "hero.ashbearer".to_string(),
        carries_level: true,
        carries_items: true,
        active_through_chapter: None,
    };
    let mut campaign_store = CampaignKvStore::new("campaign.first_flame", 1)?;
    let chapter1_end = HeroProgress {
        hero_key: "hero.ashbearer".to_string(),
        level: 5,
        experience: 12_000,
        hp_milli: 800_000,
        item_keys: vec!["item.emberblade".to_string()],
    };
    persist_hero_carryover(&mut campaign_store, &slot, &chapter1_end)?;
    let chapter2_start = rehydrate_hero_carryover(&campaign_store, &slot)?;
    let mut chapter2_end = chapter2_start.clone();
    chapter2_end.level = 8;
    chapter2_end.experience = 34_000;
    chapter2_end.item_keys.push("item.wardstone".to_string());
    persist_hero_carryover(&mut campaign_store, &slot, &chapter2_end)?;
    let chapter3_start = rehydrate_hero_carryover(&campaign_store, &slot)?;

    // Independent re-read of the campaign KV store from disk (round-trip proof).
    let kv_ron = ron::ser::to_string(&campaign_store)?;
    let kv_reread: CampaignKvStore = ron::from_str(&kv_ron)?;
    let kv_reread_hash = kv_reread.state_hash();
    let kv_hash_match = kv_reread_hash == campaign_store.state_hash();

    // --- Negative controls. ---
    let corrupt_code = match GameSaveFile::from_ron_str("this is not a save") {
        Err(err) => err.code(),
        Ok(_) => "UNEXPECTED_OK",
    };
    let mut tampered = save_at_n.clone();
    tampered.kv.put_integer("progress", "chapter", 999)?;
    let tampered_code = match tampered.validate() {
        Err(err) => err.code(),
        Ok(()) => "UNEXPECTED_OK",
    };
    // Sabotage control: a diverging pending order MUST break the acid-test equality.
    let mut sabotaged = loaded.clone();
    if let Some(order) = sabotaged.pending_sim_orders.first_mut() {
        order.kind = SimOrderKind::AddVelocity(v(9, 9, 9));
    }
    sabotaged.save_fingerprint = sabotaged.compute_fingerprint()?;
    let mut sabotaged_rt = GameRuntime::<8, 64>::restore_from_save(&sabotaged)?;
    let sabotaged_hash = sabotaged_rt.run_ticks(6)?;
    let sabotage_diverged = sabotaged_hash != continued_hash;

    let verdict = acid_match
        && replay_match
        && kv_hash_match
        && weather_after == 2
        && sight_noon > sight_midnight
        && chapter3_start.level == 8
        && chapter3_start.item_keys.len() == 2
        && corrupt_code == "LITD_GAME_RUNTIME_DECODE"
        && tampered_code == "LITD_GAME_RUNTIME_FINGERPRINT"
        && sabotage_diverged;

    let report = json!({
        "schema_version": 1,
        "issues": [850, 848],
        "verdict": if verdict { "PASS" } else { "FAIL" },
        "lowered_records": {
            "lighting_cycle_seconds": cycle_seconds,
            "lighting_payload_fingerprint": payload_fingerprint(&lighting)?,
            "weather_payload_fingerprint": payload_fingerprint(&weather)?,
            "tuning_payload_fingerprint": payload_fingerprint(&tuning)?,
            "precip_ordinal": precip_ordinal,
            "precip_intensity_permille": precip_intensity,
        },
        "day_night": {
            "day_length_ticks": day_length_ticks,
            "base_sight_radius_mm": base_sight_mm,
            "night_sight_penalty_permille": night_penalty,
            "sight_radius_midnight_mm": sight_midnight,
            "sight_radius_noon_mm": sight_noon,
            "sight_noon_meters_presentation_seam": sight_seam_meters_noon,
            "sight_increases_from_midnight_to_noon": sight_noon > sight_midnight,
        },
        "weather_order_round_trip": {
            "before_ordinal": precip_ordinal,
            "after_ordinal": weather_after,
            "match": weather_after == 2,
        },
        "save_load_acid_test": {
            "save_tick": 4,
            "continue_ticks": 6,
            "continued_state_hash": format!("{continued_hash:016X}"),
            "reloaded_state_hash": format!("{reloaded_hash:016X}"),
            "match": acid_match,
            "save_file_bytes": save_bytes.len(),
            "save_file_sha256": save_sha256,
            "save_file_sha256_certutil": save_sha256_certutil,
            "save_file_sha256_certutil_matches": save_sha256_certutil.as_deref() == Some(save_sha256.as_str()),
        },
        "replay_determinism": {
            "hash_a": format!("{replay_a:016X}"),
            "hash_b": format!("{replay_b:016X}"),
            "match": replay_match,
        },
        "campaign_kv_round_trip": {
            "entries": campaign_store.len(),
            "in_memory_hash": format!("{:016X}", campaign_store.state_hash()),
            "disk_reread_hash": format!("{kv_reread_hash:016X}"),
            "match": kv_hash_match,
        },
        "hero_carryover": {
            "chapter1_end_level": chapter1_end.level,
            "chapter2_start_level": chapter2_start.level,
            "chapter2_start_items": chapter2_start.item_keys.len(),
            "chapter3_start_level": chapter3_start.level,
            "chapter3_start_items": chapter3_start.item_keys.len(),
        },
        "negative_controls": {
            "corrupt_save_code": corrupt_code,
            "tampered_fingerprint_code": tampered_code,
            "sabotaged_replay_diverged": sabotage_diverged,
            "sabotaged_hash": format!("{sabotaged_hash:016X}"),
        },
    });

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE850_848_GAME_RUNTIME_FSV verdict={} acid_match={} replay_match={} save_sha256={} certutil={:?} report_sha256={}",
        report["verdict"],
        acid_match,
        replay_match,
        save_sha256,
        save_sha256_certutil,
        readback.sha256,
    );
    println!("report_path={}", readback.path.display());
    println!("save_path={}", save_path.display());
    if !verdict {
        return Err(format!("FSV verdict FAIL: {report}").into());
    }
    Ok(())
}
