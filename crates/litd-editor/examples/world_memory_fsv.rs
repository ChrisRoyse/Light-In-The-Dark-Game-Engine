use std::{collections::BTreeMap, fs, path::PathBuf};

use litd_editor::{
    authoring::{
        AssayBalanceProfile, GroundedAbilityRequest, GroundingAnchor, LoweredAbilityDocument,
        WardPolicy, WorldCorpus, generate_grounded_ability, verify_lowered_ability_toml,
    },
    history::EditorObject,
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties, EditorWorldRecord,
        create_editor_world_record, parse_world_record, read_world_record, write_world_record,
    },
    world_memory::{
        AuthoredWorldMemoryInput, WorldMemoryReuseRequest, build_world_memory_index,
        generated_world_script, query_world_memory, reuse_ability_from_memory,
    },
};
use litd_engine::math::Transform;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct FsvSnapshot {
    issue: u32,
    source_of_truth: String,
    before: BTreeMap<String, String>,
    aster_readback: Vec<BTreeMap<String, String>>,
    happy_path: BTreeMap<String, String>,
    edges: Vec<EdgeState>,
    after: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct EdgeState {
    edge: String,
    before: BTreeMap<String, String>,
    after: BTreeMap<String, String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_WORLD_MEMORY_FSV",
        "issue1231_world_memory",
        "manual-fsv.json",
    )?;
    let root = output_path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1231 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;

    let poison_world = world_record("wa-poison-fsv", "Venom Vale FSV");
    let frost_world = world_record("wa-frost-fsv", "Frost Vale FSV");
    let poison_aster = write_and_read_world(&root, "poison", &poison_world)?;
    let frost_aster = write_and_read_world(&root, "frost", &frost_world)?;
    let poison_readback = parse_world_record(&poison_aster.bytes)?;
    let frost_readback = parse_world_record(&frost_aster.bytes)?;

    let inputs = vec![
        memory_input(
            poison_readback,
            poison_ability("req-poison-fsv", "wa-poison-fsv"),
        )?,
        memory_input(
            frost_readback,
            frost_ability("req-frost-fsv", "wa-frost-fsv"),
        )?,
    ];
    let index = build_world_memory_index(&inputs)?;
    let hits = query_world_memory(&index, "poison stacking ability", 3)?;
    let top = hits.first().expect("world-memory hit");
    let reuse = reuse_ability_from_memory(
        &index,
        &WorldMemoryReuseRequest {
            source_constellation_id: top.constellation_id.clone(),
            request_id: "req-world-memory-reuse".to_string(),
            seed: 123100,
            requested_by: "issue1231-fsv".to_string(),
        },
        &poison_corpus("wa-target-fsv"),
        &poison_assay(),
        &ward(),
    )?;
    let lowered = verify_lowered_ability_toml(&reuse.canonical_toml, &reuse.lowered_fingerprint)?;

    let mut snapshot = FsvSnapshot {
        issue: 1231,
        source_of_truth: output_path.display().to_string(),
        before: map([
            ("world_count", "2".to_string()),
            ("query", "poison stacking ability".to_string()),
        ]),
        aster_readback: vec![
            readback_map("poison", &poison_aster, &poison_world),
            readback_map("frost", &frost_aster, &frost_world),
        ],
        happy_path: map([
            ("constellation_count", index.constellation_count.to_string()),
            ("top_kind", format!("{:?}", top.source_kind)),
            ("top_source_id", top.source_id.clone()),
            ("top_score", top.score.to_string()),
            (
                "top_provenance_count",
                top.provenance_refs.len().to_string(),
            ),
            ("reuse_target_world", reuse.target_world_id.clone()),
            ("reuse_fingerprint", reuse.lowered_fingerprint.clone()),
            ("reuse_ability_id", lowered.ability.id.clone()),
        ]),
        edges: vec![
            conflict_edge()?,
            empty_catalog_edge(),
            ood_query_edge(&index),
        ],
        after: map([("exists", "false".to_string()), ("bytes", "0".to_string())]),
    };
    write_stable_snapshot(&output_path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote world-memory FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    Ok(())
}

fn write_and_read_world(
    root: &std::path::Path,
    _name: &str,
    record: &EditorWorldRecord,
) -> Result<litd_editor::world_creation::WorldAsterReadback, Box<dyn std::error::Error>> {
    let aster_dir = root.join("aster");
    write_world_record(&aster_dir, record)?;
    read_world_record(&aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id).into())
}

