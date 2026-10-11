use std::collections::{BTreeMap, BTreeSet};

#[path = "world_memory_build.rs"]
mod world_memory_build;
#[path = "world_memory_search_guard.rs"]
mod world_memory_search_guard;
#[path = "world_memory_slots.rs"]
mod world_memory_slots;
#[path = "world_memory_store.rs"]
mod world_memory_store;
#[path = "world_memory_validation.rs"]
mod world_memory_validation;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    authoring::{
        AssayBalanceProfile, GroundedAbilityRequest, LoweredAbilityDocument, WardPolicy,
        WorldCorpus, generate_grounded_ability, verify_lowered_ability_toml,
    },
    history::{EditorObject, stable_fingerprint, to_stable_ron},
    map_build::generate_lua_script,
    world_creation::{EditorWorldRecord, validate_world_record},
};

use world_memory_build::{
    MakeConstellationArgs, detect_conflicts, invalid_world_memory, invalid_world_memory_error,
    make_constellation, optional_property_values, push_unique, score_constellation, token_counts,
};
pub use world_memory_search_guard::{
    GUARDED_WORLD_MEMORY_SEARCH_SCHEMA, GuardedWorldMemoryQuery, GuardedWorldMemorySearchLedger,
    GuardedWorldMemorySearchStatus, GuardedWorldMemorySearchVerdict,
    GuardedWorldMemorySearchWriteback, WorldMemorySearchAudience, WorldMemorySearchCalibration,
    build_guarded_world_memory_search_ledger, guarded_query_world_memory,
    parse_guarded_world_memory_search_ledger, read_guarded_world_memory_search_ledger,
    validate_guarded_world_memory_search_ledger, write_guarded_world_memory_search_ledger,
};
pub use world_memory_slots::{
    WorldMemoryFeatureVector, WorldMemorySlotKind, WorldMemorySlotShape, WorldMemorySlotState,
    WorldMemoryTypedSlot, typed_slot, validate_typed_slots,
};
pub use world_memory_store::{
    WorldMemoryAsterReadback, WorldMemoryConstellationAsterReadback,
    parse_world_memory_constellation, parse_world_memory_index, read_world_memory_constellation,
    read_world_memory_index, write_world_memory_index,
};
pub use world_memory_validation::{
    validate_world_memory_constellation, validate_world_memory_index,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WorldMemorySourceKind {
    World,
    Ability,
    Unit,
    Item,
    Quest,
    Place,
    Agent,
    Script,
    Feedback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WorldMemoryLensKind {
    Text,
    Structured,
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryScript {
    pub script_id: String,
    pub language: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthoredWorldMemoryInput {
    pub world: EditorWorldRecord,
    pub abilities: Vec<LoweredAbilityDocument>,
    pub objects: Vec<EditorObject>,
    pub scripts: Vec<WorldMemoryScript>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryIndex {
    pub schema: String,
    pub source_of_truth: String,
    pub world_count: usize,
    pub constellation_count: usize,
    pub constellations: Vec<WorldMemoryConstellation>,
    pub conflicts: Vec<WorldMemoryConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryConstellation {
    pub constellation_id: String,
    pub world_id: String,
    pub source_kind: WorldMemorySourceKind,
    pub source_id: String,
    pub title: String,
    pub source_fingerprint: String,
    pub provenance_refs: Vec<String>,
    pub lenses: BTreeMap<WorldMemoryLensKind, WorldMemoryLens>,
    pub slots: Vec<WorldMemoryTypedSlot>,
    pub reuse_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryLens {
    pub text: String,
    pub terms: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemorySearchHit {
    pub constellation_id: String,
    pub world_id: String,
    pub source_kind: WorldMemorySourceKind,
    pub source_id: String,
    pub title: String,
    pub score: u32,
    pub lens_scores: BTreeMap<WorldMemoryLensKind, u32>,
    pub provenance_refs: Vec<String>,
    pub source_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryConflict {
    pub left_id: String,
    pub right_id: String,
    pub conflict_kind: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryReuseRequest {
    pub source_constellation_id: String,
    pub request_id: String,
    pub seed: u64,
    pub requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldMemoryReuse {
    pub source_constellation_id: String,
    pub target_world_id: String,
    pub lowered_fingerprint: String,
    pub canonical_toml: String,
}

pub fn generated_world_script(world: &EditorWorldRecord) -> Result<WorldMemoryScript, EngineError> {
    validate_world_record(world)?;
    Ok(WorldMemoryScript {
        script_id: format!("{}::generated_lua", world.spec.calyx_document_id),
        language: "lua".to_string(),
        source: generate_lua_script(&world.document)?,
    })
}

pub fn build_world_memory_index(
    worlds: &[AuthoredWorldMemoryInput],
) -> Result<WorldMemoryIndex, EngineError> {
    if worlds.is_empty() {
        return invalid_world_memory("worlds", "world-memory catalog must not be empty");
    }
    let mut constellations = Vec::new();
    let mut seen = BTreeSet::new();
    for input in worlds {
        validate_world_record(&input.world)?;
        if input.scripts.is_empty() {
            return invalid_world_memory(
                "world.scripts",
                "each world requires at least one script",
            );
        }
        let world_id = input.world.spec.calyx_document_id.clone();
        push_unique(&mut constellations, &mut seen, world_constellation(input)?)?;
        for ability in &input.abilities {
            push_unique(
                &mut constellations,
                &mut seen,
                ability_constellation(&world_id, ability)?,
            )?;
        }
        for object in &input.objects {
            if matches!(
                object.kind.as_str(),
                "Unit" | "Item" | "Quest" | "Place" | "Agent"
            ) {
                push_unique(
                    &mut constellations,
                    &mut seen,
                    object_constellation(&world_id, object)?,
                )?;
            }
        }
        for script in &input.scripts {
            push_unique(
                &mut constellations,
                &mut seen,
                script_constellation(&world_id, script)?,
            )?;
        }
    }
    constellations.sort_by(|left, right| left.constellation_id.cmp(&right.constellation_id));
    let conflicts = detect_conflicts(&constellations);
    let index = WorldMemoryIndex {
        schema: "litd.authoring.world_memory.v2.no_flatten_slots".to_string(),
        source_of_truth:
            "EditorWorldRecord + LoweredAbilityDocument + typed EditorObject slots + generated script text"
                .to_string(),
        world_count: worlds.len(),
        constellation_count: constellations.len(),
        constellations,
        conflicts,
    };
    validate_world_memory_index(&index)?;
    Ok(index)
}

pub fn query_world_memory(
    index: &WorldMemoryIndex,
    query: &str,
    limit: usize,
) -> Result<Vec<WorldMemorySearchHit>, EngineError> {
    if limit == 0 || query.trim().is_empty() {
        return invalid_world_memory("query", "query must be non-empty and limit must be > 0");
    }
    let query_terms = token_counts(query);
    if query_terms.is_empty() {
        return invalid_world_memory("query.terms", "query must contain searchable terms");
    }
    let mut hits = index
        .constellations
        .iter()
        .filter_map(|constellation| score_constellation(constellation, &query_terms))
        .collect::<Vec<_>>();
    if hits.is_empty() {
        return invalid_world_memory("query.ood", "query had no grounded world-memory matches");
    }
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.constellation_id.cmp(&right.constellation_id))
    });
    hits.truncate(limit);
    Ok(hits)
}

pub fn reuse_ability_from_memory(
    index: &WorldMemoryIndex,
    request: &WorldMemoryReuseRequest,
    target_corpus: &WorldCorpus,
    assay: &AssayBalanceProfile,
    ward: &WardPolicy,
) -> Result<WorldMemoryReuse, EngineError> {
    let source = index
        .constellations
        .iter()
        .find(|item| item.constellation_id == request.source_constellation_id)
        .ok_or_else(|| {
            invalid_world_memory_error("reuse.source", "source constellation missing")
        })?;
    if source.source_kind != WorldMemorySourceKind::Ability {
        return invalid_world_memory("reuse.source_kind", "reuse source must be an ability");
    }
    let prompt = source.reuse_prompt.as_ref().ok_or_else(|| {
        invalid_world_memory_error("reuse.prompt", "ability reuse prompt missing")
    })?;
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: request.request_id.clone(),
            prompt: prompt.clone(),
            seed: request.seed,
            requested_by: request.requested_by.clone(),
        },
        target_corpus,
        assay,
        ward,
    )?;
    let lowered = outcome
        .lowered
        .ok_or_else(|| invalid_world_memory_error("reuse.lowered", outcome.admission_code))?;
    let document = verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint)?;
    Ok(WorldMemoryReuse {
        source_constellation_id: source.constellation_id.clone(),
        target_world_id: document.ability.world_id,
        lowered_fingerprint: lowered.fingerprint,
        canonical_toml: lowered.canonical_toml,
    })
}

fn world_constellation(
    input: &AuthoredWorldMemoryInput,
) -> Result<WorldMemoryConstellation, EngineError> {
    let world = &input.world;
    let properties = &world.spec.properties;
    make_constellation(MakeConstellationArgs {
        constellation_id: world.spec.calyx_document_id.clone(),
        world_id: world.spec.calyx_document_id.clone(),
        source_kind: WorldMemorySourceKind::World,
        source_id: world.spec.calyx_document_id.clone(),
        title: properties.display_name.clone(),
        source_fingerprint: world.document_fingerprint.clone(),
        provenance_refs: vec![world.summary.clone()],
        reuse_prompt: None,
        lenses: [
            (
                WorldMemoryLensKind::Text,
                format!(
                    "{} {} {}",
                    properties.display_name, properties.description, world.summary
                ),
            ),
            (
                WorldMemoryLensKind::Structured,
                format!(
                    "width {} height {} terrain_cells {} objects {} regions {} tile {}",
                    properties.width_tiles,
                    properties.height_tiles,
                    world.terrain_cell_count,
                    world.document.objects.len(),
                    world.document.regions.len(),
                    properties.initial_tile_id
                ),
            ),
            (WorldMemoryLensKind::Code, to_stable_ron(world)?),
        ],
    })
}

fn ability_constellation(
    world_id: &str,
    ability: &LoweredAbilityDocument,
) -> Result<WorldMemoryConstellation, EngineError> {
    let ability_data = &ability.ability;
    make_constellation(MakeConstellationArgs {
        constellation_id: format!(
            "{}::ability::{}::{}",
            world_id, ability_data.id, ability.provenance.request_id
        ),
        world_id: world_id.to_string(),
        source_kind: WorldMemorySourceKind::Ability,
        source_id: ability_data.id.clone(),
        title: ability_data.name.clone(),
        source_fingerprint: stable_fingerprint(ability)?,
        provenance_refs: ability.provenance.source_refs.clone(),
        reuse_prompt: Some(format!(
            "Create a {} {} {} ability for {} {}.",
            ability_data.faction,
            ability_data.summary,
            ability_data.damage_type,
            ability_data.region,
            ability.provenance.matched_terms.join(" ")
        )),
        lenses: [
            (
                WorldMemoryLensKind::Text,
                format!(
                    "{} {} {} {} {}",
                    ability_data.name,
                    ability_data.summary,
                    ability_data.faction,
                    ability_data.region,
                    ability_data.tags.join(" ")
                ),
            ),
            (
                WorldMemoryLensKind::Structured,
                format!(
                    "damage {} cooldown {} mana {} type {} tags {}",
                    ability_data.damage,
                    ability_data.cooldown_seconds,
                    ability_data.mana_cost,
                    ability_data.damage_type,
                    ability_data.tags.join(" ")
                ),
            ),
            (WorldMemoryLensKind::Code, to_stable_ron(ability)?),
        ],
    })
}

fn object_constellation(
    world_id: &str,
    object: &EditorObject,
) -> Result<WorldMemoryConstellation, EngineError> {
    let kind = match object.kind.as_str() {
        "Unit" => WorldMemorySourceKind::Unit,
        "Item" => WorldMemorySourceKind::Item,
        "Quest" => WorldMemorySourceKind::Quest,
        "Place" => WorldMemorySourceKind::Place,
        "Agent" => WorldMemorySourceKind::Agent,
        other => {
            return invalid_world_memory("object.kind", format!("unsupported object kind {other}"));
        }
    };
    let title = object
        .properties
        .get("name")
        .or_else(|| object.properties.get("display_name"))
        .cloned()
        .unwrap_or_else(|| object.id.clone());
    let structured_text = object
        .properties
        .iter()
        .map(|(key, value)| format!("{key} {value}"))
        .collect::<Vec<_>>()
        .join(" ");
    let narrative_text = object
        .properties
        .iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "name" | "display_name" | "description" | "role" | "faction" | "summary"
            )
        })
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>()
        .join(" ");
    let mut lenses = vec![
        (
            WorldMemoryLensKind::Text,
            format!("{} {} {} {}", object.id, object.kind, title, narrative_text),
        ),
        (
            WorldMemoryLensKind::Structured,
            format!(
                "kind {} parent {:?} {}",
                object.kind, object.parent, structured_text
            ),
        ),
    ];
    if let Some(code) = object
        .properties
        .get("script")
        .or_else(|| object.properties.get("source_code"))
    {
        lenses.push((WorldMemoryLensKind::Code, code.clone()));
    }
    make_constellation(MakeConstellationArgs {
        constellation_id: format!("{}::{:?}::{}", world_id, kind, object.id),
        world_id: world_id.to_string(),
        source_kind: kind,
        source_id: object.id.clone(),
        title,
        source_fingerprint: stable_fingerprint(object)?,
        provenance_refs: optional_property_values(object, "source_ref"),
        reuse_prompt: None,
        lenses,
    })
}

fn script_constellation(
    world_id: &str,
    script: &WorldMemoryScript,
) -> Result<WorldMemoryConstellation, EngineError> {
    make_constellation(MakeConstellationArgs {
        constellation_id: format!("{}::script::{}", world_id, script.script_id),
        world_id: world_id.to_string(),
        source_kind: WorldMemorySourceKind::Script,
        source_id: script.script_id.clone(),
        title: script.script_id.clone(),
        source_fingerprint: stable_fingerprint(script)?,
        provenance_refs: vec![format!("script://{world_id}/{}", script.script_id)],
        reuse_prompt: None,
        lenses: [
            (WorldMemoryLensKind::Text, script.script_id.clone()),
            (
                WorldMemoryLensKind::Structured,
                format!("language {} bytes {}", script.language, script.source.len()),
            ),
            (WorldMemoryLensKind::Code, script.source.clone()),
        ],
    })
}

#[cfg(test)]
#[path = "world_memory_entity_tests.rs"]
mod entity_tests;
#[cfg(test)]
#[path = "world_memory_search_guard_tests.rs"]
mod search_guard_tests;
#[cfg(test)]
#[path = "world_memory_tests.rs"]
mod tests;
