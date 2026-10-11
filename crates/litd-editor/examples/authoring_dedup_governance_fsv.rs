use std::{collections::BTreeMap, fs, path::PathBuf};

use litd_editor::{
    authoring::{
        AssayBalanceProfile, GroundedAbilityRequest, GroundingAnchor, LoweredAbilityDocument,
        WardPolicy, WorldCorpus, generate_grounded_ability, verify_lowered_ability_toml,
    },
    authoring_dedup_governance::{
        CatalogDedupRecord, DedupPolicy, DedupSlotKind, build_catalog_dedup_report,
    },
    history::stable_fingerprint,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct FsvSnapshot {
    issue: u32,
    source_of_truth: String,
    before: BeforeState,
    happy_path: CaseState,
    edges: Vec<EdgeState>,
    after: AfterState,
}

#[derive(Debug, Serialize)]
struct BeforeState {
    policy_tau_milli: u16,
    happy_records: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CaseState {
    candidate_count: usize,
    top_decision: String,
    top_min_score: u16,
    top_suggestion: String,
}

#[derive(Debug, Serialize)]
struct EdgeState {
    edge: String,
    before: BTreeMap<String, String>,
    after: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct AfterState {
    exists: bool,
    bytes: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, output_path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_AUTHORING_DEDUP_FSV",
        "issue1232_authoring_dedup",
        "manual-fsv.json",
    )?;
    let policy = DedupPolicy::default();
    let first = lowered_ability("req-a", 424242, "wa1-ashen-reach");
    let near = lowered_ability("req-b", 424243, "wa1-ashen-reach");
    let distinct = lowered_frost_ability();
    let happy_records = vec![
        CatalogDedupRecord::from_lowered_ability(&first)?,
        CatalogDedupRecord::from_lowered_ability(&near)?,
        CatalogDedupRecord::from_lowered_ability(&distinct)?,
    ];
    let happy = build_catalog_dedup_report(&happy_records, &policy)?;
    let happy_top = happy.candidates.first().expect("happy path candidate");

    let exact_edge = exact_edge(&first, &policy)?;
    let boundary_edge = boundary_edge()?;
    let cross_world_edge = cross_world_edge(&policy)?;
    let invalid_edge = invalid_edge();

    let mut snapshot = FsvSnapshot {
        issue: 1232,
        source_of_truth: output_path.display().to_string(),
        before: BeforeState {
            policy_tau_milli: policy.tau_milli,
            happy_records: happy_records
                .iter()
                .map(|record| record.record_id.clone())
                .collect(),
        },
        happy_path: CaseState {
            candidate_count: happy.candidate_count,
            top_decision: format!("{:?}", happy_top.decision),
            top_min_score: happy_top.minimum_score_milli,
            top_suggestion: happy_top.merge_suggestion.clone(),
        },
        edges: vec![exact_edge, boundary_edge, cross_world_edge, invalid_edge],
        after: AfterState {
            exists: false,
            bytes: 0,
        },
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_stable_snapshot(&output_path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&output_path)?;
    println!(
        "wrote authoring dedup governance FSV to {} sha256={}",
        output_path.display(),
        report_readback.sha256
    );
    Ok(())
}

fn exact_edge(
    ability: &LoweredAbilityDocument,
    policy: &DedupPolicy,
) -> Result<EdgeState, Box<dyn std::error::Error>> {
    let left = CatalogDedupRecord::from_lowered_ability(ability)?;
    let mut right = left.clone();
    right.record_id = format!("{}#copy", right.record_id);
    let report = build_catalog_dedup_report(&[left.clone(), right], policy)?;
    let candidate = &report.candidates[0];
    Ok(EdgeState {
        edge: "exact_duplicate".to_string(),
        before: map([
            ("left", left.record_id),
            ("fingerprint", left.stable_fingerprint),
        ]),
        after: map([
            ("decision", format!("{:?}", candidate.decision)),
            ("exact_fingerprint", candidate.exact_fingerprint.to_string()),
            (
                "identity_score",
                candidate.slot_scores[&DedupSlotKind::Identity].to_string(),
            ),
        ]),
    })
}

fn boundary_edge() -> Result<EdgeState, Box<dyn std::error::Error>> {
    let left = boundary_item("item.boundary.left", "alpha beta gamma")?;
    let right = boundary_item("item.boundary.right", "alpha beta delta")?;
    let policy = DedupPolicy {
        tau_milli: 667,
        ..DedupPolicy::default()
    };
    let report = build_catalog_dedup_report(&[left, right], &policy)?;
    let candidate = &report.candidates[0];
    Ok(EdgeState {
        edge: "threshold_boundary".to_string(),
        before: map([("tau", "667".to_string())]),
        after: map([
            ("decision", format!("{:?}", candidate.decision)),
            ("minimum_score", candidate.minimum_score_milli.to_string()),
        ]),
    })
}

fn cross_world_edge(policy: &DedupPolicy) -> Result<EdgeState, Box<dyn std::error::Error>> {
    let first = lowered_ability("req-cross-a", 424242, "wa1-ashen-reach");
    let second = lowered_ability("req-cross-b", 424242, "wa2-ashen-reach");
    let records = vec![
        CatalogDedupRecord::from_lowered_ability(&first)?,
        CatalogDedupRecord::from_lowered_ability(&second)?,
    ];
    let report = build_catalog_dedup_report(&records, policy)?;
    let candidate = &report.candidates[0];
    Ok(EdgeState {
        edge: "cross_world_duplicate".to_string(),
        before: map([
            ("left_world", records[0].world_id.clone()),
            ("right_world", records[1].world_id.clone()),
        ]),
        after: map([
            ("decision", format!("{:?}", candidate.decision)),
            ("conflicts", candidate.anchor_conflicts.join("|")),
            ("suggestion", candidate.merge_suggestion.clone()),
        ]),
    })
}

fn invalid_edge() -> EdgeState {
    let error = build_catalog_dedup_report(&[], &DedupPolicy::default())
        .unwrap_err()
        .to_string();
    EdgeState {
        edge: "invalid_empty_catalog".to_string(),
        before: map([("record_count", "0".to_string())]),
        after: map([("error", error)]),
    }
}

fn lowered_ability(request_id: &str, seed: u64, world_id: &str) -> LoweredAbilityDocument {
    let corpus = corpus(world_id);
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: request_id.to_string(),
            prompt:
                "Create a Dawnward cinder bloom ward flame ability for Ashen Reach dusk forest."
                    .to_string(),
            seed,
            requested_by: "issue1232-fsv".to_string(),
        },
        &corpus,
        &assay(),
        &ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn lowered_frost_ability() -> LoweredAbilityDocument {
    let corpus = WorldCorpus {
        world_id: "wa1-frost-vein".to_string(),
        revision_id: "wa1-frost-rev".to_string(),
        region: "Frost Vein".to_string(),
        factions: vec!["Nightglass".to_string()],
        lore_terms: vec![
            "moon shard".to_string(),
            "frost veil".to_string(),
            "glacier hollow".to_string(),
        ],
        anchors: vec![
            anchor(
                "frost-region",
                "Aster://wa1/world/frost#region",
                "Frost Vein is a glacier hollow.",
                &["Frost Vein", "glacier hollow"],
            ),
            anchor(
                "frost-faction",
                "Aster://wa1/factions/nightglass",
                "Nightglass hides behind a frost veil.",
                &["Nightglass", "frost veil"],
            ),
            anchor(
                "frost-reagent",
                "Aster://wa1/reagents/moon-shard",
                "Moon shard bends cold light.",
                &["moon shard"],
            ),
        ],
    };
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: "req-frost".to_string(),
            prompt:
                "Create a Nightglass moon shard frost veil ability for Frost Vein glacier hollow."
                    .to_string(),
            seed: 5150,
            requested_by: "issue1232-fsv".to_string(),
        },
        &corpus,
        &AssayBalanceProfile {
            allowed_damage_types: vec!["frost veil".to_string(), "moon shard".to_string()],
            ..assay()
        },
        &ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn boundary_item(id: &str, text: &str) -> Result<CatalogDedupRecord, Box<dyn std::error::Error>> {
    Ok(CatalogDedupRecord::from_slot_texts(
        id,
        litd_editor::authoring_dedup_governance::CatalogEntityKind::Item,
        "wa-boundary",
        vec!["anchor-boundary".to_string()],
        vec!["Aster://boundary/item".to_string()],
        stable_fingerprint(&text)?,
        [
            (DedupSlotKind::Identity, text.to_string()),
            (DedupSlotKind::Mechanics, text.to_string()),
            (DedupSlotKind::Semantics, text.to_string()),
        ],
    )?)
}

fn corpus(world_id: &str) -> WorldCorpus {
    WorldCorpus {
        world_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        region: "Ashen Reach".to_string(),
        factions: vec!["Dawnward".to_string(), "Cinder Choir".to_string()],
        lore_terms: vec![
            "cinder bloom".to_string(),
            "ward flame".to_string(),
            "dusk forest".to_string(),
        ],
        anchors: vec![
            anchor(
                "region",
                &format!("Aster://{world_id}/world/ashen-reach#region"),
                "Ashen Reach is a dusk forest scarred by ember fog.",
                &["Ashen Reach", "dusk forest"],
            ),
            anchor(
                "faction",
                &format!("Aster://{world_id}/factions/dawnward"),
                "The Dawnward bind ward flame into defensive rites.",
                &["Dawnward", "ward flame"],
            ),
            anchor(
                "reagent",
                &format!("Aster://{world_id}/reagents/cinder-bloom"),
                "Cinder bloom stores heat without open flame.",
                &["cinder bloom"],
            ),
        ],
    }
}

fn assay() -> AssayBalanceProfile {
    AssayBalanceProfile {
        assay_id: "assay-724".to_string(),
        profile_id: "ability-tier-1-control".to_string(),
        min_damage: 24,
        max_damage: 68,
        min_cooldown_seconds: 8,
        max_cooldown_seconds: 18,
        max_mana_cost: 60,
        allowed_damage_types: vec!["ward flame".to_string(), "ember".to_string()],
    }
}

fn ward() -> WardPolicy {
    WardPolicy {
        policy_id: "ward-grounding-issue1232".to_string(),
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
        if snapshot.after.exists == metadata.is_file() && snapshot.after.bytes == metadata.len() {
            return Ok(());
        }
        snapshot.after = AfterState {
            exists: metadata.is_file(),
            bytes: metadata.len(),
        };
    }
    Err(format!(
        "FSV snapshot metadata did not stabilize for {}",
        path.display()
    )
    .into())
}
