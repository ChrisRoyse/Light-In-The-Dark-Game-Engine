use serde::{Deserialize, Serialize};

use super::MortalityEventKind;

pub const RPG_HALL_OF_REMEMBERED_SCHEMA: &str = "litd.rpg.hall_of_remembered.v1";
pub const ASHEN_REACH_HALL_OF_REMEMBERED_POLICY_ID: &str =
    "server.ashen_reach.hall_of_remembered.default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HallOfRememberedPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub built_in_map_required: bool,
    pub persistent_across_server: bool,
    pub auto_populate_from_character_record: bool,
    pub opt_in_player_tributes_only: bool,
    pub browsable_in_galdmere: bool,
    pub browsable_online: bool,
    pub galdmere_surface_id: String,
    pub online_surface_id: String,
    pub minimum_deed_count: usize,
    pub maximum_tribute_count: usize,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HallDeedKind {
    BossSlain,
    LegendaryItemHeld,
    RaidWon,
    RankReached,
    KillFeat,
    WorldEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HallDeed {
    pub deed_id: String,
    pub kind: HallDeedKind,
    pub summary: String,
    pub source_ref: String,
    pub prestige_milli: u16,
}

impl HallDeed {
    pub fn new(
        deed_id: &str,
        kind: HallDeedKind,
        summary: &str,
        source_ref: &str,
        prestige_milli: u16,
    ) -> Self {
        Self {
            deed_id: deed_id.to_string(),
            kind,
            summary: summary.to_string(),
            source_ref: source_ref.to_string(),
            prestige_milli,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HallMemorialDossier {
    pub character_id: String,
    pub character_name: String,
    pub race_id: String,
    pub class_id: String,
    pub faction_id: String,
    pub lifespan_years: u16,
    pub titles: Vec<String>,
    pub deeds: Vec<HallDeed>,
    pub memorable_quotes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HallTribute {
    pub tribute_id: String,
    pub author_player_id: String,
    pub author_character_id: String,
    pub text: String,
    pub opted_in_publicly: bool,
    pub source_ref: String,
}

impl HallTribute {
    pub fn opted_in(
        tribute_id: &str,
        author_player_id: &str,
        author_character_id: &str,
        text: &str,
        source_ref: &str,
    ) -> Self {
        Self {
            tribute_id: tribute_id.to_string(),
            author_player_id: author_player_id.to_string(),
            author_character_id: author_character_id.to_string(),
            text: text.to_string(),
            opted_in_publicly: true,
            source_ref: source_ref.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HallMemorialRecord {
    pub memorial_id: String,
    pub mourning_wall_entry_id: String,
    pub character_id: String,
    pub character_name: String,
    pub race_id: String,
    pub class_id: String,
    pub faction_id: String,
    pub lifespan_years: u16,
    pub final_constitution: i16,
    pub total_deaths: u32,
    pub cause: MortalityEventKind,
    pub witness_id: Option<String>,
    pub epitaph: String,
    pub titles: Vec<String>,
    pub deeds: Vec<HallDeed>,
    pub tributes: Vec<HallTribute>,
    pub memorable_quotes: Vec<String>,
    pub surfaced_in_galdmere: bool,
    pub browsable_online: bool,
    pub persistent_across_server: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub source_issue: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HallMemorialReadback {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub memorial_id: String,
    pub mourning_wall_entry_id: String,
    pub character_id: String,
    pub character_name: String,
    pub race_id: String,
    pub class_id: String,
    pub faction_id: String,
    pub lifespan_years: u16,
    pub deed_count: usize,
    pub title_count: usize,
    pub tribute_count: usize,
    pub memorable_quote_count: usize,
    pub surfaced_in_galdmere: bool,
    pub browsable_online: bool,
    pub persistent_across_server: bool,
    pub built_in_map_required: bool,
    pub grants_map_or_minimap: bool,
    pub grants_gps_markers: bool,
    pub mortality_result_sha256: String,
    pub mortality_policy_sha256: String,
    pub policy_sha256: String,
    pub dossier_sha256: String,
    pub tributes_sha256: String,
    pub result_sha256: String,
    pub record: HallMemorialRecord,
}
