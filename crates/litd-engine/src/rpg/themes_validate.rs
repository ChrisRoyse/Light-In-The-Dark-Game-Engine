use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::{
    CharacterThemeInput, RPG_THEME_SCHEMA, ReligionThemeDefinition, RoleThemeDefinition,
    SphereDefinition, ThemeCatalog, ThemeRules,
};

pub(super) struct ThemeIndexes<'a> {
    pub spheres: BTreeMap<&'a str, &'a SphereDefinition>,
    pub roles: BTreeMap<&'a str, &'a RoleThemeDefinition>,
    pub religions: BTreeMap<&'a str, &'a ReligionThemeDefinition>,
}

pub(super) fn validate_theme_rules(rules: &ThemeRules) -> Result<(), EngineError> {
    if rules.schema != RPG_THEME_SCHEMA {
        return invalid("rules.schema", format!("expected {RPG_THEME_SCHEMA}"));
    }
    if rules.sphere_choice_deadline_level == 0 || rules.sphere_choice_deadline_level > 100 {
        return invalid(
            "rules.sphere_choice_deadline_level",
            "must be inside 1..=100",
        );
    }
    if rules.direct_sphere_score <= 0
        || rules.role_sphere_score <= 0
        || rules.required_tag_score <= 0
        || rules.forbidden_tag_penalty <= 0
        || rules.minimum_religion_fit_score <= 0
    {
        return invalid("rules.scoring", "scores and penalties must be positive");
    }
    Ok(())
}

pub(super) fn validate_theme_catalog(
    catalog: &ThemeCatalog,
) -> Result<ThemeIndexes<'_>, EngineError> {
    if catalog.schema != RPG_THEME_SCHEMA {
        return invalid("catalog.schema", format!("expected {RPG_THEME_SCHEMA}"));
    }
    validate_token("catalog.catalog_id", &catalog.catalog_id)?;
    if catalog.spheres.is_empty() || catalog.roles.is_empty() || catalog.religions.is_empty() {
        return invalid("catalog", "spheres, roles, and religions must be non-empty");
    }
    let spheres = index_spheres(&catalog.spheres)?;
    let roles = index_roles(&catalog.roles, &spheres)?;
    let religions = index_religions(&catalog.religions, &spheres)?;
    Ok(ThemeIndexes {
        spheres,
        roles,
        religions,
    })
}

pub(super) fn validate_character_theme_input(
    input: &CharacterThemeInput,
    rules: &ThemeRules,
    indexes: &ThemeIndexes<'_>,
) -> Result<(), EngineError> {
    validate_token("character_id", &input.character_id)?;
    validate_token("class_id", &input.class_id)?;
    validate_token("role_id", &input.role_id)?;
    if input.level == 0 || input.level > 100 {
        return invalid("level", "must be inside 1..=100");
    }
    if input.level > rules.sphere_choice_deadline_level && input.chosen_sphere_id.is_none() {
        return invalid(
            "chosen_sphere_id",
            "sphere is required after the choice deadline level",
        );
    }
    if !indexes.roles.contains_key(input.role_id.as_str()) {
        return invalid("role_id", "unknown role");
    }
    if let Some(sphere_id) = &input.chosen_sphere_id {
        validate_token("chosen_sphere_id", sphere_id)?;
        if !indexes.spheres.contains_key(sphere_id.as_str()) {
            return invalid("chosen_sphere_id", "unknown sphere");
        }
    }
    if let Some(religion_id) = &input.desired_religion_id {
        validate_token("desired_religion_id", religion_id)?;
        if !indexes.religions.contains_key(religion_id.as_str()) {
            return invalid("desired_religion_id", "unknown religion");
        }
    }
    validate_tag_rows("declared_theme_tags", &input.declared_theme_tags)
}

