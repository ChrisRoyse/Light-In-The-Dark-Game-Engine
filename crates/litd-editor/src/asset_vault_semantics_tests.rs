use crate::asset_vault::AssetVaultCatalog;

use super::*;

#[test]
fn real_catalog_exposes_issue1242_semantic_coverage() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    crate::asset_vault_fixture::require_full_vault(&catalog);
    println!(
        "issue1242 semantics before records={} usable={} roles=unbuilt",
        catalog.record_count(),
        catalog.usable_record_count()
    );

    let index = AssetVaultSemanticIndex::from_records(catalog.records()).unwrap();
    let coverage = index.coverage().unwrap();
    println!(
        "issue1242 semantics after profiles={} classified={} unclassified={} tags={} roles={}",
        coverage.profile_count,
        coverage.classified_profile_count,
        coverage.unclassified_profile_count,
        coverage.tag_count,
        coverage.role_counts
    );

    assert_eq!(coverage.profile_count, catalog.record_count());
    assert!(coverage.classified_profile_count > 0);
    assert!(coverage.tag_count >= 16);
    for (role, minimum) in REQUIRED_ROLES {
        assert!(
            index.role_count(role) >= *minimum,
            "{role} did not meet minimum {minimum}: {}",
            coverage.role_counts
        );
    }
}

#[test]
fn audio_only_real_subset_fails_ed11_semantic_coverage() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    crate::asset_vault_fixture::require_full_vault(&catalog);
    let records = catalog
        .records()
        .iter()
        .filter(|record| roles_for_record(record).iter().any(|role| role == "audio"))
        .cloned()
        .collect::<Vec<_>>();
    println!(
        "issue1242 semantics audio_only before records={} first={}",
        records.len(),
        records
            .first()
            .map(|record| record.relative_path.as_str())
            .unwrap_or("none")
    );

    let index = AssetVaultSemanticIndex::from_records(&records).unwrap();
    let error = index.coverage().unwrap_err().to_string();
    println!(
        "issue1242 semantics audio_only after roles={} error={}",
        index.role_counts_line(),
        error
    );

    assert!(records.len() >= 100);
    assert!(error.contains("editor.asset_vault.issue1242_semantic_coverage"));
    assert!(error.contains("architecture=0<500"));
}

#[test]
fn pbr_surface_only_real_subset_fails_character_and_audio_coverage() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    crate::asset_vault_fixture::require_full_vault(&catalog);
    let records = catalog
        .records()
        .iter()
        .filter(|record| {
            let roles = roles_for_record(record);
            roles.iter().any(|role| role == "terrain_surface")
                && !roles.iter().any(|role| role == "character")
                && !roles.iter().any(|role| role == "audio")
        })
        .take(450)
        .cloned()
        .collect::<Vec<_>>();
    println!(
        "issue1242 semantics surface_only before records={} first={}",
        records.len(),
        records
            .first()
            .map(|record| record.relative_path.as_str())
            .unwrap_or("none")
    );

    let index = AssetVaultSemanticIndex::from_records(&records).unwrap();
    let error = index.coverage().unwrap_err().to_string();
    println!(
        "issue1242 semantics surface_only after roles={} error={}",
        index.role_counts_line(),
        error
    );

    assert!(records.len() >= 250);
    assert!(error.contains("character=0<100"));
    assert!(error.contains("audio=0<100"));
}

#[test]
fn empty_real_subset_fails_before_status_is_reported() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let records = catalog
        .records()
        .iter()
        .filter(|record| record.source == "source-that-does-not-exist")
        .cloned()
        .collect::<Vec<_>>();
    println!(
        "issue1242 semantics empty before records={} after_expected=no_index",
        records.len()
    );

    let error = AssetVaultSemanticIndex::from_records(&records)
        .unwrap_err()
        .to_string();
    println!("issue1242 semantics empty after error={error}");

    assert!(records.is_empty());
    assert!(error.contains("editor.asset_vault.semantic.records"));
}
