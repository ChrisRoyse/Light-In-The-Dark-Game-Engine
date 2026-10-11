use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    GenerateLoreTemplatesArgs, LoreEntry, LoreEntryKind, LoreEntryLink, LoreTemplateKind,
    LoreTemplateRequest, LoreTemplateWardPolicy, LoreTimelineMarker, McpStoryToolCallOutput,
    invalid_mcp_story,
    mcp_story_tools_store::{
        McpStoryState, McpStoryToolRecord, load_or_create_mcp_story_state,
        validate_mcp_story_state, write_mcp_story_state, write_mcp_story_tool_ledger,
    },
};

#[path = "mcp_story_tools_lore_templates_store.rs"]
mod store;
#[path = "mcp_story_tools_lore_templates_support.rs"]
mod support;

pub use store::{
    LORE_TEMPLATE_LEDGER_SCHEMA, LoreTemplateLedger, LoreTemplateLedgerReadback,
    read_lore_template_ledger, validate_lore_template_ledger,
};
use support::{generated_entry_count, symbol_prefix, template_kind_name, validate_args};

#[path = "mcp_story_tools_lore_templates_args.rs"]
mod mcp_story_tools_lore_templates_args;
use mcp_story_tools_lore_templates_args::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreTemplateWardDecision {
    pub passed: bool,
    pub code: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreTemplateResult {
    pub template_id: String,
    pub kind: String,
    pub status: String,
    pub generated_entry_ids: Vec<String>,
    pub generated_titles: Vec<String>,
    pub lowered_fingerprints: Vec<String>,
    pub ward_decision: LoreTemplateWardDecision,
}

#[derive(Debug, Clone)]
struct TemplateBuild {
    template_id: String,
    kind: LoreTemplateKind,
    entries: Vec<LoreEntry>,
    ward_decision: LoreTemplateWardDecision,
}

pub(super) fn generate_lore_templates_tool(
    route: crate::mcp_tools::McpDomainTool,
    arguments: Value,
) -> Result<McpStoryToolCallOutput, EngineError> {
    super::validate_mutating_route(&route)?;
    let args: GenerateLoreTemplatesArgs =
        super::parse_args(super::GENERATE_LORE_TEMPLATES_TOOL, arguments)?;
    // #1518: confine all three write sinks to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
        ("template_ledger_path", args.template_ledger_path.as_path()),
    ])?;
    super::validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_args(&args)?;

    let state_before = load_or_create_mcp_story_state(&args.state_path, &args.state_id)?;
    let state_sha256_before = state_fingerprint(&state_before)?;
    let mut builds = generate_templates(&args, &state_before)?;
    apply_cross_template_links(&mut builds);
    let mut generated = Vec::new();
    let mut results = Vec::new();
    for build in builds {
        results.push(result_from_build(&build)?);
        generated.extend(build.entries);
    }
    let generated_link_count = generated.iter().map(|entry| entry.links.len()).sum();

    let mut next_state = state_before.clone();
    next_state.lore_entries.extend(generated.clone());
    next_state.update_count += 1;
    validate_mcp_story_state(&next_state)?;
    let state_after = write_mcp_story_state(&args.state_path, &next_state)?;
    let source_of_truth = format!(
        "Calyx Aster story state {} sha256 {} -> {}",
        state_after.path, state_sha256_before, state_after.sha256
    );
    let template_ledger = store::build_ledger(store::TemplatesLedgerArgs {
        ledger_id: args.template_ledger_id.clone(),
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        source_of_truth,
        state_sha256_before,
        state_sha256_after: state_after.sha256.clone(),
        ward_policy: args.ward_policy.clone(),
        generated_link_count,
        results,
    })?;
    let template_readback =
        store::write_lore_template_ledger(&args.template_ledger_path, template_ledger)?;
    let affected_ids = generated
        .iter()
        .map(|entry| entry.entry_id.clone())
        .collect::<Vec<_>>();
    let record = McpStoryToolRecord {
        request_id: args.request_id.clone(),
        requested_by: args.requested_by.clone(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        state_path: args.state_path.display().to_string(),
        state_sha256: state_after.sha256.clone(),
        state_bytes: state_after.bytes,
        action_summary: "generated original lore template starting points".to_string(),
        affected_ids,
        character_count: state_after.character_count,
        dialogue_count: state_after.dialogue_count,
        quest_count: state_after.quest_count,
        cinematic_count: state_after.cinematic_count,
        lore_context_count: state_after.lore_context_count,
        lore_entry_count: state_after.lore_entry_count,
        lore_entry_link_count: state_after.lore_entry_link_count,
        lore_timeline_marker_count: state_after.lore_timeline_marker_count,
        lore_surface_count: state_after.lore_surface_count,
        lore_surface_lore_ref_count: state_after.lore_surface_lore_ref_count,
        lore_surface_codex_unlock_count: state_after.lore_surface_codex_unlock_count,
    };
    let ledger_readback = write_mcp_story_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(super::output(
        route,
        "Calyx Aster story state + story MCP ledger + lore template provenance ledger",
        json!({
            "state_readback": state_after.clone(),
            "lore_template_readback": template_readback
        }),
        Some(state_after),
        Some(ledger_readback),
    ))
}