fn index_spheres(
    spheres: &[SphereDefinition],
) -> Result<BTreeMap<&str, &SphereDefinition>, EngineError> {
    let mut index = BTreeMap::new();
    for sphere in spheres {
        validate_token("spheres.sphere_id", &sphere.sphere_id)?;
        validate_display("spheres.display_name", &sphere.display_name)?;
        validate_tag_rows("spheres.theme_tags", &sphere.theme_tags)?;
        validate_ref_rows("spheres.affinity_sphere_ids", &sphere.affinity_sphere_ids)?;
        validate_ref_rows("spheres.opposing_sphere_ids", &sphere.opposing_sphere_ids)?;
        if index.insert(sphere.sphere_id.as_str(), sphere).is_some() {
            return invalid("spheres.sphere_id", "duplicate sphere");
        }
    }
    for sphere in spheres {
        validate_sphere_refs(
            "spheres.affinity_sphere_ids",
            &sphere.affinity_sphere_ids,
            &index,
        )?;
        validate_sphere_refs(
            "spheres.opposing_sphere_ids",
            &sphere.opposing_sphere_ids,
            &index,
        )?;
    }
    Ok(index)
}

fn index_roles<'a>(
    roles: &'a [RoleThemeDefinition],
    spheres: &BTreeMap<&str, &SphereDefinition>,
) -> Result<BTreeMap<&'a str, &'a RoleThemeDefinition>, EngineError> {
    let mut index = BTreeMap::new();
    for role in roles {
        validate_token("roles.role_id", &role.role_id)?;
        validate_display("roles.display_name", &role.display_name)?;
        validate_token("roles.primary_sphere_id", &role.primary_sphere_id)?;
        validate_ref_rows("roles.supporting_sphere_ids", &role.supporting_sphere_ids)?;
        validate_tag_rows("roles.required_theme_tags", &role.required_theme_tags)?;
        validate_tag_rows("roles.optional_theme_tags", &role.optional_theme_tags)?;
        if !spheres.contains_key(role.primary_sphere_id.as_str()) {
            return invalid("roles.primary_sphere_id", "unknown sphere");
        }
        validate_sphere_refs(
            "roles.supporting_sphere_ids",
            &role.supporting_sphere_ids,
            spheres,
        )?;
        if index.insert(role.role_id.as_str(), role).is_some() {
            return invalid("roles.role_id", "duplicate role");
        }
    }
    Ok(index)
}

fn index_religions<'a>(
    religions: &'a [ReligionThemeDefinition],
    spheres: &BTreeMap<&str, &SphereDefinition>,
) -> Result<BTreeMap<&'a str, &'a ReligionThemeDefinition>, EngineError> {
    let mut index = BTreeMap::new();
    for religion in religions {
        validate_token("religions.religion_id", &religion.religion_id)?;
        validate_token("religions.deity_id", &religion.deity_id)?;
        validate_display("religions.deity_name", &religion.deity_name)?;
        validate_ref_rows("religions.sphere_ids", &religion.sphere_ids)?;
        validate_tag_rows(
            "religions.required_theme_tags",
            &religion.required_theme_tags,
        )?;
        validate_tag_rows(
            "religions.forbidden_theme_tags",
            &religion.forbidden_theme_tags,
        )?;
        validate_ref_rows(
            "religions.empowerment_class_ids",
            &religion.empowerment_class_ids,
        )?;
        validate_sphere_refs("religions.sphere_ids", &religion.sphere_ids, spheres)?;
        if index
            .insert(religion.religion_id.as_str(), religion)
            .is_some()
        {
            return invalid("religions.religion_id", "duplicate religion");
        }
    }
    Ok(index)
}

fn validate_sphere_refs(
    field: &str,
    refs: &[String],
    spheres: &BTreeMap<&str, &SphereDefinition>,
) -> Result<(), EngineError> {
    for id in refs {
        if !spheres.contains_key(id.as_str()) {
            return invalid(field, format!("unknown sphere {id}"));
        }
    }
    Ok(())
}

fn validate_ref_rows(field: &str, rows: &[String]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for row in rows {
        validate_token(field, row)?;
        if !seen.insert(row.as_str()) {
            return invalid(field, "duplicate value");
        }
    }
    Ok(())
}

fn validate_tag_rows(field: &str, rows: &[String]) -> Result<(), EngineError> {
    if rows.len() > 32 {
        return invalid(field, "must contain <=32 tags");
    }
    validate_ref_rows(field, rows)
}

fn validate_display(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > 96 {
        return invalid(field, "must be 1..=96 bytes");
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be 1..=96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(EngineError::RpgThemeInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
