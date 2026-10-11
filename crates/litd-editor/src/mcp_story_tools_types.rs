use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::world_graph_navigation::WorldGraphTraverseQuery;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryCharacter {
    pub character_id: String,
    pub display_name: String,
    pub role: String,
    pub voice_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueTree {
    pub dialogue_id: String,
    pub title: String,
    pub nodes: Vec<DialogueNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueNode {
    pub node_id: String,
    pub speaker_id: String,
    pub line: String,
    pub choices: Vec<DialogueChoice>,
    pub quest_effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueChoice {
    pub choice_id: String,
    pub text: String,
    pub target_node_id: Option<String>,
    pub conditions: Vec<String>,
    pub effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestArc {
    pub quest_id: String,
    pub title: String,
    pub start_dialogue_id: Option<String>,
    pub completion_dialogue_id: Option<String>,
    pub objectives: Vec<QuestObjective>,
    pub rewards: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestObjective {
    pub objective_id: String,
    pub description: String,
    pub target_ref: String,
    pub required_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CinematicStage {
    pub cinematic_id: String,
    pub title: String,
    pub beats: Vec<CinematicBeat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CinematicBeat {
    pub beat_id: String,
    pub start_ms: u32,
    pub duration_ms: u32,
    pub camera: String,
    pub mood: String,
    pub staging: String,
    pub expression: Option<String>,
    pub dialogue_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreContext {
    pub lore_id: String,
    pub title: String,
    pub body: String,
    pub source_refs: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoreEntryKind {
    Entity,
    Deity,
    Cabal,
    Faction,
    Race,
    Place,
    Event,
    Artifact,
    Timeline,
    History,
    Culture,
}

impl LoreEntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Deity => "deity",
            Self::Cabal => "cabal",
            Self::Faction => "faction",
            Self::Race => "race",
            Self::Place => "place",
            Self::Event => "event",
            Self::Artifact => "artifact",
            Self::Timeline => "timeline",
            Self::History => "history",
            Self::Culture => "culture",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreEntry {
    pub entry_id: String,
    pub kind: LoreEntryKind,
    pub title: String,
    pub summary: String,
    pub body_rich_text: String,
    pub metadata: BTreeMap<String, String>,
    pub source_refs: Vec<String>,
    pub tags: Vec<String>,
    pub links: Vec<LoreEntryLink>,
    pub timeline_markers: Vec<LoreTimelineMarker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreEntryLink {
    pub target_entry_id: String,
    pub relation: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreTimelineMarker {
    pub marker_id: String,
    pub label: String,
    pub order: i32,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoreSurfaceKind {
    Book,
    Note,
    Inscription,
    NpcDialogue,
    CodexEntry,
    RegionDescription,
    EnvironmentalProp,
}

impl LoreSurfaceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Book => "book",
            Self::Note => "note",
            Self::Inscription => "inscription",
            Self::NpcDialogue => "npc_dialogue",
            Self::CodexEntry => "codex_entry",
            Self::RegionDescription => "region_description",
            Self::EnvironmentalProp => "environmental_prop",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreSurface {
    pub surface_id: String,
    pub kind: LoreSurfaceKind,
    pub title: String,
    pub body_rich_text: String,
    pub excerpt: String,
    pub lore_entry_ids: Vec<String>,
    pub world_ref: String,
    pub codex_unlock_id: Option<String>,
    pub discovery_trigger: String,
    pub source_refs: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertDialogueQuestArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub characters: Vec<StoryCharacter>,
    pub dialogue: Option<DialogueTree>,
    pub quests: Vec<QuestArc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageCinematicArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub cinematic: CinematicStage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertLoreContextArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub lore: LoreContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertLoreEntryArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub entries: Vec<LoreEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildLoreRelationshipGraphArgs {
    pub state_path: PathBuf,
    pub graph_ledger_path: PathBuf,
    pub state_id: String,
    pub graph_id: String,
    pub graph_ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub queries: Vec<WorldGraphTraverseQuery>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertLoreSurfaceArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub surfaces: Vec<LoreSurface>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoreCanonQueryKind {
    SemanticSearch,
    ConsistencyCheck,
    GroundedAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCanonClaim {
    pub claim_id: String,
    pub subject_entry_id: String,
    pub relation: Option<String>,
    pub target_entry_id: Option<String>,
    pub metadata_key: Option<String>,
    pub metadata_value: Option<String>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCanonQuery {
    pub query_id: String,
    pub kind: LoreCanonQueryKind,
    pub text: String,
    pub limit: usize,
    pub proposed_claims: Vec<LoreCanonClaim>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryLoreCanonArgs {
    pub state_path: PathBuf,
    pub semantic_vault_path: PathBuf,
    pub query_ledger_path: PathBuf,
    pub state_id: String,
    pub semantic_index_id: String,
    pub query_ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub queries: Vec<LoreCanonQuery>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoreCowriterDraftKind {
    Deity,
    Cabal,
    History,
    Legend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCowriterWardPolicy {
    pub policy_id: String,
    pub min_source_anchors: usize,
    pub min_grounded_terms: usize,
    pub blocked_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreCowriterDraft {
    pub draft_id: String,
    pub kind: LoreCowriterDraftKind,
    pub prompt: String,
    pub requested_entry_id: Option<String>,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoWriteLoreArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub cowriter_ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub cowriter_ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub ward_policy: LoreCowriterWardPolicy,
    pub drafts: Vec<LoreCowriterDraft>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoreTemplateKind {
    Pantheon,
    RivalCabalSet,
    CreationMyth,
    FallenEmpire,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreTemplateWardPolicy {
    pub policy_id: String,
    pub max_generated_entries: usize,
    pub blocked_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoreTemplateRequest {
    pub template_id: String,
    pub kind: LoreTemplateKind,
    pub theme: String,
    pub count: usize,
    pub prefix: String,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerateLoreTemplatesArgs {
    pub state_path: PathBuf,
    pub ledger_path: PathBuf,
    pub template_ledger_path: PathBuf,
    pub state_id: String,
    pub ledger_id: String,
    pub template_ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub ward_policy: LoreTemplateWardPolicy,
    pub templates: Vec<LoreTemplateRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadStoryArgs {
    pub state_path: PathBuf,
}
