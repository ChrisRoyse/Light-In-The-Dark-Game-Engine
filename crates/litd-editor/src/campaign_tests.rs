use serde_json::json;

use super::*;

fn metadata() -> CampaignMetadata {
    CampaignMetadata {
        title: "Ashen March".to_string(),
        description: "A three-chapter descent".to_string(),
        difficulty_tiers: vec!["normal".to_string(), "brutal".to_string()],
    }
}

fn shared_hero() -> RuntimeCampaignSharedDatum {
    RuntimeCampaignSharedDatum {
        key: "hero_emberblade".to_string(),
        category: RuntimeSharedObjectCategory::Hero,
        definition: json!({ "base_level": 1, "strength": 12 }),
    }
}

fn chapter(id: &str, order: u32, unlock: &[&str]) -> CampaignChapter {
    CampaignChapter {
        chapter_id: id.to_string(),
        order,
        title: format!("Chapter {id}"),
        map_ref: format!("map_{id}"),
        map_document_path: format!("calyx://world/{id}"),
        unlock_after: unlock.iter().map(|value| value.to_string()).collect(),
        branch_on_victory: Vec::new(),
    }
}

fn valid_campaign() -> Campaign {
    let mut campaign = Campaign::new("camp.ashen", metadata());
    campaign.entry_chapter_id = "ch1".to_string();
    campaign.upsert_chapter(chapter("ch1", 0, &[]));
    campaign.upsert_chapter(chapter("ch2", 1, &["ch1"]));
    campaign.upsert_chapter(chapter("ch3", 2, &["ch2"]));
    campaign.shared_object_data.push(shared_hero());
    campaign.hero_carryover_slots.push(RuntimeCampaignHeroSlot {
        slot_id: "slot_primary".to_string(),
        hero_key: "hero_emberblade".to_string(),
        carries_level: true,
        carries_items: true,
        active_through_chapter: None,
    });
    campaign
}

#[test]
fn valid_campaign_validates() {
    valid_campaign().validate().expect("valid campaign");
}

#[test]
fn entry_chapter_with_predecessor_fails() {
    let mut campaign = valid_campaign();
    campaign.chapters[0].unlock_after = vec!["ch2".to_string()];
    let error = campaign.validate().unwrap_err().to_string();
    assert!(error.contains("predecessor"), "error was {error}");
}

#[test]
fn duplicate_chapter_order_fails() {
    let mut campaign = valid_campaign();
    campaign.chapters[1].order = 0;
    let error = campaign.validate().unwrap_err().to_string();
    assert!(error.contains("order"), "error was {error}");
}

#[test]
fn hero_slot_referencing_non_hero_fails() {
    let mut campaign = valid_campaign();
    campaign.hero_carryover_slots[0].hero_key = "hero_missing".to_string();
    let error = campaign.validate().unwrap_err().to_string();
    assert!(error.contains("hero_missing"), "error was {error}");
}

#[test]
fn precedence_map_local_override_wins() {
    let campaign = valid_campaign();
    // A map that authors an object colliding with the shared hero key overrides it.
    let overriding_map = json!({ "objects": { "hero_emberblade": { "id": "hero_emberblade" } } });
    let resolution = resolve_shared_data_for_chapter(&campaign.shared_object_data, &overriding_map);
    assert_eq!(resolution.len(), 1);
    assert_eq!(resolution[0].winner, "map_local");

    // A map without the collision falls back to the campaign shared datum.
    let plain_map = json!({ "objects": { "grunt": { "id": "grunt" } } });
    let plain = resolve_shared_data_for_chapter(&campaign.shared_object_data, &plain_map);
    assert_eq!(plain[0].winner, "campaign_shared");
}
