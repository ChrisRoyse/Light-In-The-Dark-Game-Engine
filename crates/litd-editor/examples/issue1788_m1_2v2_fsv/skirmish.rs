//! #1788 2v2 skirmish runner + vault mesh pick + report hash helper.

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::{AssetVaultCatalog, AssetVaultRecord};
use litd_editor::history::{EditorDocument, EditorObject};
use litd_engine::game_runtime::{
    AllianceState, MatchRuntimeSession, PlayerResult, RuntimeDatasetsManifest, SessionOrder,
};
use litd_engine::math::Transform;

pub const MATCH_TICKS: usize = 120;
const SCRIPTED_STRIKE_INDEX: usize = 6;

pub fn is_mesh(record: &AssetVaultRecord) -> bool {
    let path = record.relative_path.to_ascii_lowercase();
    path.ends_with(".gltf") || path.ends_with(".glb") || path.ends_with(".mdl")
}

pub fn mesh_binds(record: &AssetVaultRecord) -> bool {
    let Ok(mut doc) = EditorDocument::new("calyx://world/issue1788-2v2-mesh", "probe-rev-1") else {
        return false;
    };
    if doc
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .is_err()
    {
        return false;
    }
    if doc
        .upsert_object(EditorObject::new(
            "probe",
            "Unit",
            Some("world".to_string()),
            Transform::IDENTITY,
        ))
        .is_err()
    {
        return false;
    }
    assign_vault_model_to_unit(&mut doc, "probe", record).is_ok()
}

pub fn pick_mesh(
    catalog: &AssetVaultCatalog,
    prefer: &[&str],
) -> Result<(AssetVaultRecord, bool), String> {
    for token in prefer {
        if let Some(record) = catalog
            .records()
            .iter()
            .find(|r| is_mesh(r) && r.relative_path.contains(token) && mesh_binds(r))
        {
            return Ok((record.clone(), true));
        }
    }
    catalog
        .records()
        .iter()
        .find(|r| is_mesh(r) && mesh_binds(r))
        .cloned()
        .map(|r| (r, false))
        .ok_or_else(|| "no bindable mesh record in vault".to_string())
}

pub fn skirmish_variant(base: &EditorDocument, remove_profile: &str) -> EditorDocument {
    let mut doc = base.clone();
    let _ = doc.remove_object(remove_profile);
    doc
}

pub struct TeamOutcome {
    pub active_display: String,
    pub victory_tick: Option<u64>,
    pub force0_results: Vec<String>,
    pub force1_results: Vec<String>,
    pub final_hash: u64,
    pub enemy_struct_count: usize,
    pub ally_not_in_enemy_set: bool,
    pub highlights: Vec<String>,
}

/// Run one team-AI 2v2: `active_slots` are the force that should win; enemy force loses.
pub fn run_team_skirmish(
    manifest: &RuntimeDatasetsManifest,
    active_slots: &[u8],
    enemy_slots: &[u8],
) -> Result<TeamOutcome, Box<dyn std::error::Error>> {
    let blueprint = manifest.lowered_match.verify()?;
    let roster_preview = {
        let session = MatchRuntimeSession::from_manifest(manifest)?;
        let roster = session.match_state().roster();
        for pair in active_slots.windows(2) {
            let cell = roster.alliance(pair[0], pair[1])?;
            if cell.state != AllianceState::Ally || !cell.shared_vision {
                return Err(format!(
                    "active force pair {:?}/{:?} not allied: {:?}",
                    pair[0], pair[1], cell
                )
                .into());
            }
        }
        for a in active_slots {
            for e in enemy_slots {
                if !roster.are_enemies(*a, *e)? {
                    return Err(format!("slots {a}/{e} not enemies").into());
                }
            }
        }
        let active_force = roster.slot(active_slots[0])?.force;
        let enemy_structs: Vec<_> = blueprint
            .units
            .iter()
            .filter(|u| {
                u.is_structure
                    && roster
                        .slot(u.owner)
                        .map(|s| s.force != active_force)
                        .unwrap_or(false)
            })
            .collect();
        let ally_structs_wrongly = blueprint.units.iter().any(|u| {
            u.is_structure
                && active_slots.contains(&u.owner)
                && enemy_structs.iter().any(|e| e.unit == u.unit)
        });
        (enemy_structs.len(), !ally_structs_wrongly)
    };
    let (enemy_struct_count, ally_not_in_enemy_set) = roster_preview;

    let active_struct = blueprint
        .units
        .iter()
        .find(|u| u.is_structure && u.owner == active_slots[0])
        .map(|u| u.unit)
        .ok_or("no active-owned structure for retreat strike")?;

    let mut stream = Vec::with_capacity(MATCH_TICKS + 1);
    for i in 0..MATCH_TICKS {
        if i == SCRIPTED_STRIKE_INDEX {
            stream.push(SessionOrder::DestroyUnit {
                unit: active_struct,
            });
        }
        stream.push(SessionOrder::Tick);
    }

    let mut session = MatchRuntimeSession::from_manifest(manifest)?;
    let mut victory_tick = None;
    for order in &stream {
        session.apply(order.clone())?;
        if victory_tick.is_none() {
            let all_active_win = active_slots.iter().all(|s| {
                session
                    .match_state()
                    .result(*s)
                    .map(|r| r == PlayerResult::Victory)
                    .unwrap_or(false)
            });
            if all_active_win {
                victory_tick = Some(session.applied_tick());
            }
        }
    }
    let force0_results: Vec<String> = active_slots
        .iter()
        .map(|s| {
            format!(
                "{:?}",
                session
                    .match_state()
                    .result(*s)
                    .unwrap_or(PlayerResult::Playing)
            )
        })
        .collect();
    let force1_results: Vec<String> = enemy_slots
        .iter()
        .map(|s| {
            format!(
                "{:?}",
                session
                    .match_state()
                    .result(*s)
                    .unwrap_or(PlayerResult::Playing)
            )
        })
        .collect();
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
    Ok(TeamOutcome {
        active_display,
        victory_tick,
        force0_results,
        force1_results,
        final_hash: session.combined_state_hash(),
        enemy_struct_count,
        ally_not_in_enemy_set,
        highlights,
    })
}

pub fn replay_hash(
    manifest: &RuntimeDatasetsManifest,
    active_slot: u8,
) -> Result<u64, Box<dyn std::error::Error>> {
    let blueprint = manifest.lowered_match.verify()?;
    let own_struct = blueprint
        .units
        .iter()
        .find(|u| u.is_structure && u.owner == active_slot)
        .map(|u| u.unit)
        .ok_or("no active structure")?;
    let mut stream = Vec::with_capacity(MATCH_TICKS + 1);
    for i in 0..MATCH_TICKS {
        if i == SCRIPTED_STRIKE_INDEX {
            stream.push(SessionOrder::DestroyUnit { unit: own_struct });
        }
        stream.push(SessionOrder::Tick);
    }
    let mut session = MatchRuntimeSession::from_manifest(manifest)?;
    for order in stream {
        session.apply(order)?;
    }
    Ok(session.combined_state_hash())
}

pub fn certutil_sha256(path: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("certutil")
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
