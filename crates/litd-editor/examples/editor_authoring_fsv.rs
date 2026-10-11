use litd_engine::fsv_artifacts::env_path;
use std::{fs, path::PathBuf};

use litd_editor::{
    authoring::{
        AssayBalanceProfile, AuthoringOutcome, GroundedAbilityRequest, GroundingAnchor, WardPolicy,
        WorldCorpus, generate_grounded_ability, verify_lowered_ability_toml,
    },
    history::to_stable_ron,
};
use litd_engine::EngineError;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct AuthoringFsvSnapshot {
    issue: u32,
    source_of_truths: Vec<String>,
    readme_refs: Vec<String>,
    happy_path: AuthoringOutcome,
    empty_request: AuthoringOutcome,
    out_of_domain: AuthoringOutcome,
    provisional_ungrounded: AuthoringOutcome,
    deterministic_regenerate_fingerprint: String,
    fingerprint_mismatch_code: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_AUTHORING_FSV_STATS",
        "tmp/issue1237-authoring-fsv.txt",
    );
    let snapshot_path = env_path(
        "LITD_EDITOR_AUTHORING_FSV_SNAPSHOT",
        "tmp/issue1237-authoring-snapshot.ron",
    );
    let toml_path = env_path(
        "LITD_EDITOR_AUTHORING_FSV_TOML",
        "tmp/issue1237-authoring-ability.toml",
    );

    let corpus = corpus();
    let assay = assay();
    let ward = ward();
    let request = happy_request();

    let happy_path = generate_grounded_ability(&request, &corpus, &assay, &ward)?;
    let lowered = happy_path
        .lowered
        .as_ref()
        .expect("happy path must lower to TOML")
        .clone();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint)?;
    write_text(&toml_path, &lowered.canonical_toml)?;

    let empty_request = generate_grounded_ability(
        &GroundedAbilityRequest {
            prompt: String::new(),
            ..request.clone()
        },
        &corpus,
        &assay,
        &ward,
    )?;
    let out_of_domain = generate_grounded_ability(
        &GroundedAbilityRequest {
            prompt: "make a spaceship laser stock market ability".to_string(),
            ..request.clone()
        },
        &corpus,
        &assay,
        &ward,
    )?;
    let provisional_ungrounded = generate_grounded_ability(
        &GroundedAbilityRequest {
            prompt: "make something for Dawnward".to_string(),
            ..request.clone()
        },
        &corpus,
        &assay,
        &ward,
    )?;
    let regenerate = generate_grounded_ability(&request, &corpus, &assay, &ward)?;
    let deterministic_regenerate_fingerprint =
        regenerate.lowered.as_ref().unwrap().fingerprint.clone();
    let fingerprint_mismatch_code =
        match verify_lowered_ability_toml(&lowered.canonical_toml, "0000000000000000") {
            Err(err @ EngineError::SerializedFingerprintMismatch { .. }) => err.code().to_string(),
            other => panic!("expected fingerprint mismatch, got {other:?}"),
        };

    let snapshot = AuthoringFsvSnapshot {
        issue: 1237,
        source_of_truths: vec![
            toml_path.display().to_string(),
            snapshot_path.display().to_string(),
            "real Calyx/Aster vault readback after ingest".to_string(),
        ],
        readme_refs: vec![
            "docs/readme/ED05-ai-coauthor.png".to_string(),
            "docs/readme/ED11-world-view.png".to_string(),
            "docs/readme/10-architecture.png".to_string(),
            "docs/readme/03-mcp-agent.png".to_string(),
        ],
        happy_path,
        empty_request,
        out_of_domain,
        provisional_ungrounded,
        deterministic_regenerate_fingerprint,
        fingerprint_mismatch_code,
    };
    write_text(&snapshot_path, &to_stable_ron(&snapshot)?)?;
    write_text(
        &stats_path,
        &format!(
            "issue=1237\n\
             source_of_truth_toml={}\n\
             source_of_truth_snapshot={}\n\
             happy_before=status:not_run,lowered:false\n\
             happy_after=status:{:?},fingerprint:{},toml_bytes:{},anchors:{},terms:{}\n\
             empty_before=prompt:\"\",lowered:false\n\
             empty_after=status:{:?},code:{},lowered:{}\n\
             ood_before=prompt:\"spaceship laser stock market\",lowered:false\n\
             ood_after=status:{:?},code:{},blocked:{}\n\
             provisional_before=prompt:\"make something for Dawnward\",lowered:false\n\
             provisional_after=status:{:?},code:{},anchors:{},terms:{},lowered:{}\n\
             mismatch_before=expected:0000000000000000\n\
             mismatch_after=error_code:{}\n\
             regenerate_before=seed:{}\n\
             regenerate_after=fingerprint:{},matches_happy:{}\n\
             readme_refs=ED05-ai-coauthor.png|ED11-world-view.png|10-architecture.png|03-mcp-agent.png\n\
             window_opened=false\n\
             renderer_touched=false\n",
            toml_path.display(),
            snapshot_path.display(),
            snapshot.happy_path.status,
            lowered.fingerprint,
            fs::metadata(&toml_path)?.len(),
            snapshot.happy_path.grounding.matched_anchor_ids.len(),
            snapshot.happy_path.grounding.matched_terms.len(),
            snapshot.empty_request.status,
            snapshot.empty_request.admission_code,
            snapshot.empty_request.lowered.is_some(),
            snapshot.out_of_domain.status,
            snapshot.out_of_domain.admission_code,
            snapshot.out_of_domain.grounding.blocked_terms.join("|"),
            snapshot.provisional_ungrounded.status,
            snapshot.provisional_ungrounded.admission_code,
            snapshot
                .provisional_ungrounded
                .grounding
                .matched_anchor_ids
                .len(),
            snapshot
                .provisional_ungrounded
                .grounding
                .matched_terms
                .len(),
            snapshot.provisional_ungrounded.lowered.is_some(),
            snapshot.fingerprint_mismatch_code,
            request.seed,
            snapshot.deterministic_regenerate_fingerprint,
            snapshot.deterministic_regenerate_fingerprint == lowered.fingerprint,
        ),
    )?;
    println!(
        "wrote editor authoring FSV stats to {}",
        stats_path.display()
    );
    Ok(())
}