fn generate_templates(
    args: &GenerateLoreTemplatesArgs,
    state: &McpStoryState,
) -> Result<Vec<TemplateBuild>, EngineError> {
    let mut used_ids = state
        .lore_entries
        .iter()
        .map(|entry| entry.entry_id.clone())
        .collect::<BTreeSet<_>>();
    let mut builds = Vec::with_capacity(args.templates.len());
    for template in &args.templates {
        let mut entries = match template.kind {
            LoreTemplateKind::Pantheon => pantheon_entries(template, &args.ward_policy),
            LoreTemplateKind::RivalCabalSet => cabal_entries(template, &args.ward_policy),
            LoreTemplateKind::CreationMyth => vec![history_entry(
                template,
                &args.ward_policy,
                "creation_myth",
                "Creation Myth",
                0,
            )],
            LoreTemplateKind::FallenEmpire => vec![history_entry(
                template,
                &args.ward_policy,
                "fallen_empire",
                "Fallen Empire",
                1,
            )],
        };
        for entry in &entries {
            if !used_ids.insert(entry.entry_id.clone()) {
                return Err(invalid_mcp_story(
                    "lore_template.entry_id",
                    format!("generated lore entry {} already exists", entry.entry_id),
                ));
            }
        }
        builds.push(TemplateBuild {
            template_id: template.template_id.clone(),
            kind: template.kind,
            entries: std::mem::take(&mut entries),
            ward_decision: ward_pass(template, &args.ward_policy),
        });
    }
    Ok(builds)
}

fn pantheon_entries(
    template: &LoreTemplateRequest,
    ward: &LoreTemplateWardPolicy,
) -> Vec<LoreEntry> {
    let domains = ["dawn", "oaths", "rivers", "embers", "memory", "thresholds"];
    (0..generated_entry_count(template))
        .map(|index| {
            let name = pick(DEITY_NAMES, template.seed, index);
            let domain = domains[(template.seed as usize + index) % domains.len()];
            let title = format!("{name}, Warden of {domain}");
            let suffix = format!("deity_{index}_{}", slug(&name));
            entry(EntryArgs {
        template,
        ward,
        suffix,
        kind: LoreEntryKind::Deity,
        title,
        summary: format!("Original pantheon deity of {domain} shaped by {}.", template.theme),
        body_rich_text: format!("## {name}\n{name} is an original deity from the {} pantheon. Shrines keep {domain} vows, festivals, taboos, and story hooks ready for remixing into quests.", template.theme),
        ordinal: index,
        timeline_markers: Vec::new(),
    })
        })
        .collect()
}

