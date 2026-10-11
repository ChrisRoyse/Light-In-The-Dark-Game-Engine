use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        CindermarkSource, InfernalAccessCostKind, InfernalAccessState, InfernalCircleId,
        InfernalEntryPath, InfernalEntryRequest, InfernalEscapeAttempt, InfernalEscapeKind,
        RPG_INFERNAL_ACCESS_SCHEMA, infernal_access_content_records, infernal_access_state_hash,
        persist_and_lower_rpg_authored_content, resolve_infernal_entry,
        resolve_infernal_escape_attempt, validate_infernal_access_policy,
        validate_infernal_access_state, validate_infernal_entry_request,
        validate_rpg_authored_content_records,
    },
    rpg_infernal_access_test_fixture::ashen_reach_infernal_access_policy,
};

#[test]
fn issue1066_earned_cindermark_opens_gate_after_long_descent() {
    let policy = ashen_reach_infernal_access_policy();
    let state = outside_state(None);
    let readback = resolve_infernal_entry(&state, &policy, &earned_request()).unwrap();

    assert!(readback.granted);
    assert_eq!(
        readback.path,
        InfernalEntryPath::LongDescentEarnedCindermark
    );
    assert_eq!(readback.destination_circle, InfernalCircleId::Threshold);
    assert_eq!(
        readback.cost_kind,
        InfernalAccessCostKind::CindermarkTrialScar
    );
    assert_eq!(readback.infernal_debt_before, 0);
    assert_eq!(readback.infernal_debt_after, 0);
    assert!(readback.no_recall_lock_active);
    assert!(readback.state_after.inside_infernal_deeps);
    assert_eq!(
        readback.state_after.current_circle,
        Some(InfernalCircleId::Threshold)
    );
}

#[test]
fn issue1066_devil_brand_and_pact_paths_apply_hidden_debt() {
    let policy = ashen_reach_infernal_access_policy();
    let devil =
        resolve_infernal_entry(&outside_state(None), &policy, &devil_brand_request()).unwrap();
    let pact = resolve_infernal_entry(&outside_state(None), &policy, &lesser_devil_pact_request())
        .unwrap();

    assert_eq!(devil.cost_kind, InfernalAccessCostKind::DevilBrandDebt);
    assert_eq!(devil.infernal_debt_after, 1);
    assert_eq!(pact.cost_kind, InfernalAccessCostKind::PactDebt);
    assert_eq!(pact.infernal_debt_after, 1);
}

#[test]
fn issue1066_hollow_pact_seam_requires_faction_membership() {
    let policy = ashen_reach_infernal_access_policy();
    let state = outside_state(Some("faction.hollow_pact"));
    let readback = resolve_infernal_entry(&state, &policy, &hollow_pact_request()).unwrap();

    assert_eq!(readback.path, InfernalEntryPath::HollowPactSeam);
    assert_eq!(readback.cost_kind, InfernalAccessCostKind::FactionSeamDebt);
    assert_eq!(readback.infernal_debt_after, 1);
    assert!(readback.state_after.no_recall_lock_active);
}

#[test]
fn issue1066_dragged_under_is_accidental_only() {
    let policy = ashen_reach_infernal_access_policy();
    let readback =
        resolve_infernal_entry(&outside_state(None), &policy, &dragged_under_request()).unwrap();

    assert_eq!(readback.path, InfernalEntryPath::DraggedUnderAccident);
    assert_eq!(
        readback.cost_kind,
        InfernalAccessCostKind::DraggedUnderTrauma
    );
    assert_eq!(readback.infernal_debt_after, 1);
}

#[test]
fn issue1066_recall_and_teleport_are_denied_inside_without_mutation() {
    let policy = ashen_reach_infernal_access_policy();
    let inside = resolve_infernal_entry(&outside_state(None), &policy, &earned_request())
        .unwrap()
        .state_after;
    let before = infernal_access_state_hash(&inside).unwrap();
    let recall = resolve_infernal_escape_attempt(
        &inside,
        &policy,
        &escape("escape.recall.1", InfernalEscapeKind::Recall),
    )
    .unwrap();
    let teleport = resolve_infernal_escape_attempt(
        &inside,
        &policy,
        &escape("escape.teleport.1", InfernalEscapeKind::Teleport),
    )
    .unwrap();

    assert!(recall.denied);
    assert_eq!(recall.denial_reason, "infernal_no_recall_lock");
    assert!(recall.state_unchanged);
    assert_eq!(before, infernal_access_state_hash(&inside).unwrap());
    assert!(teleport.denied);
    assert!(teleport.state_unchanged);
}

#[test]
fn issue1066_access_policy_persists_as_authored_calyx_content() {
    let policy = ashen_reach_infernal_access_policy();
    let records = infernal_access_content_records(&policy).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let (store, _database, _vault_dir) = content_store("access");
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    let rows = records
        .iter()
        .map(|record| {
            store
                .get_content_for_editor(record.kind, &record.id)
                .unwrap()
        })
        .collect::<Vec<_>>();

    assert_eq!(validation.record_count, 7);
    assert_eq!(lowered.row_count, 7);
    assert_eq!(rows.len(), 7);
    assert!(records.iter().all(|record| record.anchors.is_empty()));
    assert!(
        rows.iter()
            .all(|row| row.kind == EngineContentKind::Trigger)
    );
    assert!(rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/ashen_reach/infernal_access/")
    }));
    assert!(
        lowered
            .entities
            .iter()
            .all(|entity| entity.lowered_field_count > 0)
    );
    assert!(
        rows.iter()
            .any(|row| row.payload["surface_kind"] == "infernal_access_escape_denials")
    );
    assert!(rows.iter().all(|row| {
        row.payload["built_in_map_required"] != true
            && row.payload["grants_map_or_minimap"] != true
            && row.payload["grants_gps_markers"] != true
    }));
}

