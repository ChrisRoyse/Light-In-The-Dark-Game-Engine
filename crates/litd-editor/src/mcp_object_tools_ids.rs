//! Stable MCP tool-name identifiers for the object/game-data dispatch surface. Split out of
//! `mcp_object_tools` to keep that dispatch file under the 500-line ratchet; re-exported through
//! `mcp_object_tools` so every `mcp_object_tools::*_TOOL` path is unchanged.

pub const COPY_BASE_OBJECT_TOOL: &str = "litd.editor.objects.copy_base_object";
pub const APPLY_OBJECT_DELTA_TOOL: &str = "litd.editor.objects.apply_object_delta";
pub const UPSERT_GAMEPLAY_TABLES_TOOL: &str = "litd.editor.rpg.upsert_gameplay_tables";
pub const READ_OBJECT_DATA_TOOL: &str = "litd.editor.objects.read_object_data";
/// #829 lowering valve as a scriptable surface: lower a persisted unit to a fingerprinted
/// litd/data TOML artifact.
pub const LOWER_UNIT_TOOL: &str = "litd.editor.objects.lower_unit";
/// Re-read a lowered unit TOML artifact from disk and verify its content fingerprint.
pub const READ_LOWERED_UNIT_TOOL: &str = "litd.editor.objects.read_lowered_unit";
/// #819 ability lowering valve: lower a persisted ability to a fingerprinted litd/data TOML.
pub const LOWER_ABILITY_TOOL: &str = "litd.editor.objects.lower_ability";
/// Re-read a lowered ability TOML artifact from disk and verify its content fingerprint.
pub const READ_LOWERED_ABILITY_TOOL: &str = "litd.editor.objects.read_lowered_ability";
/// #820 hero lowering valve: lower a persisted hero to a fingerprinted litd/data TOML.
pub const LOWER_HERO_TOOL: &str = "litd.editor.objects.lower_hero";
/// Re-read a lowered hero TOML artifact from disk and verify its content fingerprint.
pub const READ_LOWERED_HERO_TOOL: &str = "litd.editor.objects.read_lowered_hero";
/// #821 item lowering valve: lower a persisted WC3 item to a fingerprinted litd/data TOML.
pub const LOWER_ITEM_TOOL: &str = "litd.editor.objects.lower_item";
/// Re-read a lowered item TOML artifact from disk and verify its content fingerprint.
pub const READ_LOWERED_ITEM_TOOL: &str = "litd.editor.objects.read_lowered_item";
/// #822 commerce lowering valve: lower a persisted recipe/shop/drop-table (with whole-document
/// cross-record validation) to a fingerprinted litd/data TOML artifact.
pub const LOWER_SHOP_TOOL: &str = "litd.editor.objects.lower_shop";
/// Re-read a lowered commerce TOML artifact from disk and verify its content fingerprint.
pub const READ_LOWERED_SHOP_TOOL: &str = "litd.editor.objects.read_lowered_shop";
/// #825: lower the whole authored tech tree (graph-validated) through the #829 valve.
pub const LOWER_TECH_REQUIREMENTS_TOOL: &str = "litd.editor.objects.lower_tech_requirements";
/// Re-read a lowered tech-tree TOML artifact from disk and verify its content fingerprint.
pub const READ_LOWERED_TECH_REQUIREMENTS_TOOL: &str =
    "litd.editor.objects.read_lowered_tech_requirements";
/// #827: lower the singleton gameplay-constants record through the #829 valve.
pub const LOWER_GAMEPLAY_CONSTANTS_TOOL: &str = "litd.editor.objects.lower_gameplay_constants";
/// Re-read a lowered gameplay-constants TOML artifact from disk and verify its content fingerprint.
pub const READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL: &str =
    "litd.editor.objects.read_lowered_gameplay_constants";
/// #823 buffs/effects lowering valve + readback (fingerprinted litd/data buff TOML).
pub const LOWER_BUFF_TOOL: &str = "litd.editor.objects.lower_buff";
pub const READ_LOWERED_BUFF_TOOL: &str = "litd.editor.objects.read_lowered_buff";
/// #824 upgrades lowering valve + readback (fingerprinted litd/data upgrade TOML).
pub const LOWER_UPGRADE_TOOL: &str = "litd.editor.objects.lower_upgrade";
pub const READ_LOWERED_UPGRADE_TOOL: &str = "litd.editor.objects.read_lowered_upgrade";
/// #826 destructible/doodad type lowering valve + readback (one pair covers both kinds).
pub const LOWER_DESTRUCTIBLE_DOODAD_TOOL: &str = "litd.editor.objects.lower_destructible_doodad";
pub const READ_LOWERED_DESTRUCTIBLE_DOODAD_TOOL: &str =
    "litd.editor.objects.read_lowered_destructible_doodad";
/// #828 Game Interface lowering valve + readback (resolved UI customization TOML).
pub const LOWER_GAME_INTERFACE_TOOL: &str = "litd.editor.objects.lower_game_interface";
pub const READ_LOWERED_GAME_INTERFACE_TOOL: &str =
    "litd.editor.objects.read_lowered_game_interface";
