//! FSV for the five WC3 runtime HUD systems (#839 quests, #840 leaderboards/
//! multiboards, #841 timers, #842 dialogs, #843 floating text; parent #804).
//!
//! One packaged fixture drives all five end-to-end with REAL data:
//! * Quest DEFINITIONS are authored as a `QuestBook`, persisted through the REAL
//!   Calyx content path and lowered to a frozen fingerprinted sim package, then
//!   LOADED into the runtime quest log by reading the lowered fields back. The full
//!   quest lifecycle (create/discover/complete) is printed.
//! * A leaderboard value updates on a resource change.
//! * A periodic timer fires an EXACT asserted count over a fixed tick span.
//! * A dialog button click enters the deterministic order stream; the SAME click at
//!   the SAME tick is replayed twice and the final `state_hash` is asserted equal.
//! * Floating text spawns and expires with tick-exact lifetimes.
//!
//! Negative controls: a sabotaged quest book (empty title) fails loud, and a timer
//! with a zero period is rejected with a typed error. The report is path-free and its
//! sha256 is recomputed independently via `certutil` and printed.

use std::error::Error;
use std::process::Command;

use litd_engine::calyx_content::{EngineCalyxContentStore, EngineContentMeasurementConfig};
use litd_engine::calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_engine::game_runtime::hud::boards::Leaderboard;
use litd_engine::game_runtime::hud::floating_text::FloatingTextSpawn;
use litd_engine::game_runtime::hud::quests::QuestProgress;
use litd_engine::game_runtime::hud::quests_authoring::{
    QuestAuthoring, QuestBook, load_quest_book_into_log, persist_and_lower_quest_book,
};
use litd_engine::game_runtime::hud::timers::TimerKind;
use litd_engine::game_runtime::{GameHudWorld, HudColor, HudOrder, HudOrderKind, PlayerMask};
use serde_json::{Value, json};

// Reuse the addon-absent learned-lens panel fixture (#1791): no ORT is present, so
// measurement degrades to pending_measurement while the explicit payload still
// persists + lowers. Shared with the world-environment FSV harnesses.
#[path = "world_environment_fsv_common/fixture.rs"]
mod fixture;

const PERIOD_TICKS: u64 = 5;
const SPAN_TICKS: u64 = 32;
const CLICK_TICK: u64 = 5;
const FLOAT_SPAWN_TICK: u64 = 2;
const FLOAT_LIFESPAN: u64 = 8;

fn quest_book() -> Result<QuestBook, Box<dyn Error>> {
    Ok(QuestBook::new(
        "litd-first-flame-quests",
        vec![
            QuestAuthoring {
                id: 1,
                title: "Kindle the First Flame".to_string(),
                description: "Reach the Ember Shrine and light it.".to_string(),
                icon: "ReplaceableTextures/CommandButtons/BTNFlame.blp".to_string(),
                required: true,
                required_flags: vec![10, 20],
            },
            QuestAuthoring {
                id: 2,
                title: "Gather the Lost".to_string(),
                description: "Rescue three stranded villagers.".to_string(),
                icon: "ReplaceableTextures/CommandButtons/BTNVillager.blp".to_string(),
                required: false,
                required_flags: vec![30],
            },
        ],
    )?)
}

