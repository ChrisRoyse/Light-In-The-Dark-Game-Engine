//! Parser: captured itemsearch record text -> validated [`ItemSpec`] (#1688).
//!
//! The itemsearch dump (`carrionfields.net/itemsearch`, seed captured
//! 2026-07-12) publishes one record per item as a header line plus zero or more
//! `- Modifies <affect> by <n>` lines:
//!
//! ```text
//! Name: a pair of blackened leather gloves Area: Pine Forest Item Type: armor \
//!   Wear: hands Material: leather Level: 51 Weight: 1 lb 10 oz Flags: giant_only \
//!   Amor Class: Pierce: 13 Bash: 7 Slash: 13 Magic: 3 Element: 14
//! - Modifies armor class by 10
//! ```
//!
//! Every field is transcribed factually. Fields itemsearch does NOT publish are
//! recorded as such (value = 0 with a note; weapon dice are reconstructed from
//! the published average, exact-average-preserving). Any record that cannot be
//! mapped to a valid spec is a LOUD itemized failure -- never a silent drop.

use super::item_affects::ItemAffect;
use super::item_affects::ItemAffectKind;
use super::item_kinds::{ItemKind, ItemKindPayload};
use super::item_specs::validate_item_spec;
use super::item_specs_types::*;
use super::item_weapons::{DamageDice, DamageType, WeaponClass, WeaponProfile};
use super::item_wear_slots::{WearGating, WearSlot};

/// A single record's raw header + affect lines, split out of the dump.
struct RawRecord<'a> {
    index: usize,
    header: &'a str,
    affect_lines: Vec<&'a str>,
}

/// Split a dump into `Name:`-delimited records. A record is a header line that
/// starts with `Name:` followed by its `- Modifies ...` affect lines (blank
/// lines are ignored). Text before the first `Name:` (the itemsearch page
/// chrome / filter form) is skipped.
fn split_records(dump: &str) -> Vec<RawRecord<'_>> {
    let mut records = Vec::new();
    for line in dump.lines() {
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix("Name:") {
            records.push(RawRecord {
                index: records.len() + 1,
                header: header.trim(),
                affect_lines: Vec::new(),
            });
        } else if trimmed.starts_with("- Modifies")
            && let Some(current) = records.last_mut()
        {
            current.affect_lines.push(trimmed);
        }
    }
    records
}

/// Parse a whole dump into `(specs, failures, id_collisions)`. Deterministic:
/// records are processed in dump order and id collisions are resolved by a
/// stable numeric suffix.
pub(super) fn parse_dump(
    dump: &str,
) -> (
    Vec<ItemSpec>,
    Vec<super::item_ingest_types::ItemIngestFailure>,
    usize,
) {
    use super::item_ingest_types::ItemIngestFailure;
    use std::collections::BTreeMap;

    let raws = split_records(dump);
    let mut specs = Vec::new();
    let mut failures = Vec::new();
    let mut id_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut collisions = 0usize;

    for raw in &raws {
        let name = read_field(raw.header, "", " Area:")
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("<unnamed record {}>", raw.index));
        match record_to_spec(raw, &mut id_counts, &mut collisions) {
            Ok(spec) => specs.push(spec),
            Err(reason) => failures.push(ItemIngestFailure {
                record_index: raw.index,
                item_name: name,
                reason,
            }),
        }
    }
    (specs, failures, collisions)
}

