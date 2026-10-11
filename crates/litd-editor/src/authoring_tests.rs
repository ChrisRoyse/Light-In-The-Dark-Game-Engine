use super::*;

#[test]
fn grounded_request_lowers_to_valid_fingerprinted_toml() {
    let outcome =
        generate_grounded_ability(&happy_request(), &corpus(), &assay(), &ward()).unwrap();

    assert_eq!(outcome.status, AuthoringStatus::Admitted);
    assert_eq!(outcome.ward_decision.code, "LITD-WARD-PASS");
    let lowered = outcome.lowered.as_ref().unwrap();
    assert!(lowered.canonical_toml.contains("[ability]"));
    assert!(lowered.canonical_toml.contains("cinder_bloom"));

    let document =
        verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap();
    assert_eq!(document.ability.id, "ability.dawnward.cinder_bloom");
    assert_eq!(document.provenance.source_refs.len(), 3);
}

#[test]
fn empty_request_is_refused_without_lowering() {
    let mut request = happy_request();
    request.prompt.clear();

    let outcome = generate_grounded_ability(&request, &corpus(), &assay(), &ward()).unwrap();

    assert_eq!(outcome.status, AuthoringStatus::Refused);
    assert_eq!(outcome.admission_code, "LITD-AUTHORING-EMPTY-REQUEST");
    assert!(outcome.proposal.is_none());
    assert!(outcome.lowered.is_none());
}

#[test]
fn out_of_domain_request_is_refused_without_lowering() {
    let mut request = happy_request();
    request.prompt = "make a spaceship laser stock market ability".to_string();

    let outcome = generate_grounded_ability(&request, &corpus(), &assay(), &ward()).unwrap();

    assert_eq!(outcome.status, AuthoringStatus::Refused);
    assert_eq!(outcome.admission_code, "LITD-AUTHORING-REFUSED-OOD");
    assert_eq!(
        outcome.grounding.blocked_terms,
        vec!["spaceship", "laser", "stock market"]
    );
    assert!(outcome.lowered.is_none());
}

#[test]
fn thin_grounding_returns_provisional_without_lowering() {
    let mut request = happy_request();
    request.prompt = "make something for Dawnward".to_string();

    let outcome = generate_grounded_ability(&request, &corpus(), &assay(), &ward()).unwrap();

    assert_eq!(outcome.status, AuthoringStatus::Provisional);
    assert_eq!(
        outcome.admission_code,
        "LITD-AUTHORING-PROVISIONAL-UNGROUNDED"
    );
    assert!(outcome.proposal.is_none());
    assert!(outcome.lowered.is_none());
}

#[test]
fn fingerprint_mismatch_is_rejected() {
    let outcome =
        generate_grounded_ability(&happy_request(), &corpus(), &assay(), &ward()).unwrap();
    let lowered = outcome.lowered.unwrap();

    match verify_lowered_ability_toml(&lowered.canonical_toml, "0000000000000000") {
        Err(EngineError::SerializedFingerprintMismatch { expected, .. }) => {
            assert_eq!(expected, "0000000000000000");
        }
        other => panic!("expected SerializedFingerprintMismatch, got {other:?}"),
    }
}

#[test]
fn same_seed_regenerates_same_lowered_artifact() {
    let first = generate_grounded_ability(&happy_request(), &corpus(), &assay(), &ward()).unwrap();
    let second = generate_grounded_ability(&happy_request(), &corpus(), &assay(), &ward()).unwrap();

    assert_eq!(
        first.lowered.as_ref().unwrap().fingerprint,
        second.lowered.as_ref().unwrap().fingerprint
    );
    assert_eq!(
        first.lowered.as_ref().unwrap().canonical_toml,
        second.lowered.as_ref().unwrap().canonical_toml
    );
}

#[test]
fn tampered_lowered_balance_is_rejected() {
    let outcome =
        generate_grounded_ability(&happy_request(), &corpus(), &assay(), &ward()).unwrap();
    let lowered = outcome.lowered.unwrap();
    let tampered = lowered
        .canonical_toml
        .replace("damage = ", "damage = 999 # ");

    match verify_lowered_ability_toml(&tampered, &lowered.fingerprint) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.authoring.toml.balance");
            assert!(reason.contains("balance"));
        }
        other => panic!("expected balance rejection, got {other:?}"),
    }
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
            GroundingAnchor {
                anchor_id: "wa1-anchor-region".to_string(),
                source_ref: "Aster://wa1/world/ashen-reach#region".to_string(),
                text: "Ashen Reach is a dusk forest scarred by ember fog.".to_string(),
                terms: vec!["Ashen Reach".to_string(), "dusk forest".to_string()],
            },
            GroundingAnchor {
                anchor_id: "wa1-anchor-faction".to_string(),
                source_ref: "Aster://wa1/factions/dawnward".to_string(),
                text: "The Dawnward bind ward flame into defensive rites.".to_string(),
                terms: vec!["Dawnward".to_string(), "ward flame".to_string()],
            },
            GroundingAnchor {
                anchor_id: "wa1-anchor-reagent".to_string(),
                source_ref: "Aster://wa1/reagents/cinder-bloom".to_string(),
                text: "Cinder bloom stores heat without open flame.".to_string(),
                terms: vec!["cinder bloom".to_string()],
            },
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
