//! #1788 phase-1 skirmish runner: drive one SP-vs-AI melee to a terminal result and hash it.

use std::path::Path;
use std::process::Command;

use litd_editor::history::EditorDocument;
use litd_engine::game_runtime::{
    MatchRuntimeSession, PlayerResult, RuntimeDatasetsManifest, SessionOrder,
};

const MATCH_TICKS: usize = 90;
const SCRIPTED_STRIKE_INDEX: usize = 6;

pub(crate) fn skirmish_variant(base: &EditorDocument, remove_profile: &str) -> EditorDocument {
    let mut doc = base.clone();
    let _ = doc.remove_object(remove_profile);
    doc
}

pub(crate) struct MatchOutcome {
    pub(crate) active_display: String,
    pub(crate) victory_tick: Option<u64>,
    pub(crate) result_active: String,
    pub(crate) result_enemy: String,
    pub(crate) final_hash: u64,
    pub(crate) highlights: Vec<String>,
}

/// Run one SP-vs-AI skirmish to a terminal result. `active_slot` is the dense slot the authored
/// AI drives; `enemy_slot` is the passive objective. One scripted strike on an active-owned
/// structure exercises the authored retreat rule (mirrors the #1786 idiom).
pub(crate) fn run_skirmish(
    manifest: &RuntimeDatasetsManifest,
    active_slot: u8,
    enemy_slot: u8,
) -> Result<MatchOutcome, Box<dyn std::error::Error>> {
    let blueprint = manifest.lowered_match.verify()?;
    let own_struct = blueprint
        .units
        .iter()
        .find(|u| u.is_structure && u.owner == active_slot)
        .map(|u| u.unit)
        .ok_or("no active-owned structure for the retreat strike")?;

    let mut stream = Vec::with_capacity(MATCH_TICKS + 1);
    for i in 0..MATCH_TICKS {
        if i == SCRIPTED_STRIKE_INDEX {
            stream.push(SessionOrder::DestroyUnit { unit: own_struct });
        }
        stream.push(SessionOrder::Tick);
    }

    let mut session = MatchRuntimeSession::from_manifest(manifest)?;
    let mut victory_tick = None;
    for order in &stream {
        session.apply(order.clone())?;
        if victory_tick.is_none()
            && session.match_state().result(active_slot)? == PlayerResult::Victory
        {
            victory_tick = Some(session.applied_tick());
        }
    }
    let highlights: Vec<String> = session
        .ai()
        .timeline()
        .iter()
        .map(|e| format!("t{:>3} p{} {:<14} {}", e.tick, e.player, e.kind, e.detail))
        .collect();
    let active_display = manifest
        .ai_profiles
        .first()
        .map(|p| p.display_name.clone())
        .unwrap_or_default();
    Ok(MatchOutcome {
        active_display,
        victory_tick,
        result_active: format!("{:?}", session.match_state().result(active_slot)?),
        result_enemy: format!("{:?}", session.match_state().result(enemy_slot)?),
        final_hash: session.combined_state_hash(),
        highlights,
    })
}

pub(crate) fn replay_hash(
    manifest: &RuntimeDatasetsManifest,
    active_slot: u8,
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut session = MatchRuntimeSession::from_manifest(manifest)?;
    let mut stream = Vec::with_capacity(MATCH_TICKS + 1);
    let blueprint = manifest.lowered_match.verify()?;
    let own_struct = blueprint
        .units
        .iter()
        .find(|u| u.is_structure && u.owner == active_slot)
        .map(|u| u.unit)
        .ok_or("no active structure")?;
    for i in 0..MATCH_TICKS {
        if i == SCRIPTED_STRIKE_INDEX {
            stream.push(SessionOrder::DestroyUnit { unit: own_struct });
        }
        stream.push(SessionOrder::Tick);
    }
    for order in stream {
        session.apply(order)?;
    }
    Ok(session.combined_state_hash())
}

pub(crate) fn certutil_sha256(path: &Path) -> Option<String> {
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
