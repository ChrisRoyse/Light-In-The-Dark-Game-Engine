//! #1687 FSV: newbie OUTFIT kit at both bands (outfit anywhere at L5, guild-only
//! at L15) persisted + read back through Calyx, the full shop haggle loop with a
//! measured price delta, and loud-fail edges.

use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        HaggleSide, haggle_quote, outfit_kit_content_records, outfit_kit_readback,
        persist_and_lower_rpg_authored_content, shopkeeper_content_records, shopkeeper_readback,
        validate_outfit_kit, validate_shopkeeper,
    },
    rpg_outfit_shop_test_fixture::{
        galdmere_merchant_spec, newbie_outfit_guild_spec, newbie_outfit_lowbie_spec,
    },
};

#[test]
fn issue1687_outfit_at_level_5_is_anywhere_with_maps() {
    let spec = newbie_outfit_lowbie_spec();
    let readback = outfit_kit_readback(&spec).unwrap();
    assert_eq!(readback.band, "lowbie_anywhere");
    assert!(!readback.guild_gated);
    // Soft leather gear: light, weapon, helmet, shield, vest (helpfile 314).
    assert_eq!(readback.gear_count, 5);
    assert_eq!(readback.provision_count, 2);
    assert_eq!(readback.map_count, 3);
    assert!(!readback.breakage_prone);
    // Level 5 is inside the 1-10 anywhere window.
    assert_eq!(
        (readback.level_window_low, readback.level_window_high),
        (1, 10)
    );
    assert!((readback.level_window_low..=readback.level_window_high).contains(&5));

    // Persist + lower + read back; the map grant carries the #944 acquired-map
    // doctrine as an archive copy with no HUD/GPS flags.
    let (store, _vault) = content_store("outfit-lowbie");
    let records = outfit_kit_content_records(&spec).unwrap();
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, 1);
    assert_eq!(lowered.validation.acquired_map_record_count, 1);
    let row = store
        .get_content_for_editor(EngineContentKind::Item, &records[0].id)
        .unwrap();
    assert_eq!(row.payload["surface_kind"], "outfit_kit");
    assert_eq!(row.payload["band"], "lowbie_anywhere");
    assert_eq!(
        row.payload["acquired_map_source"]["source_kind"],
        "archive_copy"
    );
    assert_ne!(row.payload["grants_map_or_minimap"], true);
}

#[test]
fn issue1687_outfit_at_level_15_is_guild_gated_and_shoddy() {
    let spec = newbie_outfit_guild_spec();
    let readback = outfit_kit_readback(&spec).unwrap();
    assert_eq!(readback.band, "guild_only");
    assert!(readback.guild_gated);
    assert!(readback.breakage_prone);
    // No maps re-granted in the guild re-gear (helpfile 314).
    assert_eq!(readback.map_count, 0);
    // Level 15 is inside the 11+ guild window.
    assert_eq!(readback.level_window_low, 11);
    assert!((readback.level_window_low..=readback.level_window_high).contains(&15));

    let (store, _vault) = content_store("outfit-guild");
    let records = outfit_kit_content_records(&spec).unwrap();
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, 1);
    // Guild kit grants no maps -> zero acquired-map records.
    assert_eq!(lowered.validation.acquired_map_record_count, 0);
    let row = store
        .get_content_for_editor(EngineContentKind::Item, &records[0].id)
        .unwrap();
    assert_eq!(row.payload["breakage_prone"], true);
    assert!(row.payload.get("acquired_map_source").is_none());
}

