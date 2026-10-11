use std::collections::BTreeSet;

use super::serialize_sha256;

use crate::EngineError;

use super::{
    CharacterThemeInput, CharacterThemeReadback, RPG_THEME_SCHEMA, ReligionThemeDefinition,
    ReligionThemeMatchReadback, RoleThemeDefinition, SphereDefinition, ThemeCatalog,
    ThemeCatalogReadback, ThemeRules, ThemeScoreComponent,
};

pub(super) fn build_theme_catalog_readback(
    catalog: &ThemeCatalog,
) -> Result<ThemeCatalogReadback, EngineError> {
    let mut sphere_ids: Vec<_> = catalog
        .spheres
        .iter()
        .map(|sphere| sphere.sphere_id.clone())
        .collect();
    sphere_ids.sort();
    let mut role_ids: Vec<_> = catalog
        .roles
        .iter()
        .map(|role| role.role_id.clone())
        .collect();
    role_ids.sort();
    let mut religion_ids: Vec<_> = catalog
        .religions
        .iter()
        .map(|religion| religion.religion_id.clone())
        .collect();
    religion_ids.sort();
    let mut tags = BTreeSet::new();
    collect_catalog_tags(catalog, &mut tags);
    let mut readback = ThemeCatalogReadback {
        schema: RPG_THEME_SCHEMA.to_string(),
        catalog_id: catalog.catalog_id.clone(),
        sphere_count: catalog.spheres.len(),
        role_count: catalog.roles.len(),
        religion_count: catalog.religions.len(),
        theme_tag_count: tags.len(),
        deity_sphere_edge_count: catalog
            .religions
            .iter()
            .map(|religion| religion.sphere_ids.len())
            .sum(),
        sphere_ids,
        role_ids,
        religion_ids,
        input_sha256: serialize_sha256(catalog)?,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&catalog_payload(&readback))?;
    Ok(readback)
}

pub(super) fn build_character_theme_readback(
    input: &CharacterThemeInput,
    catalog: &ThemeCatalog,
    rules: &ThemeRules,
    catalog_readback: &ThemeCatalogReadback,
) -> Result<CharacterThemeReadback, EngineError> {
    let role = catalog
        .roles
        .iter()
        .find(|role| role.role_id == input.role_id)
        .ok_or_else(|| EngineError::RpgThemeInvalidConfig {
            field: "role_id".to_string(),
            reason: "unknown role".to_string(),
        })?;
    let chosen_sphere = input
        .chosen_sphere_id
        .as_deref()
        .and_then(|id| catalog.spheres.iter().find(|sphere| sphere.sphere_id == id));
    let mut effective_tags = BTreeSet::new();
    if let Some(sphere) = chosen_sphere {
        for tag in &sphere.theme_tags {
            effective_tags.insert(tag.clone());
        }
    }
    for tag in &role.required_theme_tags {
        effective_tags.insert(tag.clone());
    }
    for tag in &role.optional_theme_tags {
        effective_tags.insert(tag.clone());
    }
    for tag in &input.declared_theme_tags {
        effective_tags.insert(tag.clone());
    }
    let religion = input.desired_religion_id.as_deref().and_then(|id| {
        catalog
            .religions
            .iter()
            .find(|religion| religion.religion_id == id)
    });
    let religion_match = religion.map(|religion| {
        match_religion(input, role, chosen_sphere, religion, rules, &effective_tags)
    });
    let mut readback = CharacterThemeReadback {
        schema: RPG_THEME_SCHEMA.to_string(),
        character_id: input.character_id.clone(),
        class_id: input.class_id.clone(),
        level: input.level,
        chosen_sphere_id: input.chosen_sphere_id.clone(),
        role_id: input.role_id.clone(),
        desired_religion_id: input.desired_religion_id.clone(),
        sphere_deadline_level: rules.sphere_choice_deadline_level,
        sphere_selection_required: input.level > rules.sphere_choice_deadline_level,
        role_sphere_match: role_sphere_match(input.chosen_sphere_id.as_deref(), role),
        role_theme_tags: sorted_role_tags(role),
        declared_theme_tags: sorted_vec(input.declared_theme_tags.clone()),
        effective_theme_tags: effective_tags.into_iter().collect(),
        religion_match,
        catalog_sha256: catalog_readback.input_sha256.clone(),
        input_sha256: serialize_sha256(input)?,
        rules_sha256: serialize_sha256(rules)?,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&character_payload(&readback))?;
    Ok(readback)
}