fn cabal_entries(template: &LoreTemplateRequest, ward: &LoreTemplateWardPolicy) -> Vec<LoreEntry> {
    let tactics = ["blackmail", "omens", "relics", "debts", "mirrors"];
    (0..generated_entry_count(template))
        .map(|index| {
            let name = pick(CABAL_NAMES, template.seed, index);
            let tactic = tactics[(template.seed as usize + index) % tactics.len()];
            let title = format!("The {name} Cabal");
            let suffix = format!("cabal_{index}_{}", slug(&name));
            entry(EntryArgs {
        template,
        ward,
        suffix,
        kind: LoreEntryKind::Cabal,
        title,
        summary: format!("Original rival cabal using {tactic} in {}.", template.theme),
        body_rich_text: format!("## The {name} Cabal\nThe cabal contests {} through {tactic}, hidden patrons, public masks, and betrayal hooks a creator can reassign during remix.", template.theme),
        ordinal: index,
        timeline_markers: Vec::new(),
    })
        })
        .collect()
}

fn history_entry(
    template: &LoreTemplateRequest,
    ward: &LoreTemplateWardPolicy,
    suffix: &str,
    title_prefix: &str,
    offset: usize,
) -> LoreEntry {
    let name = pick(HISTORY_NAMES, template.seed, offset);
    let title = format!("{title_prefix}: {name}");
    let entry_id = template_entry_id(template, suffix);
    let marker = LoreTimelineMarker {
        marker_id: format!("{entry_id}_marker"),
        label: format!("{title_prefix} seed"),
        order: ((template.seed + offset as u64) % 10_000) as i32,
        description: format!("{title_prefix} timeline marker for {}", template.theme),
    };
    entry(EntryArgs {
        template,
        ward,
        suffix: suffix.to_string(),
        kind: LoreEntryKind::History,
        title,
        summary: format!("Original {title_prefix} timeline for {}.", template.theme),
        body_rich_text: format!(
            "## {title_prefix}\n{name} gives {} a mythic origin, unresolved cost, surviving rite, and playable contradiction for future canon checks.",
            template.theme
        ),
        ordinal: offset,
        timeline_markers: vec![marker],
    })
}

fn entry(args: EntryArgs<'_>) -> LoreEntry {
    let EntryArgs {
        template,
        ward,
        suffix,
        kind,
        title,
        summary,
        body_rich_text,
        ordinal,
        timeline_markers,
    } = args;
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "generated_by".to_string(),
        "lore_template_generator".to_string(),
    );
    metadata.insert("template_id".to_string(), template.template_id.clone());
    metadata.insert(
        "template_kind".to_string(),
        template_kind_name(template.kind).to_string(),
    );
    metadata.insert("ward_policy_id".to_string(), ward.policy_id.clone());
    metadata.insert("seed".to_string(), template.seed.to_string());
    metadata.insert("ordinal".to_string(), ordinal.to_string());
    LoreEntry {
        entry_id: template_entry_id(template, &suffix),
        kind,
        title,
        summary,
        body_rich_text,
        metadata,
        source_refs: vec!["issue_1054".to_string(), template.template_id.clone()],
        tags: vec![
            "lore_template".to_string(),
            "original_lore".to_string(),
            template_kind_name(template.kind).to_string(),
            symbol_prefix(&template.prefix),
        ],
        links: Vec::new(),
        timeline_markers,
    }
}

fn apply_cross_template_links(builds: &mut [TemplateBuild]) {
    let pantheon = ids_for(builds, LoreTemplateKind::Pantheon);
    let cabals = ids_for(builds, LoreTemplateKind::RivalCabalSet);
    let creation = ids_for(builds, LoreTemplateKind::CreationMyth);
    let empire = ids_for(builds, LoreTemplateKind::FallenEmpire);
    for build in builds {
        let peer_ids: Vec<String> = build
            .entries
            .iter()
            .map(|entry| entry.entry_id.clone())
            .collect();
        for (index, entry) in build.entries.iter_mut().enumerate() {
            match build.kind {
                LoreTemplateKind::Pantheon => {
                    link_cycle(entry, &peer_ids, index, "balances");
                    link_first(entry, &creation, "appears_in");
                    link_first(entry, &empire, "patron_of");
                }
                LoreTemplateKind::RivalCabalSet => {
                    link_cycle(entry, &peer_ids, index, "opposes");
                    link_first(entry, &pantheon, "bargains_with");
                }
                LoreTemplateKind::CreationMyth => {
                    link_first(entry, &pantheon, "names");
                    link_first(entry, &cabals, "foreshadows");
                }
                LoreTemplateKind::FallenEmpire => {
                    link_first(entry, &creation, "descends_from");
                    link_first(entry, &cabals, "splintered_into");
                }
            }
        }
    }
}