/// Map one raw record to a validated [`ItemSpec`], or a loud reason string.
fn record_to_spec(
    raw: &RawRecord<'_>,
    id_counts: &mut std::collections::BTreeMap<String, usize>,
    collisions: &mut usize,
) -> Result<ItemSpec, String> {
    let header = raw.header;
    let name = read_field(header, "", " Area:").ok_or("missing Name field")?;
    let area = read_field(header, " Area:", " Item Type:").ok_or("missing Area field")?;
    let type_str = read_field(header, " Item Type:", " Wear:").ok_or("missing Item Type field")?;
    let wear_str = read_field(header, " Wear:", " Material:").ok_or("missing Wear field")?;
    let material = read_field(header, " Material:", " Level:").ok_or("missing Material field")?;
    let level_str = read_field(header, " Level:", " Weight:").ok_or("missing Level field")?;
    // Weight runs to the first optional block (Flags / Amor Class / Weapon Class)
    // or end of line.
    let weight_str = read_field_to_any(
        header,
        " Weight:",
        &[" Flags:", " Amor Class:", " Weapon Class:"],
    )
    .ok_or("missing Weight field")?;

    let kind = parse_item_kind(type_str)?;
    let slot = WearSlot::from_id(wear_str.trim())
        .ok_or_else(|| format!("unknown wear slot '{}'", wear_str.trim()))?;
    if slot.is_racial() {
        return Err(format!(
            "racial slot '{}' requires anatomy-race gating (#1683) not published by itemsearch; \
             defer to manual authoring",
            slot.as_id()
        ));
    }
    let wear = WearGating {
        slot,
        gating_race_ids: Vec::new(),
    };

    let level: u16 = level_str
        .trim()
        .parse()
        .map_err(|_| format!("unparseable Level '{}'", level_str.trim()))?;
    let weight = parse_weight(weight_str)?;
    let flags = parse_flags(header);
    let affects = parse_affects(&raw.affect_lines)?;

    let armor = read_amor_class(header).transpose()?;
    let weapon = read_weapon(header).transpose()?;

    let kind_payload = kind_payload_for(kind, &weapon, &armor)?;
    let item_id = unique_id(&slugify(name), id_counts, collisions);
    let source_area_id = format!("area.cf.{}", slugify(area));
    let keywords = derive_keywords(name);

    let identify = identify_for(kind);
    let spec = ItemSpec {
        schema: RPG_ITEM_SPEC_SCHEMA.to_string(),
        item_id,
        display_name: name.to_string(),
        keywords,
        source_area_id,
        source_area_name: area.to_string(),
        kind_payload,
        wear,
        level,
        weight,
        // itemsearch does not publish an item's coin value; 0 marks "unpublished".
        value_coin: 0,
        material: material.trim().to_string(),
        flags,
        armor,
        weapon,
        affects,
        condition: ItemCondition {
            breakage_prone: false,
            repairable: true,
        },
        restring: None,
        identify,
        source_issue: 1688,
        canon_issues: vec![936, 989, 1688],
        mechanics_source_url: "https://carrionfields.net/itemsearch".to_string(),
        implementation_notes: vec![
            "Ingested from the itemsearch seed captured 2026-07-12 (#1688); stats transcribed \
             factually."
                .to_string(),
            "CF short-description carried verbatim as display_name; #1670 name originalization is \
             a separate authored pass."
                .to_string(),
            "value_coin=0 marks a field itemsearch does not publish; weapon dice reconstruct the \
             published average exactly (1d(2*avg-1))."
                .to_string(),
        ],
    };
    // Validate LOUD -- an unknown material/flag, a kind/payload mismatch, or a
    // degenerate field surfaces here as an itemized failure, never a bad row.
    validate_item_spec(&spec).map_err(|error| error.to_string())?;
    Ok(spec)
}

fn parse_item_kind(type_str: &str) -> Result<ItemKind, String> {
    Ok(match type_str.trim() {
        "weapon" => ItemKind::Weapon,
        "armor" => ItemKind::Armor,
        "clothing" => ItemKind::Clothing,
        "treasure" => ItemKind::Treasure,
        "instrument" => ItemKind::Instrument,
        other => return Err(format!("unknown Item Type '{other}'")),
    })
}

fn kind_payload_for(
    kind: ItemKind,
    weapon: &Option<WeaponProfile>,
    armor: &Option<ArmorClass>,
) -> Result<ItemKindPayload, String> {
    Ok(match kind {
        ItemKind::Weapon => {
            if weapon.is_none() {
                return Err("weapon record without a Weapon Class block".to_string());
            }
            ItemKindPayload::Weapon
        }
        ItemKind::Armor => {
            if armor.is_none() {
                return Err("armor record without an Amor Class block".to_string());
            }
            ItemKindPayload::Armor
        }
        ItemKind::Clothing => ItemKindPayload::Clothing,
        ItemKind::Treasure => ItemKindPayload::Treasure,
        // itemsearch does not publish an instrument's bard power modifier.
        ItemKind::Instrument => ItemKindPayload::Instrument { power_modifier: 0 },
        other => {
            return Err(format!(
                "item kind '{}' is not part of the itemsearch browsable set",
                other.as_id()
            ));
        }
    })
}