fn match_religion(
    input: &CharacterThemeInput,
    role: &RoleThemeDefinition,
    chosen_sphere: Option<&SphereDefinition>,
    religion: &ReligionThemeDefinition,
    rules: &ThemeRules,
    tags: &BTreeSet<String>,
) -> ReligionThemeMatchReadback {
    let mut score = 0;
    let mut breakdown = Vec::new();
    let mut matched_spheres = Vec::new();
    let direct_sphere_match = chosen_sphere
        .map(|sphere| religion.sphere_ids.contains(&sphere.sphere_id))
        .unwrap_or(false);
    if direct_sphere_match {
        score += rules.direct_sphere_score;
        matched_spheres.push(input.chosen_sphere_id.clone().unwrap_or_default());
        breakdown.push(component(
            "direct_sphere",
            rules.direct_sphere_score,
            "chosen sphere is inside deity spheres",
        ));
    }
    if religion.sphere_ids.contains(&role.primary_sphere_id) {
        score += rules.role_sphere_score;
        breakdown.push(component(
            "role_primary_sphere",
            rules.role_sphere_score,
            "role primary sphere is inside deity spheres",
        ));
    }
    let mut missing_required_tags = Vec::new();
    for tag in &religion.required_theme_tags {
        if tags.contains(tag) {
            score += rules.required_tag_score;
            breakdown.push(component(
                format!("required_tag.{tag}"),
                rules.required_tag_score,
                "required religion tag is present",
            ));
        } else {
            missing_required_tags.push(tag.clone());
        }
    }
    let mut forbidden_tags_present = Vec::new();
    for tag in &religion.forbidden_theme_tags {
        if tags.contains(tag) {
            score -= rules.forbidden_tag_penalty;
            forbidden_tags_present.push(tag.clone());
            breakdown.push(component(
                format!("forbidden_tag.{tag}"),
                -rules.forbidden_tag_penalty,
                "forbidden religion tag is present",
            ));
        }
    }
    matched_spheres.sort();
    let class_requires_empowerment = religion
        .empowerment_class_ids
        .iter()
        .any(|class_id| class_id == &input.class_id);
    let religion_fit_accepted = direct_sphere_match
        && missing_required_tags.is_empty()
        && forbidden_tags_present.is_empty()
        && score >= rules.minimum_religion_fit_score;
    let grade = if religion_fit_accepted {
        "accepted"
    } else if direct_sphere_match {
        "needs_role_development"
    } else {
        "sphere_mismatch"
    };
    ReligionThemeMatchReadback {
        religion_id: religion.religion_id.clone(),
        deity_id: religion.deity_id.clone(),
        deity_name: religion.deity_name.clone(),
        class_requires_empowerment,
        direct_sphere_match,
        score,
        grade: grade.to_string(),
        religion_fit_accepted,
        empowerment_fit_accepted: class_requires_empowerment && religion_fit_accepted,
        matched_sphere_ids: matched_spheres,
        missing_required_tags,
        forbidden_tags_present,
        score_breakdown: breakdown,
    }
}

fn role_sphere_match(chosen_sphere_id: Option<&str>, role: &RoleThemeDefinition) -> String {
    match chosen_sphere_id {
        Some(id) if id == role.primary_sphere_id => "primary".to_string(),
        Some(id)
            if role
                .supporting_sphere_ids
                .iter()
                .any(|support| support == id) =>
        {
            "supporting".to_string()
        }
        Some(_) => "mismatch".to_string(),
        None => "unselected".to_string(),
    }
}

fn sorted_role_tags(role: &RoleThemeDefinition) -> Vec<String> {
    let tags: BTreeSet<String> = role
        .required_theme_tags
        .iter()
        .chain(role.optional_theme_tags.iter())
        .cloned()
        .collect();
    tags.into_iter().collect()
}

fn collect_catalog_tags(catalog: &ThemeCatalog, tags: &mut BTreeSet<String>) {
    for sphere in &catalog.spheres {
        tags.extend(sphere.theme_tags.iter().cloned());
    }
    for role in &catalog.roles {
        tags.extend(role.required_theme_tags.iter().cloned());
        tags.extend(role.optional_theme_tags.iter().cloned());
    }
    for religion in &catalog.religions {
        tags.extend(religion.required_theme_tags.iter().cloned());
        tags.extend(religion.forbidden_theme_tags.iter().cloned());
    }
}

fn sorted_vec(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

fn component(
    component: impl Into<String>,
    delta: i32,
    reason: impl Into<String>,
) -> ThemeScoreComponent {
    ThemeScoreComponent {
        component: component.into(),
        delta,
        reason: reason.into(),
    }
}

fn catalog_payload(readback: &ThemeCatalogReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "catalog_id": readback.catalog_id,
        "sphere_count": readback.sphere_count,
        "role_count": readback.role_count,
        "religion_count": readback.religion_count,
        "theme_tag_count": readback.theme_tag_count,
        "deity_sphere_edge_count": readback.deity_sphere_edge_count,
        "sphere_ids": readback.sphere_ids,
        "role_ids": readback.role_ids,
        "religion_ids": readback.religion_ids,
        "input_sha256": readback.input_sha256,
    })
}

fn character_payload(readback: &CharacterThemeReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "character_id": readback.character_id,
        "class_id": readback.class_id,
        "level": readback.level,
        "chosen_sphere_id": readback.chosen_sphere_id,
        "role_id": readback.role_id,
        "desired_religion_id": readback.desired_religion_id,
        "sphere_deadline_level": readback.sphere_deadline_level,
        "sphere_selection_required": readback.sphere_selection_required,
        "role_sphere_match": readback.role_sphere_match,
        "effective_theme_tags": readback.effective_theme_tags,
        "religion_match": readback.religion_match,
        "catalog_sha256": readback.catalog_sha256,
        "input_sha256": readback.input_sha256,
        "rules_sha256": readback.rules_sha256,
    })
}