/// Build the packaged HUD world, seeding leaderboard/timer/dialog/floating-text and
/// scheduling the dialog-click order. Returns the world so the caller can run + hash.
fn build_world() -> Result<(GameHudWorld, u32, u32), Box<dyn Error>> {
    let mut world = GameHudWorld::new();

    // #840 leaderboard: initial values, then a resource change bumps Red's gold.
    let mut leaderboard = Leaderboard::new("Gold Mined");
    leaderboard.set_row("Red", 100);
    leaderboard.set_row("Blue", 250);
    leaderboard.set_row("Red", 640); // resource change
    leaderboard.sort_descending();
    world.boards.set_leaderboard(leaderboard);

    // #841 periodic timer + window.
    let timer = world
        .timers
        .start_timer(TimerKind::Periodic, PERIOD_TICKS, 0)?;
    world.timers.create_window(timer, "Next Spawn")?;

    // #843 floating text (a damage number) spawned mid-run, tick-exact lifespan.
    world.floating.spawn(
        FloatingTextSpawn {
            text: "128".to_string(),
            position_mm: [4000, 0, 1000],
            velocity_mm_per_tick: [0, 0, 40],
            color: HudColor::new(255, 64, 64, 255),
            lifespan_ticks: FLOAT_LIFESPAN,
            fade_start_ticks: FLOAT_LIFESPAN / 2,
        },
        FLOAT_SPAWN_TICK,
    )?;

    // #842 dialog + buttons, shown to player 1, click scheduled on the order stream.
    let dialog = world.dialogs.create_dialog("Defend or retreat?")?;
    let defend = world.dialogs.add_button(dialog, "Defend", 0)?;
    let _retreat = world.dialogs.add_button(dialog, "Retreat", 0)?;
    world.dialogs.show(dialog, PlayerMask::single(1)?)?;
    world.enqueue_order(HudOrder::new(
        CLICK_TICK,
        0,
        HudOrderKind::DialogClick {
            dialog,
            button: defend,
            player: 1,
        },
    ))?;

    Ok((world, dialog, defend))
}

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue839_843_runtime_hud")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");

    // ---- #839 quests: author -> persist -> lower -> load into the runtime log ----
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent:issue839",
            fixture::unavailable_panel_state(),
        ),
    );
    let book = quest_book()?;
    let readback = persist_and_lower_quest_book(
        &store,
        "issue839-fsv",
        "editor://worlds/first-flame/quests",
        "First Flame Quest Book",
        &book,
    )?;

    let mut world = GameHudWorld::new();
    let loaded = load_quest_book_into_log(&readback.lowered, &mut world.quests)?;

    // Quest lifecycle, printed states: discover then complete quest 1 once both flags
    // are satisfied (a completion attempt with unmet flags is rejected mid-way).
    let mut lifecycle: Vec<Value> = Vec::new();
    world.quests.discover_quest(1, 1)?;
    lifecycle.push(json!({"stage": "discovered", "progress": progress_str(&world, 1)}));
    let premature = world.quests.try_complete(1, 2).is_err();
    world.quests.satisfy_flag(10);
    world.quests.satisfy_flag(20);
    world.quests.try_complete(1, 3)?;
    lifecycle.push(json!({"stage": "completed", "progress": progress_str(&world, 1)}));
    let quest_state = json!({
        "loaded_quests": loaded,
        "lowered_field_count": readback.lowered.fields.len(),
        "premature_completion_rejected": premature,
        "quest1_progress": progress_str(&world, 1),
        "quest1_completed_tick": world.quests.quest(1).and_then(|q| q.completed_tick),
        "quest2_required": world.quests.quest(2).map(|q| q.required),
        "notifications": world.quests.notifications().len(),
        "lifecycle": lifecycle,
    });

    // ---- #840/#841/#842/#843 driven together, twice, for replay determinism ----
    let (mut first, _d, _b) = build_world()?;
    let mut timer_fires = 0_usize;
    let mut float_active_at_9 = 0_usize;
    for tick in 1..=SPAN_TICKS {
        let report = first.step()?;
        timer_fires += report.timers_fired;
        if tick == FLOAT_SPAWN_TICK + FLOAT_LIFESPAN - 1 {
            float_active_at_9 = first.floating.active_count();
        }
    }
    let first_hash = first.state_hash();

    // Independent replay with the identical order stream.
    let (mut second, _d2, _b2) = build_world()?;
    second.run_ticks(SPAN_TICKS)?;
    let second_hash = second.state_hash();

    let expected_fires = (SPAN_TICKS / PERIOD_TICKS) as usize;
    let leaderboard_top = first
        .boards
        .leaderboard()
        .and_then(|lb| lb.rows.first())
        .map(|row| (row.label.clone(), row.value));
    let click = first.dialogs.last_click();

    // ---- negative controls ----
    let sabotaged_quest_rejected = QuestBook::new(
        "sabotaged",
        vec![QuestAuthoring {
            id: 1,
            title: "   ".to_string(), // empty title
            description: "d".to_string(),
            icon: "i".to_string(),
            required: false,
            required_flags: vec![],
        }],
    )
    .is_err();
    let zero_period_rejected = {
        let mut w = GameHudWorld::new();
        w.timers.start_timer(TimerKind::Periodic, 0, 0).is_err()
    };

    let acceptance = json!({
        "quests_loaded_from_lowered": loaded == 2,
        "quest1_completed": progress_str(&world, 1) == "Completed",
        "premature_completion_rejected": premature,
        "leaderboard_resource_change_applied": leaderboard_top == Some(("Red".to_string(), 640)),
        "timer_fired_exact_count": timer_fires == expected_fires,
        "floating_text_alive_before_expiry": float_active_at_9 == 1,
        "floating_text_expired_after_lifespan": first.floating.active_count() == 0,
        "dialog_click_resolved": click.is_some_and(|c| c.tick == CLICK_TICK && c.player == 1),
        "dialog_dismissed_after_click": !first.dialogs.is_shown_to(_d, 1),
        "replay_state_hash_matches": first_hash == second_hash,
        "sabotaged_quest_rejected": sabotaged_quest_rejected,
        "zero_period_timer_rejected": zero_period_rejected,
    });

    let report = json!({
        "schema": "litd.engine.issue839_843.runtime_hud_fsv.v1",
        "issues": [839, 840, 841, 842, 843],
        "parent": 804,
        "source_of_truth": {
            "manual_gate": "Read this report + the persisted Aster quest-book row; command success alone is not proof.",
            "determinism": "All five systems advance from GameHudWorld ticks + the HUD order stream; no wall clock. Identical order streams reproduce state_hash."
        },
        "sufficiency_argument": "One quest book (authored, persisted, lowered, loaded), one leaderboard resource change, one periodic timer over a fixed tick span, one order-stream dialog click, and one tick-lifetime floating text jointly exercise every system's create/update/complete/expire path plus per-player visibility. A twice-run identical stream proves lockstep determinism; two negative controls prove sabotaged data and a zero period fail loud.",
        "quests": quest_state,
        "quest_book_persist": readback.evidence(),
        "leaderboard": { "top_row": leaderboard_top },
        "timers": { "period_ticks": PERIOD_TICKS, "span_ticks": SPAN_TICKS, "fires": timer_fires, "expected_fires": expected_fires },
        "floating_text": { "spawn_tick": FLOAT_SPAWN_TICK, "lifespan_ticks": FLOAT_LIFESPAN, "active_before_expiry": float_active_at_9, "active_after_span": first.floating.active_count() },
        "dialog": { "click_tick": click.map(|c| c.tick), "click_player": click.map(|c| c.player), "click_button": click.map(|c| c.button) },
        "replay": { "first_state_hash": format!("{first_hash:016x}"), "second_state_hash": format!("{second_hash:016x}"), "match": first_hash == second_hash },
        "acceptance": acceptance,
    });

    let written = run.write_report(&report)?;
    for (field, value) in written.json["acceptance"].as_object().expect("acceptance") {
        if value != true {
            return Err(format!("issue839_843 acceptance {field} was not true").into());
        }
    }

    let certutil_sha = certutil_sha256(&written.path.display().to_string());
    println!(
        "ISSUE839_843_RUNTIME_HUD_FSV verdict=GREEN report_sha256={} certutil_sha256={} sha_match={} replay_state_hash={:016x} timer_fires={}/{}",
        written.sha256,
        certutil_sha,
        certutil_sha.eq_ignore_ascii_case(&written.sha256),
        first_hash,
        timer_fires,
        expected_fires,
    );
    Ok(())
}

fn progress_str(world: &GameHudWorld, id: u32) -> &'static str {
    match world.quests.quest(id).map(|q| q.progress) {
        Some(QuestProgress::Active) => "Active",
        Some(QuestProgress::Completed) => "Completed",
        Some(QuestProgress::Failed) => "Failed",
        None => "Missing",
    }
}

/// Recompute the report sha256 independently with the platform `certutil` so the
/// hash is not self-reported by the harness alone.
fn certutil_sha256(path: &str) -> String {
    let output = match Command::new("certutil")
        .args(["-hashfile", path, "SHA256"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return "certutil-unavailable".to_string(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(str::trim)
        .find(|line| line.len() == 64 && line.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or("certutil-parse-failed")
        .to_string()
}