#[test]
fn issue1687_full_shop_loop_with_measured_haggle_delta() {
    let shop = galdmere_merchant_spec();
    let readback = shopkeeper_readback(&shop).unwrap();
    assert_eq!(readback.stock_count, 3);
    assert!(readback.has_fence);
    assert!(readback.is_cartographer);
    assert!(readback.is_city);

    // Buy side, base 100: un-haggled = 100 * 200% = 200.
    // A CHA-25 thief master realises the full 30% cap -> 200 - 60 = 140.
    let thief_buy = haggle_quote(&shop, 100, HaggleSide::PlayerBuys, 25, true, true);
    assert_eq!(thief_buy.swing_percent, 30);
    assert_eq!(thief_buy.quoted_price_coin, 140);
    assert_eq!(thief_buy.swing_coin, -60);

    // An untrained low-CHA adventurer gets NO swing ("guard his treasure").
    let novice_buy = haggle_quote(&shop, 100, HaggleSide::PlayerBuys, 10, false, false);
    assert_eq!(novice_buy.swing_percent, 0);
    assert_eq!(novice_buy.quoted_price_coin, 200);
    assert_eq!(novice_buy.swing_coin, 0);

    // A non-thief who learned the skill realises HALF (merchants resist amateurs).
    let learned_buy = haggle_quote(&shop, 100, HaggleSide::PlayerBuys, 25, true, false);
    assert_eq!(learned_buy.swing_percent, 15);
    assert_eq!(learned_buy.quoted_price_coin, 170);

    // Sell side, base 100: un-haggled payout = 40; a thief master lifts it +30%.
    let thief_sell = haggle_quote(&shop, 100, HaggleSide::PlayerSells, 25, true, true);
    assert_eq!(thief_sell.quoted_price_coin, 52);
    assert_eq!(thief_sell.swing_coin, 12);
    // Haggling strictly widens the player's favour: buy cheaper than sell? No --
    // the delta is what matters: the haggled buy beats the un-haggled buy.
    assert!(thief_buy.quoted_price_coin < novice_buy.quoted_price_coin);

    // Persist + read the merchant back as an Agent row.
    let (store, _vault) = content_store("shop-galdmere");
    let records = shopkeeper_content_records(&shop).unwrap();
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, 1);
    let row = store
        .get_content_for_editor(EngineContentKind::Agent, &records[0].id)
        .unwrap();
    assert_eq!(row.payload["surface_kind"], "shopkeeper");
    assert_eq!(row.payload["fence"]["cities_only"], true);
    assert_eq!(row.payload["barter"]["food_per_item"], 8);
}

#[test]
fn issue1687_outfit_edges_fail_closed() {
    // Lowbie band cannot be breakage-prone (that is the guild gear).
    let mut bad = newbie_outfit_lowbie_spec();
    bad.breakage_prone = true;
    assert_code(validate_outfit_kit(&bad), "LITD-RPG-STATS-INVALID-CONFIG");

    // Guild band cannot re-grant maps (helpfile 314).
    let mut guild_maps = newbie_outfit_guild_spec();
    guild_maps.maps = newbie_outfit_lowbie_spec().maps;
    assert_code(
        validate_outfit_kit(&guild_maps),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Unknown gear slot fails loud.
    let mut bad_slot = newbie_outfit_lowbie_spec();
    bad_slot.gear[0].wear_slot_id = "elbow".to_string();
    assert_code(
        validate_outfit_kit(&bad_slot),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Unknown material fails loud.
    let mut bad_mat = newbie_outfit_lowbie_spec();
    bad_mat.gear[2].material_id = "unobtanium".to_string();
    assert_code(
        validate_outfit_kit(&bad_mat),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

#[test]
fn issue1687_shop_edges_fail_closed() {
    // A shop cannot sell to players below base.
    let mut cheap = galdmere_merchant_spec();
    cheap.buy_markup_percent = 80;
    assert_code(validate_shopkeeper(&cheap), "LITD-RPG-STATS-INVALID-CONFIG");

    // Fence is city-only (helpfile 1426): a non-city keeper cannot fence.
    let mut rural_fence = galdmere_merchant_spec();
    rural_fence.is_city = false;
    assert_code(
        validate_shopkeeper(&rural_fence),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Barter ratios must be the factual helpfile-2262 values.
    let mut bad_barter = galdmere_merchant_spec();
    bad_barter.barter.food_per_item = 99;
    assert_code(
        validate_shopkeeper(&bad_barter),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Haggle is CHARISMA-governed (helpfile 501).
    let mut bad_stat = galdmere_merchant_spec();
    bad_stat.haggle.governing_stat_id = "strength".to_string();
    assert_code(
        validate_shopkeeper(&bad_stat),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}

fn assert_code(result: Result<(), EngineError>, code: &str) {
    assert_eq!(result.unwrap_err().code(), code);
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1687-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
