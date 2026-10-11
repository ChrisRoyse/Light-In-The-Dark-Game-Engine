use std::collections::BTreeSet;

use litd_engine::EngineError;
use serde::Serialize;

use super::{AnimationClipConstellation, AnimationRigKind, AnimationRigProfile};

pub(super) fn classify_rig(joint_names: &[String]) -> AnimationRigKind {
    let names = joint_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let has = |needle: &str| names.iter().any(|name| name.contains(needle));
    if has("head")
        && (has("torso") || has("chest") || has("spine"))
        && has("upperleg.l")
        && has("upperleg.r")
    {
        AnimationRigKind::SkinnedBiped
    } else {
        AnimationRigKind::SkinnedNonBiped
    }
}

pub(super) fn clip_tags(name: &str, rig: &AnimationRigProfile) -> Vec<String> {
    let mut tags = tokenize(name);
    match rig.rig_kind {
        AnimationRigKind::SkinnedBiped => tags.push("biped".to_string()),
        AnimationRigKind::SkinnedNonBiped => tags.push("skinned".to_string()),
        AnimationRigKind::UnskinnedNodeAnimation => tags.push("unskinned".to_string()),
    }
    for tag in action_tags(&tags) {
        tags.push(tag);
    }
    tags.sort();
    tags.dedup();
    tags
}

pub(super) fn matched_terms(clip: &AnimationClipConstellation, terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .filter(|term| clip.tags.iter().any(|tag| tag == *term))
        .cloned()
        .collect()
}

pub(super) fn query_terms(description: &str) -> Vec<String> {
    tokenize(description)
        .into_iter()
        .filter(|token| {
            !matches!(
                token.as_str(),
                "a" | "an" | "the" | "for" | "clip" | "clips" | "frame" | "frames" | "by"
            ) && !token.chars().all(|ch| ch.is_ascii_digit())
        })
        .collect()
}

pub(super) fn stable_id<T: Serialize>(value: &T) -> Result<String, EngineError> {
    let text = serde_json::to_string(value).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: error.to_string(),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}

fn action_tags(tokens: &[String]) -> Vec<String> {
    let token_set = tokens.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut tags = Vec::new();
    if ["kick", "punch", "shoot", "sword", "slash"]
        .iter()
        .any(|token| token_set.contains(token))
    {
        tags.push("attack".to_string());
    }
    if ["run", "walk", "jump"]
        .iter()
        .any(|token| token_set.contains(token))
    {
        tags.push("locomotion".to_string());
    }
    if ["dance", "hello", "yes", "no"]
        .iter()
        .any(|token| token_set.contains(token))
    {
        tags.push("emote".to_string());
    }
    tags
}

fn tokenize(value: &str) -> Vec<String> {
    let mut expanded = String::new();
    for ch in value.chars() {
        if ch == '_' || ch == '-' || ch.is_whitespace() {
            expanded.push(' ');
        } else if ch.is_ascii_uppercase()
            && expanded
                .chars()
                .last()
                .is_some_and(|last| last.is_ascii_lowercase())
        {
            expanded.push(' ');
            expanded.push(ch.to_ascii_lowercase());
        } else {
            expanded.push(ch.to_ascii_lowercase());
        }
    }
    expanded
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>()
}
