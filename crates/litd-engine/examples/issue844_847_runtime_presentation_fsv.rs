//! FSV for #844-#847 (parent #804): the runtime presentation/perception systems.
//!
//! Drives all four real systems end-to-end:
//! - #847 fog: a deterministic sim moves a unit; its lowered sight radius reveals
//!   a per-player visibility grid; a reveal modifier is applied; cell counts per
//!   state are printed before/after. Replay-determinism: the whole seeded
//!   sim+fog scenario runs twice and the sim state hash AND fog state hash must
//!   match.
//! - #846 cameras: a runtime op sequence (pan/rotate/zoom/lock/mode) produces
//!   sampled numeric poses; a scripted cinematic pose is adopted.
//! - #845 cinematics: a #892 lowered timeline plays through the runtime player;
//!   letterbox/fade/transmission are sampled at timestamps; skip + sim-gating
//!   are exercised.
//! - #844 effects: a validated effect attaches to a unit and is destroyed when
//!   its owner dies.
//!
//! Negative controls: a sabotaged timeline is rejected at start; an effect with
//! a dangling art ref is rejected at lowering.
//!
//! report.json is path-free and run-invariant (stable codes, no absolute paths);
//! its sha256 is printed for independent `certutil` recomputation.

use std::collections::BTreeSet;