fn identify_for(kind: ItemKind) -> IdentifyReveal {
    let mut identify_reveals = vec![
        ItemRevealProperty::Level,
        ItemRevealProperty::Weight,
        ItemRevealProperty::Value,
        ItemRevealProperty::Material,
        ItemRevealProperty::StatAffects,
        ItemRevealProperty::Flags,
    ];
    match kind {
        ItemKind::Weapon => identify_reveals.push(ItemRevealProperty::WeaponDamage),
        ItemKind::Armor => identify_reveals.push(ItemRevealProperty::ArmorClass),
        _ => {}
    }
    IdentifyReveal {
        identify_reveals,
        deep_lore_reveals: vec![
            ItemRevealProperty::SpecialProc,
            ItemRevealProperty::AlignmentRestriction,
        ],
    }
}

fn parse_weight(weight_str: &str) -> Result<ItemWeight, String> {
    // Format: "<pounds> lb <ounces> oz".
    let cleaned = weight_str.trim();
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    let malformed = || format!("unparseable Weight '{cleaned}'");
    if parts.len() != 4 || parts[1] != "lb" || parts[3] != "oz" {
        return Err(malformed());
    }
    let pounds: u16 = parts[0].parse().map_err(|_| malformed())?;
    let ounces: u16 = parts[2].parse().map_err(|_| malformed())?;
    Ok(ItemWeight { pounds, ounces })
}

fn parse_flags(header: &str) -> Vec<String> {
    match read_field_to_any(header, " Flags:", &[" Amor Class:", " Weapon Class:"]) {
        Some(raw) => raw.split_whitespace().map(|s| s.to_string()).collect(),
        None => Vec::new(),
    }
}

fn parse_affects(lines: &[&str]) -> Result<Vec<ItemAffect>, String> {
    let mut affects = Vec::new();
    for line in lines {
        // "- Modifies <affect name> by <n>".
        let body = line
            .trim()
            .strip_prefix("- Modifies")
            .ok_or_else(|| format!("malformed affect line '{line}'"))?
            .trim();
        let (name, value) = body
            .rsplit_once(" by ")
            .ok_or_else(|| format!("malformed affect line '{line}'"))?;
        let affect_id = name.trim().to_ascii_lowercase().replace(' ', "_");
        let kind = ItemAffectKind::from_id(&affect_id)
            .ok_or_else(|| format!("unknown affect '{}'", name.trim()))?;
        let modifier: i32 = value
            .trim()
            .parse()
            .map_err(|_| format!("unparseable affect modifier '{}'", value.trim()))?;
        affects.push(ItemAffect {
            affect: kind,
            modifier,
        });
    }
    Ok(affects)
}

fn read_amor_class(header: &str) -> Option<Result<ArmorClass, String>> {
    let block = read_field_to_any(header, " Amor Class:", &[" Weapon Class:"])?;
    Some(parse_amor_class(block))
}

fn parse_amor_class(block: &str) -> Result<ArmorClass, String> {
    // "Pierce: 13 Bash: 7 Slash: 13 Magic: 3 Element: 14".
    let malformed = || format!("unparseable Amor Class '{}'", block.trim());
    let pierce = read_field(block, "Pierce:", " Bash:").ok_or_else(malformed)?;
    let bash = read_field(block, " Bash:", " Slash:").ok_or_else(malformed)?;
    let slash = read_field(block, " Slash:", " Magic:").ok_or_else(malformed)?;
    let magic = read_field(block, " Magic:", " Element:").ok_or_else(malformed)?;
    let element = read_field_to_any(block, " Element:", &[]).ok_or_else(malformed)?;
    let parse = |s: &str| s.trim().parse::<i16>().map_err(|_| malformed());
    Ok(ArmorClass {
        pierce: parse(pierce)?,
        bash: parse(bash)?,
        slash: parse(slash)?,
        magic: parse(magic)?,
        element: parse(element)?,
    })
}

