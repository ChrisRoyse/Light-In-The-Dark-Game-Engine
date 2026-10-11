use super::serialize_sha256;
use serde_json::{Value, json};

use crate::{
    EngineError,
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
};

use super::{
    ReligionThemeDefinition, RoleThemeDefinition, SphereDefinition, ThemeCatalog, ThemeRules,
    build_theme_catalog_readback, rpg_authored_payload_without_nulls,
    themes_validate::{validate_theme_catalog, validate_theme_rules},
    validate_rpg_authored_content_records,
};

pub fn theme_content_records(
    catalog: &ThemeCatalog,
    rules: &ThemeRules,
) -> Result<Vec<EngineContentRecord>, EngineError> {
    validate_theme_rules(rules)?;
    validate_theme_catalog(catalog)?;
    let readback = build_theme_catalog_readback(catalog)?;
    let catalog_sha256 = readback.input_sha256.clone();
    let catalog_result_sha256 = readback.result_sha256.clone();
    let rules_sha256 = serialize_sha256(rules)?;
    let mut records = Vec::new();

    records.push(content_record(
        EngineContentKind::Trigger,
        "server.ashen_reach.themes.rules",
        "Theme Rules: Ashen Reach",
        "rules",
        json!({
            "schema": catalog.schema,
            "source_issue": 964,
            "content_migration_issue": 1455,
            "surface_kind": "theme_rules",
            "catalog_id": catalog.catalog_id,
            "sphere_choice_deadline_level": rules.sphere_choice_deadline_level,
            "direct_sphere_score": rules.direct_sphere_score,
            "role_sphere_score": rules.role_sphere_score,
            "required_tag_score": rules.required_tag_score,
            "forbidden_tag_penalty": rules.forbidden_tag_penalty,
            "minimum_religion_fit_score": rules.minimum_religion_fit_score,
            "theme_rules_sha256": rules_sha256,
            "theme_catalog_sha256": catalog_sha256,
            "theme_catalog_result_sha256": catalog_result_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )?);
    for sphere in &catalog.spheres {
        records.push(sphere_record(
            sphere,
            catalog,
            &catalog_sha256,
            &catalog_result_sha256,
        )?);
    }
    for role in &catalog.roles {
        records.push(role_record(
            role,
            catalog,
            &catalog_sha256,
            &catalog_result_sha256,
        )?);
    }
    for religion in &catalog.religions {
        records.push(religion_record(
            religion,
            catalog,
            &catalog_sha256,
            &catalog_result_sha256,
        )?);
    }

    validate_rpg_authored_content_records(&records).map_err(|error| {
        EngineError::RpgThemeInvalidConfig {
            field: "themes.authored_content".to_string(),
            reason: error.to_string(),
        }
    })?;
    Ok(records)
}

fn sphere_record(
    sphere: &SphereDefinition,
    catalog: &ThemeCatalog,
    catalog_sha256: &str,
    catalog_result_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Lore,
        &sphere.sphere_id,
        &format!("Theme Sphere: {}", sphere.display_name),
        "spheres",
        json!({
            "schema": catalog.schema,
            "source_issue": 964,
            "content_migration_issue": 1455,
            "surface_kind": "theme_sphere",
            "catalog_id": catalog.catalog_id,
            "sphere_id": sphere.sphere_id,
            "display_name": sphere.display_name,
            "theme_tags": sphere.theme_tags,
            "affinity_sphere_ids": sphere.affinity_sphere_ids,
            "opposing_sphere_ids": sphere.opposing_sphere_ids,
            "theme_catalog_sha256": catalog_sha256,
            "theme_catalog_result_sha256": catalog_result_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn role_record(
    role: &RoleThemeDefinition,
    catalog: &ThemeCatalog,
    catalog_sha256: &str,
    catalog_result_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Story,
        &role.role_id,
        &format!("Role Theme: {}", role.display_name),
        "roles",
        json!({
            "schema": catalog.schema,
            "source_issue": 964,
            "content_migration_issue": 1455,
            "surface_kind": "role_theme",
            "catalog_id": catalog.catalog_id,
            "role_id": role.role_id,
            "display_name": role.display_name,
            "primary_sphere_id": role.primary_sphere_id,
            "supporting_sphere_ids": role.supporting_sphere_ids,
            "required_theme_tags": role.required_theme_tags,
            "optional_theme_tags": role.optional_theme_tags,
            "theme_catalog_sha256": catalog_sha256,
            "theme_catalog_result_sha256": catalog_result_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn religion_record(
    religion: &ReligionThemeDefinition,
    catalog: &ThemeCatalog,
    catalog_sha256: &str,
    catalog_result_sha256: &str,
) -> Result<EngineContentRecord, EngineError> {
    content_record(
        EngineContentKind::Lore,
        &religion.religion_id,
        &format!("Religion Theme: {}", religion.deity_name),
        "religions",
        json!({
            "schema": catalog.schema,
            "source_issue": 964,
            "content_migration_issue": 1455,
            "surface_kind": "religion_theme",
            "catalog_id": catalog.catalog_id,
            "religion_id": religion.religion_id,
            "deity_id": religion.deity_id,
            "deity_name": religion.deity_name,
            "sphere_ids": religion.sphere_ids,
            "required_theme_tags": religion.required_theme_tags,
            "forbidden_theme_tags": religion.forbidden_theme_tags,
            "empowerment_class_ids": religion.empowerment_class_ids,
            "theme_catalog_sha256": catalog_sha256,
            "theme_catalog_result_sha256": catalog_result_sha256,
            "built_in_map_required": false,
            "grants_map_or_minimap": false,
            "grants_gps_markers": false,
        }),
    )
}

fn content_record(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    collection: &str,
    payload: Value,
) -> Result<EngineContentRecord, EngineError> {
    let payload = rpg_authored_payload_without_nulls(payload);
    Ok(EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        source: EngineContentSource {
            path: format!("editor://rpg/ashen_reach/themes/{collection}/{id}"),
            author: "editor_authoring.issue1455.themes".to_string(),
            source_hash: serialize_sha256(&payload)?,
        },
        payload,
        anchors: Vec::new(),
    })
}