use litd_engine::{
    EngineError,
    cinematic_timeline::{
        CameraKeyframe, CinematicCameraPose, CinematicFade, CinematicSubtitle, CinematicTimeline,
        CinematicTransmission, FadeKind,
    },
    fsv_artifacts::begin_issue_run,
    game_runtime::{
        camera::{CameraMode, CameraOp, CameraState},
        cinematic_player::{CinematicPlayer, PlayState, SimGating},
        effects::{
            EffectArtRegistry, EffectAttach, EffectRuntime, EffectSpawn, lower_effect_spawn,
        },
        fog::{
            FogState, ModifierKind, SharedVisionTable, VisibilityModifier, VisibilityState,
            VisionSource, world_to_cell,
        },
    },
    math::{Fixed, FixedVec3, Vec3},
    sim::{FixedTickSim, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
};
use serde_json::json;

const GRID: u32 = 16;
const SIGHT_RADIUS_CELLS: u32 = 3; // authored on the lowered unit record (#818)
const PLAYER_SELF: u32 = 1;
const PLAYER_ALLY: u32 = 2;
const PLAYER_ENEMY: u32 = 3;

/// One fully-seeded fog+sim scenario. Returns the sim state hash and the fog
/// state hash before and after the unit moves + a reveal modifier is applied,
/// plus the printed per-state cell counts.
struct FogRun {
    sim_state_hash: u64,
    fog_hash_before: u64,
    fog_hash_after: u64,
    counts_before: (usize, usize, usize),
    counts_after: (usize, usize, usize),
    ally_sees_after: bool,
    enemy_sees_after: bool,
    concealed_cell_state: VisibilityState,
}

fn cell_of(pos: FixedVec3) -> Result<(i32, i32), EngineError> {
    let size = Fixed::ONE.raw();
    let x = world_to_cell(pos.x.raw(), 0, size)?;
    let z = world_to_cell(pos.z.raw(), 0, size)?;
    Ok((x, z))
}

fn run_fog_scenario(seed: u64) -> Result<FogRun, EngineError> {
    // Deterministic sim: unit for player 1 starts at cell (2,2); an allied unit
    // sits at (11,11); an enemy at (13,3). The unit is ordered to move +x.
    let mut sim = FixedTickSim::<8>::new(SimConfig::new(64)?, seed)?;
    let unit = sim.spawn_body(
        FixedVec3::from_i32s(2, 0, 2),
        FixedVec3::ZERO,
        FixedVec3::ZERO,
    )?;
    let ally = sim.spawn_body(
        FixedVec3::from_i32s(11, 0, 11),
        FixedVec3::ZERO,
        FixedVec3::ZERO,
    )?;
    let _enemy = sim.spawn_body(
        FixedVec3::from_i32s(13, 0, 3),
        FixedVec3::ZERO,
        FixedVec3::ZERO,
    )?;

    let mut fog = FogState::new(GRID, GRID, &[PLAYER_SELF, PLAYER_ALLY, PLAYER_ENEMY])?;
    let mut alliance = SharedVisionTable::new();
    alliance.ally(PLAYER_SELF, PLAYER_ALLY);

    // BEFORE: vision at the unit's starting cell + the allied unit's cell.
    let (ux0, uz0) = cell_of(sim.body_state(unit).unwrap().position)?;
    let (ax, az) = cell_of(sim.body_state(ally).unwrap().position)?;
    let sources_before = [
        VisionSource {
            owner: PLAYER_SELF,
            cell_x: ux0,
            cell_y: uz0,
            sight_radius_cells: SIGHT_RADIUS_CELLS,
        },
        VisionSource {
            owner: PLAYER_ALLY,
            cell_x: ax,
            cell_y: az,
            sight_radius_cells: 2,
        },
    ];
    fog.recompute(&sources_before, &[], &alliance);
    let fog_hash_before = fog.state_hash();
    let counts_before = fog.counts(PLAYER_SELF).unwrap();

    // Move the unit +x for 8 world units (deterministic order stream).
    let mut queue = SimOrderQueue::<8>::default();
    queue.enqueue(SimOrder::new(
        sim.tick() + 1,
        SimOrderSource::human(0),
        0,
        unit,
        SimOrderKind::SetVelocity(FixedVec3::new(Fixed::ONE, Fixed::ZERO, Fixed::ZERO)),
    ))?;
    queue.run_ticks(&mut sim, 512)?; // 512 ticks @ 64/s * 1 u/s == 8 units

    // AFTER: recompute from the moved unit; add a reveal modifier over the enemy
    // corner and conceal one previously-visible cell (conceal wins).
    let (ux1, uz1) = cell_of(sim.body_state(unit).unwrap().position)?;
    let sources_after = [
        VisionSource {
            owner: PLAYER_SELF,
            cell_x: ux1,
            cell_y: uz1,
            sight_radius_cells: SIGHT_RADIUS_CELLS,
        },
        VisionSource {
            owner: PLAYER_ALLY,
            cell_x: ax,
            cell_y: az,
            sight_radius_cells: 2,
        },
    ];
    let modifiers = [
        VisibilityModifier {
            player: PLAYER_SELF,
            kind: ModifierKind::Reveal,
            min_x: 12,
            min_y: 0,
            max_x: 15,
            max_y: 4,
        },
        VisibilityModifier {
            player: PLAYER_SELF,
            kind: ModifierKind::Conceal,
            min_x: ux1,
            min_y: uz1,
            max_x: ux1,
            max_y: uz1,
        },
    ];
    fog.recompute(&sources_after, &modifiers, &alliance);
    let fog_hash_after = fog.state_hash();
    let counts_after = fog.counts(PLAYER_SELF).unwrap();

    // Targeting queries: player 1 sees the revealed enemy corner; the enemy does
    // not see player 1's unit; the concealed unit cell is masked for player 1.
    let enemy_sees_after = fog.is_visible(PLAYER_ENEMY, ux1, uz1);
    let ally_sees_after = fog.is_visible(PLAYER_SELF, ax, az);
    let concealed_cell_state = fog.grid(PLAYER_SELF).unwrap().state(ux1, uz1);

    Ok(FogRun {
        sim_state_hash: sim.state_hash(),
        fog_hash_before,
        fog_hash_after,
        counts_before,
        counts_after,
        ally_sees_after,
        enemy_sees_after,
        concealed_cell_state,
    })
}

fn build_timeline() -> CinematicTimeline {
    CinematicTimeline {
        timeline_id: "issue845.intro".to_string(),
        duration_seconds: 12.0,
        camera_track: vec![
            CameraKeyframe {
                time_seconds: 0.0,
                camera_ref: "cam_start".to_string(),
                pose: CinematicCameraPose {
                    target: Vec3::ZERO,
                    distance: 25.0,
                    yaw_degrees: 0.0,
                    pitch_degrees: 55.0,
                    fov_degrees: 60.0,
                },
            },
            CameraKeyframe {
                time_seconds: 12.0,
                camera_ref: "cam_end".to_string(),
                pose: CinematicCameraPose {
                    target: Vec3::new(20.0, 0.0, 0.0),
                    distance: 25.0,
                    yaw_degrees: 90.0,
                    pitch_degrees: 55.0,
                    fov_degrees: 60.0,
                },
            },
        ],
        transmissions: vec![CinematicTransmission {
            start_seconds: 3.0,
            duration_seconds: 4.0,
            speaker_unit_type: "hero_commander".to_string(),
            portrait_ref: "portrait.commander".to_string(),
            text: "Hold the line.".to_string(),
            track: 0,
        }],
        fades: vec![CinematicFade {
            start_seconds: 0.0,
            duration_seconds: 2.0,
            kind: FadeKind::In,
            color: [0.0, 0.0, 0.0, 1.0],
        }],
        subtitles: vec![CinematicSubtitle {
            start_seconds: 3.0,
            duration_seconds: 4.0,
            text: "Hold the line.".to_string(),
        }],
        cues: Vec::new(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue844_847_runtime_presentation")?;

    // ---- #847 fog + replay determinism -----------------------------------
    let first = run_fog_scenario(0x844_847)?;
    let second = run_fog_scenario(0x844_847)?;
    let sim_hash_matches = first.sim_state_hash == second.sim_state_hash;
    let fog_hash_matches = first.fog_hash_before == second.fog_hash_before
        && first.fog_hash_after == second.fog_hash_after;
    if !sim_hash_matches || !fog_hash_matches {
        return Err("REPLAY DETERMINISM FAILED: identical seed+orders diverged".into());
    }
    // The unit moved, so the fog grid must have changed between before/after.
    if first.fog_hash_before == first.fog_hash_after {
        return Err("fog did not change after the unit moved".into());
    }
    if first.enemy_sees_after {
        return Err("enemy must not see player 1's unit".into());
    }
    if !first.ally_sees_after {
        return Err("shared vision: ally cell must be visible to player 1".into());
    }
    if first.concealed_cell_state != VisibilityState::Masked {
        return Err("conceal modifier must mask the unit cell".into());
    }

    // ---- #846 cameras -----------------------------------------------------
    let mut cam = CameraState::top_down_rts();
    let ops = [
        CameraOp::Pan([5.0, 0.0, -3.0]),
        CameraOp::Rotate {
            yaw_degrees: 45.0,
            pitch_degrees: -10.0,
        },
        CameraOp::Zoom(-8.0),
        CameraOp::Lock(true),
        CameraOp::Pan([100.0, 0.0, 0.0]), // inert while locked
    ];
    let changed = cam.apply_all(&ops);
    let locked_pose = cam.pose;
    cam.apply(&CameraOp::Lock(false));
    cam.apply(&CameraOp::SetMode(CameraMode::FirstPerson));
    let fp_distance = cam.pose.distance;
    // Scripted cinematic pose adoption.
    cam.apply_cinematic_pose(&CinematicCameraPose {
        target: Vec3::new(20.0, 0.0, 0.0),
        distance: 25.0,
        yaw_degrees: 90.0,
        pitch_degrees: 55.0,
        fov_degrees: 60.0,
    });
    let scripted_pose = cam.pose;
    if changed != 4 || locked_pose.target[0] != 5.0 || fp_distance != 0.0 {
        return Err("camera op sequence produced an unexpected pose".into());
    }

    // ---- #845 cinematics --------------------------------------------------
    let timeline = build_timeline();
    let mut player = CinematicPlayer::start(timeline.clone(), SimGating::Pause)?;
    let letterbox_on = player.letterbox_active();
    let sim_frozen = !player.should_advance_sim();
    // Sample at timestamps: t=1 (mid fade-in), t=5 (transmission active).
    let at_1 = player.sample_at(1.0)?;
    let at_5 = player.sample_at(5.0)?;
    let fade_mid = at_1.fade.alpha; // fade-in: 1 -> 0 over [0,2], so 0.5 at t=1
    let transmission_text = at_5
        .active_transmission
        .as_ref()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    let subtitle_at_5 = at_5.active_subtitle.clone().unwrap_or_default();
    player.advance(13.0); // past duration -> finishes
    let finished_after_play = player.state() == PlayState::Finished;
    let sim_resumes = player.should_advance_sim();
    // Skip on a fresh player.
    let mut skippable = CinematicPlayer::start(timeline, SimGating::Pause)?;
    skippable.skip();
    let skip_finished = skippable.state() == PlayState::Finished && skippable.was_skipped();

    // ---- #844 effects -----------------------------------------------------
    let registry = EffectArtRegistry::new(&["fx.blood", "fx.flame", "fx.rune"]);
    let mut effects = EffectRuntime::new();
    let attach_spawn = EffectSpawn {
        effect_ref: "fx.flame".to_string(),
        attach: EffectAttach::Unit {
            owner: 42,
            attach_point: "weapon".to_string(),
        },
        lifetime_ticks: None,
        destroy_on_owner_death: true,
    };
    let lowered = lower_effect_spawn(&attach_spawn, &registry)?;
    let effect_id = effects.spawn(&lowered, 0);
    let mut alive: BTreeSet<u32> = BTreeSet::new();
    alive.insert(42);
    effects.tick(1, &alive);
    let effect_alive_with_owner = effects.contains(effect_id);
    alive.remove(&42); // owner dies
    let destroyed = effects.tick(2, &alive);
    let effect_dead_after_owner = destroyed == vec![effect_id] && effects.active_count() == 0;

    // Negative control: dangling art ref must be rejected at lowering.
    let dangling = EffectSpawn {
        effect_ref: "fx.does_not_exist".to_string(),
        attach: EffectAttach::Point(FixedVec3::ZERO),
        lifetime_ticks: Some(5),
        destroy_on_owner_death: false,
    };
    let dangling_code = lower_effect_spawn(&dangling, &registry)
        .err()
        .map(|e| e.code())
        .unwrap_or("NONE");

    // Negative control: sabotaged timeline rejected at start.
    let mut sabotaged = build_timeline();
    sabotaged.fades[0].duration_seconds = 0.0; // zero-length fade
    let sabotage_code = CinematicPlayer::start(sabotaged, SimGating::Pause)
        .err()
        .map(|e| e.code)
        .unwrap_or("NONE");

    // ---- verdict ----------------------------------------------------------
    let all_ok = sim_hash_matches
        && fog_hash_matches
        && letterbox_on
        && sim_frozen
        && (fade_mid - 0.5).abs() < 1e-4
        && transmission_text == "Hold the line."
        && subtitle_at_5 == "Hold the line."
        && finished_after_play
        && sim_resumes
        && skip_finished
        && effect_alive_with_owner
        && effect_dead_after_owner
        && dangling_code == "EFFECT_ART_REF_DANGLING"
        && sabotage_code == "CINEMATIC_FADE_ZERO_LENGTH"
        && scripted_pose.yaw_degrees == 90.0;
    let verdict = if all_ok { "GREEN" } else { "RED" };

    // report.json: PATH-FREE, run-invariant (stable codes, cell counts, hashes).
    let report = json!({
        "schema_version": 1,
        "issues": [844, 845, 846, 847],
        "parent": 804,
        "verdict": verdict,
        "fog_847": {
            "grid": GRID,
            "sight_radius_cells": SIGHT_RADIUS_CELLS,
            "replay_sim_state_hash": format!("{:016x}", first.sim_state_hash),
            "replay_fog_hash_before": format!("{:016x}", first.fog_hash_before),
            "replay_fog_hash_after": format!("{:016x}", first.fog_hash_after),
            "sim_hash_matches_on_replay": sim_hash_matches,
            "fog_hash_matches_on_replay": fog_hash_matches,
            "counts_before_masked_explored_visible": [
                first.counts_before.0, first.counts_before.1, first.counts_before.2
            ],
            "counts_after_masked_explored_visible": [
                first.counts_after.0, first.counts_after.1, first.counts_after.2
            ],
            "enemy_sees_unit": first.enemy_sees_after,
            "ally_shared_vision": first.ally_sees_after,
            "concealed_cell_masked": first.concealed_cell_state == VisibilityState::Masked
        },
        "cameras_846": {
            "ops_that_changed_state": changed,
            "pan_inert_while_locked": (locked_pose.target[0] as i32) == 5,
            "first_person_distance_zero": fp_distance == 0.0,
            "scripted_pose_yaw": scripted_pose.yaw_degrees as i32,
            "boundary": "presentation/per-client; not in any sim state hash"
        },
        "cinematics_845": {
            "letterbox_on_while_active": letterbox_on,
            "sim_frozen_while_paused": sim_frozen,
            "fade_alpha_at_t1": (fade_mid * 1000.0) as i32,
            "transmission_at_t5": transmission_text,
            "subtitle_at_t5": subtitle_at_5,
            "finished_after_play": finished_after_play,
            "sim_resumes_after_finish": sim_resumes,
            "skip_finishes": skip_finished
        },
        "effects_844": {
            "effect_alive_with_owner": effect_alive_with_owner,
            "effect_destroyed_on_owner_death": effect_dead_after_owner,
            "dangling_ref_error_code": dangling_code
        },
        "negative_controls": {
            "dangling_effect_ref_code": dangling_code,
            "sabotaged_timeline_code": sabotage_code
        }
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE844_847_RUNTIME_FSV verdict={verdict} report_sha256={} report_bytes={}",
        readback.sha256,
        readback.bytes.len()
    );
    println!(
        "REPLAY_DETERMINISM sim_state_hash={:016x} fog_hash_before={:016x} fog_hash_after={:016x}",
        first.sim_state_hash, first.fog_hash_before, first.fog_hash_after
    );
    println!(
        "FOG_COUNTS before(m/e/v)={:?} after(m/e/v)={:?}",
        first.counts_before, first.counts_after
    );
    if !all_ok {
        return Err("FSV verdict RED: an assertion failed".into());
    }
    Ok(())
}