fn happy_request() -> GroundedAbilityRequest {
    GroundedAbilityRequest {
        request_id: "issue1237-request-ability".to_string(),
        prompt: "Create a Dawnward cinder bloom ward flame ability for Ashen Reach dusk forest."
            .to_string(),
        seed: 424242,
        requested_by: "issue1237-fsv".to_string(),
    }
}

fn corpus() -> WorldCorpus {
    WorldCorpus {
        world_id: "wa1-ashen-reach".to_string(),
        revision_id: "wa1-rev-20260627".to_string(),
        region: "Ashen Reach".to_string(),
        factions: vec!["Dawnward".to_string(), "Cinder Choir".to_string()],
        lore_terms: vec![
            "cinder bloom".to_string(),
            "ward flame".to_string(),
            "dusk forest".to_string(),
        ],
        anchors: vec![
            anchor(
                "wa1-anchor-region",
                "Aster://wa1/world/ashen-reach#region",
                "Ashen Reach is a dusk forest scarred by ember fog.",
                &["Ashen Reach", "dusk forest"],
            ),
            anchor(
                "wa1-anchor-faction",
                "Aster://wa1/factions/dawnward",
                "The Dawnward bind ward flame into defensive rites.",
                &["Dawnward", "ward flame"],
            ),
            anchor(
                "wa1-anchor-reagent",
                "Aster://wa1/reagents/cinder-bloom",
                "Cinder bloom stores heat without open flame.",
                &["cinder bloom"],
            ),
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
        policy_id: "ward-grounding-issue1237".to_string(),
        min_source_anchors: 3,
        min_grounded_terms: 4,
        blocked_terms: vec![
            "spaceship".to_string(),
            "laser".to_string(),
            "stock market".to_string(),
        ],
    }
}

fn write_text(path: &PathBuf, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