#[test]
fn issue1066_access_fails_closed_on_invalid_edges() {
    let policy = ashen_reach_infernal_access_policy();

    let mut duplicate = policy.clone();
    duplicate.entry_rules[1].path = InfernalEntryPath::LongDescentEarnedCindermark;
    assert_code(
        validate_infernal_access_policy(&duplicate),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut no_recall = policy.clone();
    no_recall.no_recall_inside = false;
    assert_code(
        validate_infernal_access_policy(&no_recall),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut missing_mark = earned_request();
    missing_mark.cindermark_ref = None;
    assert_code(
        validate_infernal_entry_request(&missing_mark, &policy),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    assert_code(
        resolve_infernal_entry(&outside_state(None), &policy, &hollow_pact_request()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let inside = resolve_infernal_entry(&outside_state(None), &policy, &earned_request())
        .unwrap()
        .state_after;
    assert_code(
        resolve_infernal_entry(&inside, &policy, &earned_request()),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    let mut bad_outside = outside_state(None);
    bad_outside.no_recall_lock_active = true;
    assert_code(
        validate_infernal_access_state(&bad_outside, &policy),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn outside_state(faction_id: Option<&str>) -> InfernalAccessState {
    InfernalAccessState {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        character_id: "char.hellrunner".to_string(),
        party_id: "party.vanguard".to_string(),
        location_id: "zone.sunken_vaults.gatewalk".to_string(),
        faction_id: faction_id.map(str::to_string),
        inside_infernal_deeps: false,
        current_circle: None,
        no_recall_lock_active: false,
        infernal_debt_count: 0,
        access_history: Vec::new(),
    }
}

fn earned_request() -> InfernalEntryRequest {
    request(InfernalEntryPath::LongDescentEarnedCindermark)
}

fn devil_brand_request() -> InfernalEntryRequest {
    let mut request = request(InfernalEntryPath::LongDescentDevilBrandedCindermark);
    request.cindermark_ref = Some("sigil.cindermark.devil_brand".to_string());
    request.cindermark_source = Some(CindermarkSource::DevilBrand);
    request.lesser_devil_id = Some("devil.vorthax_gatebroker".to_string());
    request.accepted_hidden_cost = true;
    request
}

fn lesser_devil_pact_request() -> InfernalEntryRequest {
    let mut request = request(InfernalEntryPath::LesserDevilPact);
    request.at_gate_id = None;
    request.completed_long_descent_ref = None;
    request.cindermark_ref = None;
    request.cindermark_source = None;
    request.lesser_devil_id = Some("devil.vorthax_gatebroker".to_string());
    request.pact_contract_id = Some("contract.infernal_passage.1".to_string());
    request.accepted_hidden_cost = true;
    request
}

fn hollow_pact_request() -> InfernalEntryRequest {
    let mut request = request(InfernalEntryPath::HollowPactSeam);
    request.at_gate_id = None;
    request.completed_long_descent_ref = None;
    request.cindermark_ref = None;
    request.cindermark_source = None;
    request.faction_power_id = Some("power.hollow_pact.infernal_seam".to_string());
    request.accepted_hidden_cost = true;
    request
}

fn dragged_under_request() -> InfernalEntryRequest {
    let mut request = request(InfernalEntryPath::DraggedUnderAccident);
    request.at_gate_id = None;
    request.completed_long_descent_ref = None;
    request.cindermark_ref = None;
    request.cindermark_source = None;
    request.thin_place_death_id = Some("death.thin_place.ember_crack.1".to_string());
    request.current_location_thin_place = true;
    request.accident_not_player_chosen = true;
    request
}

fn request(path: InfernalEntryPath) -> InfernalEntryRequest {
    InfernalEntryRequest {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        request_id: format!("infernal.entry.{}", path.label()),
        character_id: "char.hellrunner".to_string(),
        path,
        at_gate_id: Some("gate.cinders".to_string()),
        completed_long_descent_ref: Some("quest.long_descent.sunken_vaults".to_string()),
        cindermark_ref: Some("sigil.cindermark.earned".to_string()),
        cindermark_source: Some(CindermarkSource::EarnedQuestline),
        lesser_devil_id: None,
        pact_contract_id: None,
        accepted_hidden_cost: false,
        faction_power_id: None,
        thin_place_death_id: None,
        current_location_thin_place: false,
        accident_not_player_chosen: false,
    }
}

fn escape(attempt_id: &str, kind: InfernalEscapeKind) -> InfernalEscapeAttempt {
    InfernalEscapeAttempt {
        schema: RPG_INFERNAL_ACCESS_SCHEMA.to_string(),
        attempt_id: attempt_id.to_string(),
        character_id: "char.hellrunner".to_string(),
        kind,
        declared_destination_id: "zone.galdmere.square".to_string(),
    }
}

fn content_store(
    label: &str,
) -> (
    EngineCalyxContentStore,
    EngineCalyxDatabase,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database.clone(), label),
        database,
        vault_dir,
    )
}

// #1535: TestTempDir auto-cleans on Drop.
fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1066-{label}"))
}

fn assert_code<T: std::fmt::Debug>(result: Result<T, EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}