fn memory_input(
    world: EditorWorldRecord,
    ability: LoweredAbilityDocument,
) -> Result<AuthoredWorldMemoryInput, Box<dyn std::error::Error>> {
    let script = generated_world_script(&world)?;
    Ok(AuthoredWorldMemoryInput {
        world,
        abilities: vec![ability],
        objects: vec![
            unit_object(
                "unit_venom_archer",
                "Venom Archer",
                "poison stacking ranged unit",
            ),
            item_object("item_antidote", "Antidote", "cleanses poison stacks"),
        ],
        scripts: vec![script],
    })
}

fn conflict_edge() -> Result<EdgeState, Box<dyn std::error::Error>> {
    let first = memory_input(
        world_record("wa-conflict-a-fsv", "Venom Conflict A"),
        poison_ability("req-conflict-a-fsv", "wa-conflict-a-fsv"),
    )?;
    let second = memory_input(
        world_record("wa-conflict-b-fsv", "Venom Conflict B"),
        poison_ability("req-conflict-b-fsv", "wa-conflict-b-fsv"),
    )?;
    let index = build_world_memory_index(&[first, second])?;
    Ok(EdgeState {
        edge: "anchor_conflict".to_string(),
        before: map([("worlds", "2".to_string())]),
        after: map([
            ("conflicts", index.conflicts.len().to_string()),
            ("kind", index.conflicts[0].conflict_kind.clone()),
            ("reason", index.conflicts[0].reason.clone()),
        ]),
    })
}

fn empty_catalog_edge() -> EdgeState {
    EdgeState {
        edge: "empty_catalog".to_string(),
        before: map([("worlds", "0".to_string())]),
        after: map([(
            "error",
            build_world_memory_index(&[]).unwrap_err().to_string(),
        )]),
    }
}

fn ood_query_edge(index: &litd_editor::world_memory::WorldMemoryIndex) -> EdgeState {
    EdgeState {
        edge: "ood_query".to_string(),
        before: map([("query", "spaceship stockmarket laser".to_string())]),
        after: map([(
            "error",
            query_world_memory(index, "spaceship stockmarket laser", 3)
                .unwrap_err()
                .to_string(),
        )]),
    }
}

fn poison_ability(request_id: &str, world_id: &str) -> LoweredAbilityDocument {
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: request_id.to_string(),
            prompt: "Create a Venom Circle poison stacking venom dose ability for Mire Reach toxin grove."
                .to_string(),
            seed: 1231,
            requested_by: "issue1231-fsv".to_string(),
        },
        &poison_corpus(world_id),
        &poison_assay(),
        &ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn frost_ability(request_id: &str, world_id: &str) -> LoweredAbilityDocument {
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: request_id.to_string(),
            prompt:
                "Create a Snow Court frost veil moon shard ability for Frost Reach glacier hollow."
                    .to_string(),
            seed: 2231,
            requested_by: "issue1231-fsv".to_string(),
        },
        &frost_corpus(world_id),
        &AssayBalanceProfile {
            allowed_damage_types: vec!["frost veil".to_string(), "moon shard".to_string()],
            ..poison_assay()
        },
        &ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn world_record(world_id: &str, name: &str) -> EditorWorldRecord {
    let properties = EditorWorldProperties {
        display_name: name.to_string(),
        description: format!("{name} authored-world memory FSV"),
        width_tiles: 8,
        height_tiles: 8,
        playable_bounds: EditorPlayableBounds {
            min_tile_x: 0,
            min_tile_z: 0,
            max_tile_x_exclusive: 8,
            max_tile_z_exclusive: 8,
        },
        ..Default::default()
    };
    create_editor_world_record(EditorWorldCreationSpec {
        calyx_document_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        properties,
    })
    .unwrap()
}

fn unit_object(id: &str, name: &str, role: &str) -> EditorObject {
    EditorObject::new(id, "Unit", None, Transform::IDENTITY)
        .with_property("name", name)
        .with_property("unit_type", "unit.venom_archer")
        .with_property("stats", "hp 420 damage 18 poison_stacks 3")
        .with_property("role", role)
        .with_property("source_ref", "Aster://wa-poison/units/venom-archer")
}

fn item_object(id: &str, name: &str, description: &str) -> EditorObject {
    EditorObject::new(id, "Item", None, Transform::IDENTITY)
        .with_property("name", name)
        .with_property("item_type", "item.antidote")
        .with_property("description", description)
        .with_property("source_ref", "Aster://wa-poison/items/antidote")
}

