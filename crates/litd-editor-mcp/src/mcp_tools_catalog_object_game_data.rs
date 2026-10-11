//! Object and gameplay-data MCP catalog entries (#802/#933/#972; #818/#829 unit lowering;
//! #819 ability lowering; #820 hero lowering; #821 item lowering;
//! #822 recipe/shop/drop-table commerce lowering; #825 tech-tree lowering;
//! #827 gameplay-constants lowering; #823 buff, #824 upgrade, #826 destructible/doodad,
//! #828 game-interface lowering). Split out of `mcp_tools_catalog` to keep that file under
//! the 500-line ratchet (the same domain-split pattern as `mcp_tools_catalog_trigger_runtime`).

use super::super::{
    McpToolDomainGroup,
    mcp_tools_builders::{DomainSpec, domain, mut_tool, read_tool},
};

pub(super) fn object_game_data_domain() -> McpToolDomainGroup {
    domain(DomainSpec {
        domain_id: "object_game_data",
        title: "Object and gameplay-data tools",
        child_issue: 919,
        source_issues: &[802, 933, 972],
        capability_namespaces: &["editor.objects", "editor.rpg", "editor.economy"],
        gui_surface: "egui://editor.objects",
        mcp_prefixes: &[
            "litd.editor.objects",
            "litd.editor.rpg",
            "litd.editor.economy",
        ],
        default_readback_tool_name: "litd.editor.objects.read_object_data",
        readme_refs: &[
            "docs/readme/04-editor.png",
            "docs/readme/ED03-object-editor.png",
            "docs/readme/GP09-inventory.png",
        ],
        tools: vec![
            mut_tool(
                "editor.objects.copy_base_object",
                "Copy units, abilities, heroes, items, buffs, and upgrades from base data",
                "egui://editor.objects/copy_base_object",
                "litd.editor.objects.read_object_data",
            ),
            mut_tool(
                "editor.objects.apply_object_delta",
                "Apply typed object-editor deltas",
                "egui://editor.objects/apply_object_delta",
                "litd.editor.objects.read_object_data",
            ),
            mut_tool(
                "editor.rpg.upsert_gameplay_tables",
                "Upsert RPG progression, scarcity, and full-loot gameplay tables",
                "egui://editor.objects/upsert_gameplay_tables",
                "litd.editor.objects.read_object_data",
            ),
            mut_tool(
                "editor.rpg.author_race",
                "Author a race straight into the canonical Calyx content vault so it loads at runtime (#1455)",
                "egui://editor.objects/author_race",
                "litd.editor.objects.read_object_data",
            ),
            read_tool(
                "editor.objects.read_object_data",
                "Read object definitions, RPG tables, and economy rules",
                "egui://editor.objects/read_object_data",
            ),
            read_tool(
                "editor.objects.lower_unit",
                "Lower a persisted unit through the #829 valve to a fingerprinted litd/data TOML artifact",
                "egui://editor.objects/lower_unit",
            ),
            read_tool(
                "editor.objects.read_lowered_unit",
                "Re-read a lowered unit TOML artifact from disk and verify its content fingerprint",
                "egui://editor.objects/read_lowered_unit",
            ),
            read_tool(
                "editor.objects.lower_ability",
                "Lower a persisted ability (#819) through the #829 valve to a fingerprinted litd/data TOML artifact",
                "egui://editor.objects/lower_ability",
            ),
            read_tool(
                "editor.objects.read_lowered_ability",
                "Re-read a lowered ability TOML artifact from disk and verify its content fingerprint",
                "egui://editor.objects/read_lowered_ability",
            ),
            read_tool(
                "editor.objects.lower_hero",
                "Lower a persisted hero (#820) through the #829 valve to a fingerprinted litd/data TOML artifact",
                "egui://editor.objects/lower_hero",
            ),
            read_tool(
                "editor.objects.read_lowered_hero",
                "Re-read a lowered hero TOML artifact from disk and verify its content fingerprint",
                "egui://editor.objects/read_lowered_hero",
            ),
            read_tool(
                "editor.objects.lower_item",
                "Lower a persisted WC3 item (class, charges, inventory flags, stat bonuses, item-ability refs) through the #829 valve to a fingerprinted litd/data TOML artifact (#821)",
                "egui://editor.objects/lower_item",
            ),
            read_tool(
                "editor.objects.read_lowered_item",
                "Re-read a lowered item TOML artifact from disk and verify its content fingerprint (#821)",
                "egui://editor.objects/read_lowered_item",
            ),
            read_tool(
                "editor.objects.lower_shop",
                "Lower a persisted recipe/shop/creep-drop-table with whole-document cross-record validation (dangling item refs and recipe cycles fail loud) to a fingerprinted litd/data TOML artifact (#822)",
                "egui://editor.objects/lower_shop",
            ),
            read_tool(
                "editor.objects.read_lowered_shop",
                "Re-read a lowered commerce TOML artifact from disk and verify its content fingerprint (#822)",
                "egui://editor.objects/read_lowered_shop",
            ),
            read_tool(
                "editor.objects.lower_tech_requirements",
                "Lower the authored tech tree through the #829 valve to a graph-validated, fingerprinted litd/data TOML (dangling refs and requirement cycles fail loud)",
                "egui://editor.objects/lower_tech_requirements",
            ),
            read_tool(
                "editor.objects.read_lowered_tech_requirements",
                "Re-read a lowered tech-tree TOML artifact from disk and verify its content fingerprint",
                "egui://editor.objects/read_lowered_tech_requirements",
            ),
            read_tool(
                "editor.objects.lower_gameplay_constants",
                "Lower the singleton gameplay-constants record through the #829 valve to the full resolved, fingerprinted litd/data TOML",
                "egui://editor.objects/lower_gameplay_constants",
            ),
            read_tool(
                "editor.objects.read_lowered_gameplay_constants",
                "Re-read a lowered gameplay-constants TOML artifact from disk and verify its content fingerprint",
                "egui://editor.objects/read_lowered_gameplay_constants",
            ),
            read_tool(
                "editor.objects.lower_buff",
                "#823: lower a persisted buff/effect through the #829 valve to a fingerprinted TOML artifact",
                "egui://editor.objects/lower_buff",
            ),
            read_tool(
                "editor.objects.read_lowered_buff",
                "Re-read a lowered buff TOML artifact from disk and verify its content fingerprint",
                "egui://editor.objects/read_lowered_buff",
            ),
            read_tool(
                "editor.objects.lower_upgrade",
                "#824: lower a persisted leveled upgrade through the #829 valve to a fingerprinted TOML artifact",
                "egui://editor.objects/lower_upgrade",
            ),
            read_tool(
                "editor.objects.read_lowered_upgrade",
                "Re-read a lowered upgrade TOML artifact from disk and verify its content fingerprint",
                "egui://editor.objects/read_lowered_upgrade",
            ),
            read_tool(
                "editor.objects.lower_destructible_doodad",
                "#826: lower a persisted destructible or doodad type through the #829 valve to a fingerprinted TOML artifact",
                "egui://editor.objects/lower_destructible_doodad",
            ),
            read_tool(
                "editor.objects.read_lowered_destructible_doodad",
                "Re-read a lowered destructible/doodad type TOML artifact from disk and verify its fingerprint",
                "egui://editor.objects/read_lowered_destructible_doodad",
            ),
            read_tool(
                "editor.objects.lower_game_interface",
                "#828: lower a persisted Game Interface record through the #829 valve to a resolved fingerprinted TOML artifact",
                "egui://editor.objects/lower_game_interface",
            ),
            read_tool(
                "editor.objects.read_lowered_game_interface",
                "Re-read a lowered Game Interface TOML artifact from disk and verify its content fingerprint",
                "egui://editor.objects/read_lowered_game_interface",
            ),
        ],
    })
}
