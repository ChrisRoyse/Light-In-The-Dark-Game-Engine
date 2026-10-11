use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use litd_engine::EngineError;

use crate::asset_vault::AssetVaultRecord;

use super::manifest::asset_vault_error;

pub const ISSUE1242_SEMANTIC_STATUS: &str =
    "issue1242_p0_asset_foundation_hash_semantic_coverage_verified";

const REQUIRED_ROLES: &[(&str, usize)] = &[
    ("architecture", 500),
    ("character", 100),
    ("terrain_surface", 250),
    ("foliage", 100),
    ("vfx_light", 40),
    ("grave_ruin", 40),
    ("audio", 100),
];

/// Every role name the classifier can emit. Single source of truth: the #1733 floor
/// selector validates `--floor <role>=<n>` overrides against this set so an unknown role
/// name fails loud instead of silently selecting nothing.
pub const ROLE_NAMES: [&str; 7] = [
    "architecture",
    "audio",
    "character",
    "foliage",
    "grave_ruin",
    "terrain_surface",
    "vfx_light",
];

/// The per-role floor minimums (the issue #1242 editor baseline, reused verbatim by the
/// #1733 floor-targeted restore). Exposed so the Python intake tooling consumes these
/// numbers instead of hard-coding a second, drift-prone copy.
pub fn required_role_floors() -> &'static [(&'static str, usize)] {
    REQUIRED_ROLES
}

/// The minimal metadata needed to classify an asset that may NOT be on disk yet (a catalog
/// candidate the intake is deciding whether to fetch). It mirrors exactly the fields the
/// on-disk [`AssetVaultRecord`] classifier reads, so both the vault-coverage validator and
/// the floor selector share ONE classification implementation and can never drift.
pub struct AssetRoleInput<'a> {
    pub source: &'a str,
    pub asset_id: &'a str,
    pub asset_type: &'a str,
    pub title: &'a str,
    pub relative_path: &'a str,
}