fn read_weapon(header: &str) -> Option<Result<WeaponProfile, String>> {
    let block = read_field_to_any(header, " Weapon Class:", &[])?;
    Some(parse_weapon(block))
}

fn parse_weapon(block: &str) -> Result<WeaponProfile, String> {
    // "<class> Weapon Damage: average <avg> (<noun>)".
    let class_str = read_field(block, "", " Weapon Damage:")
        .ok_or_else(|| format!("unparseable Weapon block '{}'", block.trim()))?;
    let damage = read_field_to_any(block, " Weapon Damage:", &[])
        .ok_or_else(|| format!("unparseable Weapon Damage '{}'", block.trim()))?;
    // itemsearch renders a two-handed weapon as "<class> (two-handed)".
    let (class_name, two_handed) = match class_str.trim().strip_suffix(" (two-handed)") {
        Some(base) => (base.trim(), true),
        None => (class_str.trim(), false),
    };
    let weapon_class = WeaponClass::from_id(class_name)
        .ok_or_else(|| format!("unknown weapon class '{class_name}'"))?;
    // "average <avg> (<noun>)".
    let after_avg = damage
        .trim()
        .strip_prefix("average ")
        .ok_or_else(|| format!("unparseable Weapon Damage '{}'", damage.trim()))?;
    let (avg_str, noun_paren) = after_avg
        .split_once(" (")
        .ok_or_else(|| format!("unparseable Weapon Damage '{}'", damage.trim()))?;
    let avg: u16 = avg_str
        .trim()
        .parse()
        .map_err(|_| format!("unparseable weapon average '{}'", avg_str.trim()))?;
    if avg == 0 {
        return Err("weapon average of 0 cannot roll damage".to_string());
    }
    let noun = noun_paren.trim_end().trim_end_matches(')').trim();
    let damage_type = DamageType::from_damage_noun(noun)
        .ok_or_else(|| format!("unknown weapon damage noun '{noun}' (not in helpfile 2304)"))?;
    // itemsearch publishes only the average, not the Nd S split; 1d(2*avg-1)
    // reproduces the published average exactly (average of 1dS = (S+1)/2).
    let dice = DamageDice {
        count: 1,
        sides: 2 * avg - 1,
    };
    Ok(WeaponProfile {
        weapon_class,
        dice,
        damage_type,
        damage_noun: noun.to_string(),
        two_handed,
    })
}

/// Read the substring after `start` up to `end` (or `""` start = from the
/// beginning). Returns trimmed text, or `None` when the markers are absent.
fn read_field<'a>(hay: &'a str, start: &str, end: &str) -> Option<&'a str> {
    read_field_to_any(hay, start, &[end])
}

/// Read the substring after `start` up to whichever of `ends` appears first, or
/// to end-of-string when `ends` is empty / none present.
fn read_field_to_any<'a>(hay: &'a str, start: &str, ends: &[&str]) -> Option<&'a str> {
    let from = if start.is_empty() {
        0
    } else {
        let idx = hay.find(start)?;
        idx + start.len()
    };
    let rest = &hay[from..];
    let mut cut = rest.len();
    for end in ends {
        if let Some(pos) = rest.find(end) {
            cut = cut.min(pos);
        }
    }
    Some(rest[..cut].trim())
}

/// Lowercase alphanumeric slug with single underscores between word runs.
fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut prev_us = true; // suppress leading underscore
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us {
            out.push('_');
            prev_us = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("item");
    }
    out
}

fn unique_id(
    slug: &str,
    id_counts: &mut std::collections::BTreeMap<String, usize>,
    collisions: &mut usize,
) -> String {
    let base = format!("item.cf.{slug}");
    let count = id_counts.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        *collisions += 1;
        format!("{base}_{}", *count)
    }
}

fn derive_keywords(name: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "a", "an", "the", "some", "of", "with", "and", "pair", "set", "to",
    ];
    let mut keywords: Vec<String> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() > 1)
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect();
    keywords.dedup();
    if keywords.is_empty() {
        keywords.push(slugify(name));
    }
    keywords.truncate(6);
    keywords
}