fn result_from_build(build: &TemplateBuild) -> Result<LoreTemplateResult, EngineError> {
    let mut lowered = Vec::with_capacity(build.entries.len());
    for entry in &build.entries {
        lowered.push(lowered_fingerprint(
            entry,
            &build.template_id,
            &build.ward_decision,
        )?);
    }
    Ok(LoreTemplateResult {
        template_id: build.template_id.clone(),
        kind: template_kind_name(build.kind).to_string(),
        status: "admitted".to_string(),
        generated_entry_ids: build
            .entries
            .iter()
            .map(|entry| entry.entry_id.clone())
            .collect(),
        generated_titles: build
            .entries
            .iter()
            .map(|entry| entry.title.clone())
            .collect(),
        lowered_fingerprints: lowered,
        ward_decision: build.ward_decision.clone(),
    })
}

fn ids_for(builds: &[TemplateBuild], kind: LoreTemplateKind) -> Vec<String> {
    builds
        .iter()
        .filter(|build| build.kind == kind)
        .flat_map(|build| build.entries.iter().map(|entry| entry.entry_id.clone()))
        .collect()
}

fn link_cycle(entry: &mut LoreEntry, ids: &[String], index: usize, relation: &str) {
    if ids.len() > 1 {
        add_link(entry, &ids[(index + 1) % ids.len()], relation);
    }
}

fn link_first(entry: &mut LoreEntry, ids: &[String], relation: &str) {
    if let Some(target) = ids.first() {
        add_link(entry, target, relation);
    }
}

fn add_link(entry: &mut LoreEntry, target: &str, relation: &str) {
    if entry.entry_id == target
        || entry
            .links
            .iter()
            .any(|link| link.target_entry_id == target && link.relation == relation)
    {
        return;
    }
    entry.links.push(LoreEntryLink {
        target_entry_id: target.to_string(),
        relation: relation.to_string(),
        description: format!("{relation} relationship to {target}"),
    });
}

fn ward_pass(
    template: &LoreTemplateRequest,
    ward: &LoreTemplateWardPolicy,
) -> LoreTemplateWardDecision {
    LoreTemplateWardDecision {
        passed: true,
        code: "LITD-WARD-PASS".to_string(),
        reasons: vec![format!(
            "{} generated {} original entries under policy {}",
            template.template_id,
            generated_entry_count(template),
            ward.policy_id
        )],
    }
}

fn template_entry_id(template: &LoreTemplateRequest, suffix: &str) -> String {
    format!("tpl_{}_{}", symbol_prefix(&template.prefix), suffix)
}

fn pick(table: &[&str], seed: u64, index: usize) -> String {
    table[(seed as usize + index) % table.len()].to_string()
}

fn slug(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn lowered_fingerprint(
    entry: &LoreEntry,
    template_id: &str,
    ward: &LoreTemplateWardDecision,
) -> Result<String, EngineError> {
    serde_json::to_vec(&(entry, template_id, ward))
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "lore-template-lowered-json",
            reason: error.to_string(),
        })
}

fn state_fingerprint(state: &McpStoryState) -> Result<String, EngineError> {
    serde_json::to_vec_pretty(state)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "mcp-story-state-json",
            reason: error.to_string(),
        })
}

const DEITY_NAMES: &[&str] = &[
    "Aerun", "Bellara", "Caelis", "Dorven", "Elyra", "Fenn", "Iriath", "Kael", "Morrow", "Nara",
    "Orun", "Vey",
];
const CABAL_NAMES: &[&str] = &[
    "Glassward",
    "Cinderfold",
    "Mournveil",
    "Starwrit",
    "Bellknife",
    "Veilborne",
    "Ashquill",
    "Ironmoth",
    "Nightledger",
    "Duskspire",
];
const HISTORY_NAMES: &[&str] = &[
    "The First Bell",
    "The Ash Covenant",
    "The River Under Stone",
    "The Crown Without Heirs",
];

#[cfg(test)]
#[path = "mcp_story_tools_lore_templates_tests.rs"]
mod tests;