fn poison_corpus(world_id: &str) -> WorldCorpus {
    WorldCorpus {
        world_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        region: "Mire Reach".to_string(),
        factions: vec!["Venom Circle".to_string()],
        lore_terms: vec![
            "poison stacking".to_string(),
            "venom dose".to_string(),
            "toxin grove".to_string(),
        ],
        anchors: vec![
            anchor(
                "poison-region",
                &format!("Aster://{world_id}/world/mire#region"),
                "Mire Reach is a toxin grove.",
                &["Mire Reach", "toxin grove"],
            ),
            anchor(
                "poison-faction",
                &format!("Aster://{world_id}/factions/venom-circle"),
                "Venom Circle tracks poison stacking rites.",
                &["Venom Circle", "poison stacking"],
            ),
            anchor(
                "poison-reagent",
                &format!("Aster://{world_id}/reagents/venom-dose"),
                "Venom dose adds poison stacks.",
                &["venom dose"],
            ),
        ],
    }
}

fn frost_corpus(world_id: &str) -> WorldCorpus {
    WorldCorpus {
        world_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        region: "Frost Reach".to_string(),
        factions: vec!["Snow Court".to_string()],
        lore_terms: vec![
            "frost veil".to_string(),
            "moon shard".to_string(),
            "glacier hollow".to_string(),
        ],
        anchors: vec![
            anchor(
                "frost-region",
                &format!("Aster://{world_id}/world/frost#region"),
                "Frost Reach is a glacier hollow.",
                &["Frost Reach", "glacier hollow"],
            ),
            anchor(
                "frost-faction",
                &format!("Aster://{world_id}/factions/snow-court"),
                "Snow Court shapes a frost veil.",
                &["Snow Court", "frost veil"],
            ),
            anchor(
                "frost-reagent",
                &format!("Aster://{world_id}/reagents/moon-shard"),
                "Moon shard bends cold light.",
                &["moon shard"],
            ),
        ],
    }
}

fn poison_assay() -> AssayBalanceProfile {
    AssayBalanceProfile {
        assay_id: "assay-1231".to_string(),
        profile_id: "ability-tier-1-control".to_string(),
        min_damage: 18,
        max_damage: 54,
        min_cooldown_seconds: 6,
        max_cooldown_seconds: 16,
        max_mana_cost: 70,
        allowed_damage_types: vec!["poison stacking".to_string(), "venom dose".to_string()],
    }
}

fn ward() -> WardPolicy {
    WardPolicy {
        policy_id: "ward-grounding-issue1231".to_string(),
        min_source_anchors: 3,
        min_grounded_terms: 4,
        blocked_terms: vec![
            "spaceship".to_string(),
            "laser".to_string(),
            "stock market".to_string(),
        ],
    }
}

fn anchor(id: &str, source: &str, text: &str, terms: &[&str]) -> GroundingAnchor {
    GroundingAnchor {
        anchor_id: id.to_string(),
        source_ref: source.to_string(),
        text: text.to_string(),
        terms: terms.iter().map(|term| term.to_string()).collect(),
    }
}

fn readback_map(
    name: &str,
    readback: &litd_editor::world_creation::WorldAsterReadback,
    original: &EditorWorldRecord,
) -> BTreeMap<String, String> {
    map([
        ("name", name.to_string()),
        ("bytes", readback.bytes.len().to_string()),
        ("seq", readback.seq.to_string()),
        ("graph_rows", readback.graph_rows.to_string()),
        ("document_id", original.spec.calyx_document_id.clone()),
        ("fingerprint", original.document_fingerprint.clone()),
    ])
}

fn map<const N: usize>(items: [(&str, String); N]) -> BTreeMap<String, String> {
    items
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn write_stable_snapshot(
    path: &PathBuf,
    snapshot: &mut FsvSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    for _ in 0..8 {
        fs::write(path, serde_json::to_string_pretty(snapshot)?)?;
        let metadata = fs::metadata(path)?;
        let next = map([
            ("exists", metadata.is_file().to_string()),
            ("bytes", metadata.len().to_string()),
        ]);
        if snapshot.after == next {
            return Ok(());
        }
        snapshot.after = next;
    }
    Err(format!(
        "FSV snapshot metadata did not stabilize for {}",
        path.display()
    )
    .into())
}