/// Classify a candidate asset into its semantic roles. THE single source of truth for role
/// assignment: [`roles_for_record`] delegates here, and the floor selector calls it directly.
pub fn classify_roles(input: &AssetRoleInput<'_>) -> Vec<String> {
    let key = format!(
        "{} {} {} {} {}",
        input.source, input.asset_id, input.asset_type, input.title, input.relative_path
    )
    .to_ascii_lowercase();
    let mut roles = BTreeSet::new();
    if has_extension(input.relative_path, &["wav", "ogg", "flac"]) || key.contains("audio") {
        roles.insert("audio".to_string());
    }
    add_role_if(
        &mut roles,
        "character",
        contains_any(
            &key,
            &[
                "barbarian",
                "character",
                "hero",
                "knight",
                "mage",
                "skeleton",
                "unit",
                "villager",
                "warrior",
                "wizard",
                "zombie",
            ],
        ),
    );
    add_role_if(
        &mut roles,
        "architecture",
        contains_any(
            &key,
            &[
                "arch", "bastion", "bridge", "building", "castle", "citadel", "dungeon", "gate",
                "house", "ruin", "tower", "village", "wall",
            ],
        ),
    );
    add_role_if(
        &mut roles,
        "grave_ruin",
        contains_any(
            &key,
            &[
                "coffin", "crypt", "grave", "obelisk", "pillar", "ruin", "skull", "statue", "tomb",
            ],
        ),
    );
    add_role_if(
        &mut roles,
        "terrain_surface",
        matches!(input.asset_type, "material" | "surface" | "terrain")
            || contains_any(
                &key,
                &[
                    "cliff", "dirt", "grass", "ground", "lava", "material", "mud", "rock", "sand",
                    "snow", "stone", "surface", "terrain",
                ],
            ),
    );
    add_role_if(
        &mut roles,
        "foliage",
        contains_any(
            &key,
            &[
                "bush", "flower", "foliage", "grass", "leaf", "mushroom", "plant", "shrub", "tree",
            ],
        ),
    );
    add_role_if(
        &mut roles,
        "vfx_light",
        contains_any(
            &key,
            &[
                "beacon", "candle", "crystal", "fire", "glow", "lantern", "lava", "light", "magic",
                "rune", "spell", "torch",
            ],
        ),
    );
    roles.into_iter().collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetVaultSemanticCoverage {
    pub profile_count: usize,
    pub classified_profile_count: usize,
    pub unclassified_profile_count: usize,
    pub tag_count: usize,
    pub role_counts: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetVaultSemanticIndex {
    profiles: Vec<AssetSemanticProfile>,
    role_counts: BTreeMap<String, usize>,
    tag_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSemanticProfile {
    pub relative_path: String,
    pub source: String,
    pub asset_id: String,
    pub asset_type: String,
    pub license: String,
    pub sha256: String,
    pub roles: Vec<String>,
    pub tags: Vec<String>,
}

impl AssetVaultSemanticIndex {
    pub fn from_records(records: &[AssetVaultRecord]) -> Result<Self, EngineError> {
        if records.is_empty() {
            return asset_vault_error(
                "semantic.records",
                "no records available for semantic index",
            );
        }
        let profiles = records.iter().map(profile_for_record).collect::<Vec<_>>();
        let mut role_counts = BTreeMap::new();
        let mut tags = BTreeSet::new();
        for profile in &profiles {
            for role in &profile.roles {
                *role_counts.entry(role.clone()).or_insert(0) += 1;
            }
            tags.extend(profile.tags.iter().cloned());
        }
        Ok(Self {
            profiles,
            role_counts,
            tag_count: tags.len(),
        })
    }

    pub fn validate_issue1242_coverage(&self) -> Result<(), EngineError> {
        let missing = REQUIRED_ROLES
            .iter()
            .filter_map(|(role, minimum)| {
                let count = self.role_count(role);
                (count < *minimum).then(|| format!("{role}={count}<{minimum}"))
            })
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return asset_vault_error(
                "issue1242_semantic_coverage",
                format!(
                    "P0 asset semantic coverage below baseline: missing={} role_counts={}",
                    missing.join(","),
                    self.role_counts_line()
                ),
            );
        }
        Ok(())
    }

    pub fn coverage(&self) -> Result<AssetVaultSemanticCoverage, EngineError> {
        self.validate_issue1242_coverage()?;
        Ok(self.coverage_snapshot())
    }

    /// Semantic role counts without enforcing P0 role floors (#1878 progressive boot).
    /// Full floors remain enforced by [`Self::coverage`] / [`Self::validate_issue1242_coverage`].
    pub fn coverage_snapshot(&self) -> AssetVaultSemanticCoverage {
        let classified_profile_count = self
            .profiles
            .iter()
            .filter(|profile| !profile.roles.is_empty())
            .count();
        AssetVaultSemanticCoverage {
            profile_count: self.profiles.len(),
            classified_profile_count,
            unclassified_profile_count: self.profiles.len() - classified_profile_count,
            tag_count: self.tag_count,
            role_counts: self.role_counts_line(),
        }
    }

    pub fn role_count(&self, role: &str) -> usize {
        self.role_counts.get(role).copied().unwrap_or(0)
    }

    pub fn profiles(&self) -> &[AssetSemanticProfile] {
        &self.profiles
    }

    pub fn profile_for_path(&self, relative_path: &str) -> Option<&AssetSemanticProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.relative_path == relative_path)
    }

    fn role_counts_line(&self) -> String {
        self.role_counts
            .iter()
            .map(|(role, count)| format!("{role}={count}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn profile_for_record(record: &AssetVaultRecord) -> AssetSemanticProfile {
    let roles = roles_for_record(record);
    let tags = tags_for_record(record, &roles);
    AssetSemanticProfile {
        relative_path: record.relative_path.clone(),
        source: record.source.clone(),
        asset_id: record.asset_id.clone(),
        asset_type: record.asset_type.clone(),
        license: record.license.clone(),
        sha256: record.sha256.clone(),
        roles,
        tags,
    }
}

fn roles_for_record(record: &AssetVaultRecord) -> Vec<String> {
    classify_roles(&AssetRoleInput {
        source: &record.source,
        asset_id: &record.asset_id,
        asset_type: &record.asset_type,
        title: &record.title,
        relative_path: &record.relative_path,
    })
}

fn tags_for_record(record: &AssetVaultRecord, roles: &[String]) -> Vec<String> {
    let mut tags = BTreeSet::from([
        format!("source:{}", record.source),
        format!("asset_type:{}", record.asset_type),
        format!("license:{}", record.license),
    ]);
    if let Some(extension) = Path::new(&record.relative_path)
        .extension()
        .and_then(|value| value.to_str())
    {
        tags.insert(format!("format:{}", extension.to_ascii_lowercase()));
    }
    tags.extend(roles.iter().map(|role| format!("role:{role}")));
    tags.into_iter().collect()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn add_role_if(roles: &mut BTreeSet<String>, role: &str, condition: bool) {
    if condition {
        roles.insert(role.to_string());
    }
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| extensions.contains(&value.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "asset_vault_semantics_tests.rs"]
mod tests;
