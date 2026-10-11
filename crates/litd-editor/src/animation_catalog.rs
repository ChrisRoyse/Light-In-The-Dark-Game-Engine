//! Searchable glTF animation clip and skeleton constellations.

use std::{collections::BTreeMap, fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    asset_products::{AssetProductKind, product_from_vault_record},
    asset_vault::AssetVaultRecord,
};

#[path = "animation_catalog_support.rs"]
mod support;
use support::{classify_rig, clip_tags, matched_terms, query_terms, stable_id};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationClipIndex {
    pub schema: String,
    pub source_of_truth: String,
    pub clip_count: usize,
    pub rig_count: usize,
    pub clips: Vec<AnimationClipConstellation>,
    pub rigs: Vec<AnimationRigProfile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationClipConstellation {
    pub constellation_id: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub clip_index: usize,
    pub name: String,
    pub duration_ms: u32,
    pub frame_count: u32,
    pub channel_count: usize,
    pub sampler_count: usize,
    pub target_summary: String,
    pub tags: Vec<String>,
    pub rig: AnimationRigProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationRigProfile {
    pub rig_id: String,
    pub compatibility_key: String,
    pub rig_kind: AnimationRigKind,
    pub skin_count: usize,
    pub joint_count: usize,
    pub root_joint: String,
    pub joint_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationRigKind {
    SkinnedBiped,
    SkinnedNonBiped,
    UnskinnedNodeAnimation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationClipQuery {
    pub description: String,
    pub min_frame_count: Option<u32>,
    pub max_frame_count: Option<u32>,
    pub rig: Option<AnimationRigFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationRigFilter {
    pub compatibility_key: String,
    pub exact_rig_id: Option<String>,
    pub allow_retarget: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationClipSearchResult {
    pub query: AnimationClipQuery,
    pub match_count: usize,
    pub matches: Vec<AnimationClipSearchHit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationClipSearchHit {
    pub clip: AnimationClipConstellation,
    pub score: i32,
    pub matched_terms: Vec<String>,
    pub retargetable: bool,
}

pub fn build_animation_clip_index_from_records(
    records: &[AssetVaultRecord],
) -> Result<AnimationClipIndex, EngineError> {
    if records.is_empty() {
        return invalid_animation_catalog("records", "must contain at least one asset record");
    }
    let mut clips = Vec::new();
    let mut rigs = BTreeMap::<String, AnimationRigProfile>::new();
    for record in records {
        let (rig, mut record_clips) = read_record_animation_constellations(record)?;
        rigs.entry(rig.rig_id.clone()).or_insert(rig);
        clips.append(&mut record_clips);
    }
    clips.sort_by(|left, right| {
        left.source_relative_path
            .cmp(&right.source_relative_path)
            .then_with(|| left.clip_index.cmp(&right.clip_index))
    });
    Ok(AnimationClipIndex {
        schema: "litd.animation_clip_index.v1".to_string(),
        source_of_truth: "asset vault glTF/GLB files + glTF skins/animations".to_string(),
        clip_count: clips.len(),
        rig_count: rigs.len(),
        clips,
        rigs: rigs.into_values().collect(),
    })
}

pub fn search_animation_clips(
    index: &AnimationClipIndex,
    query: AnimationClipQuery,
) -> Result<AnimationClipSearchResult, EngineError> {
    validate_query(&query)?;
    let terms = query_terms(&query.description);
    let mut matches = Vec::new();
    for clip in &index.clips {
        let Some(retargetable) = rig_matches(clip, query.rig.as_ref()) else {
            continue;
        };
        if let Some(min) = query.min_frame_count
            && clip.frame_count < min
        {
            continue;
        }
        if let Some(max) = query.max_frame_count
            && clip.frame_count > max
        {
            continue;
        }
        let matched_terms = matched_terms(clip, &terms);
        if !terms.is_empty() && matched_terms.len() != terms.len() {
            continue;
        }
        let score = (matched_terms.len() as i32 * 100)
            - query
                .min_frame_count
                .map(|target| clip.frame_count.abs_diff(target) as i32)
                .unwrap_or(0);
        matches.push(AnimationClipSearchHit {
            clip: clip.clone(),
            score,
            matched_terms,
            retargetable,
        });
    }
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| {
                left.clip
                    .source_relative_path
                    .cmp(&right.clip.source_relative_path)
            })
            .then_with(|| left.clip.clip_index.cmp(&right.clip.clip_index))
    });
    Ok(AnimationClipSearchResult {
        query,
        match_count: matches.len(),
        matches,
    })
}

fn read_record_animation_constellations(
    record: &AssetVaultRecord,
) -> Result<(AnimationRigProfile, Vec<AnimationClipConstellation>), EngineError> {
    let verified_sha = verify_record_hash(record)?;
    let product = product_from_vault_record(record)?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_animation_catalog(
            "record.kind",
            "animation catalog accepts only mesh assets",
        );
    }
    let extension = Path::new(&product.source_relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(extension.as_deref(), Some("gltf" | "glb")) {
        return invalid_animation_catalog(
            "record.extension",
            "animation catalog accepts only glTF/GLB mesh assets",
        );
    }
    let (document, buffers, _images) =
        gltf::import(&product.source_absolute_path).map_err(|error| {
            EngineError::UiRenderInvalidConfig {
                field: "editor.animation_catalog.gltf.import".to_string(),
                reason: format!(
                    "{} failed animation import: {error}",
                    product.source_absolute_path
                ),
            }
        })?;
    let rig = rig_profile(&document, &product.source_relative_path, &verified_sha);
    let mut clips = Vec::new();
    for (clip_index, animation) in document.animations().enumerate() {
        let name = animation
            .name()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("animation_{clip_index}"));
        let mut duration_secs = 0.0_f32;
        let mut frame_count = 0_u32;
        let mut targets = Vec::new();
        for channel in animation.channels() {
            let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
            let inputs =
                reader
                    .read_inputs()
                    .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                        field: "editor.animation_catalog.gltf.animation.inputs".to_string(),
                        reason: format!(
                            "{} animation '{name}' has no time input",
                            product.source_relative_path
                        ),
                    })?;
            let mut channel_frames = 0_u32;
            for time in inputs {
                if !time.is_finite() || time < 0.0 {
                    return invalid_animation_catalog(
                        "animation.time",
                        format!(
                            "{} animation '{name}' has invalid time",
                            product.source_relative_path
                        ),
                    );
                }
                duration_secs = duration_secs.max(time);
                channel_frames += 1;
            }
            frame_count = frame_count.max(channel_frames);
            targets.push(format!(
                "node{}:{:?}",
                channel.target().node().index(),
                channel.target().property()
            ));
        }
        clips.push(AnimationClipConstellation {
            constellation_id: stable_id(&(
                &product.source_relative_path,
                &verified_sha,
                clip_index,
                &name,
                frame_count,
                &rig.rig_id,
            ))?,
            source_relative_path: product.source_relative_path.clone(),
            source_sha256: verified_sha.clone(),
            clip_index,
            name: name.clone(),
            duration_ms: (duration_secs * 1000.0).round() as u32,
            frame_count,
            channel_count: animation.channels().count(),
            sampler_count: animation.samplers().count(),
            target_summary: targets.join("|"),
            tags: clip_tags(&name, &rig),
            rig: rig.clone(),
        });
    }
    Ok((rig, clips))
}

fn rig_profile(
    document: &gltf::Document,
    source_relative_path: &str,
    source_sha256: &str,
) -> AnimationRigProfile {
    let Some(skin) = document.skins().next() else {
        let rig_id = stable_id(&(source_relative_path, source_sha256, "unskinned")).unwrap();
        return AnimationRigProfile {
            rig_id,
            compatibility_key: "unskinned-node-animation".to_string(),
            rig_kind: AnimationRigKind::UnskinnedNodeAnimation,
            skin_count: 0,
            joint_count: 0,
            root_joint: "none".to_string(),
            joint_names: Vec::new(),
        };
    };
    let joint_names = skin
        .joints()
        .map(|node| {
            node.name()
                .filter(|name| !name.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("node{}", node.index()))
        })
        .collect::<Vec<_>>();
    let root_joint = skin
        .skeleton()
        .and_then(|node| node.name().map(str::to_string))
        .or_else(|| joint_names.first().cloned())
        .unwrap_or_else(|| "unknown".to_string());
    let rig_kind = classify_rig(&joint_names);
    let compatibility_key = match rig_kind {
        AnimationRigKind::SkinnedBiped => "biped".to_string(),
        AnimationRigKind::SkinnedNonBiped => format!("skinned:{}", joint_names.len()),
        AnimationRigKind::UnskinnedNodeAnimation => "unskinned-node-animation".to_string(),
    };
    let rig_id = stable_id(&(&compatibility_key, &root_joint, &joint_names)).unwrap();
    AnimationRigProfile {
        rig_id,
        compatibility_key,
        rig_kind,
        skin_count: document.skins().count(),
        joint_count: joint_names.len(),
        root_joint,
        joint_names,
    }
}

fn rig_matches(
    clip: &AnimationClipConstellation,
    filter: Option<&AnimationRigFilter>,
) -> Option<bool> {
    let Some(filter) = filter else {
        return Some(false);
    };
    if clip.rig.compatibility_key != filter.compatibility_key {
        return None;
    }
    match &filter.exact_rig_id {
        Some(rig_id) if &clip.rig.rig_id == rig_id => Some(false),
        Some(_) if filter.allow_retarget => Some(true),
        Some(_) => None,
        None => Some(false),
    }
}

fn validate_query(query: &AnimationClipQuery) -> Result<(), EngineError> {
    if query.description.trim().is_empty()
        && query.min_frame_count.is_none()
        && query.max_frame_count.is_none()
        && query.rig.is_none()
    {
        return invalid_animation_catalog(
            "query",
            "must include text, frame bounds, or rig filter",
        );
    }
    if let (Some(min), Some(max)) = (query.min_frame_count, query.max_frame_count)
        && min > max
    {
        return invalid_animation_catalog("query.frame_count", "min must be <= max");
    }
    if let Some(rig) = &query.rig
        && rig.compatibility_key.trim().is_empty()
    {
        return invalid_animation_catalog("query.rig", "compatibility_key must be non-empty");
    }
    Ok(())
}

fn verify_record_hash(record: &AssetVaultRecord) -> Result<String, EngineError> {
    let bytes =
        fs::read(&record.absolute_path).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.animation_catalog.source.read".to_string(),
            reason: format!("failed to read {}: {error}", record.absolute_path.display()),
        })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = format!("{:x}", hasher.finalize());
    if hash != record.sha256 {
        return invalid_animation_catalog(
            "source.sha256",
            format!(
                "{} manifest sha256 {} != actual {}",
                record.relative_path, record.sha256, hash
            ),
        );
    }
    Ok(hash)
}

fn invalid_animation_catalog<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.animation_catalog.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "animation_catalog_tests.rs"]
mod tests;
