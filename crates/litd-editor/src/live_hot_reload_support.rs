use litd_engine::{EngineError, math::FixedVec3, sim_replay::SimSnapshot};

use crate::{
    history::{EditorDocument, EditorObject},
    play::PlaySession,
};

use super::{
    LiveHotReloadPatch, LiveRuntimeSnapshot, invalid_live_hot_reload, live_hot_reload_error,
};

pub(super) fn runtime_readback(
    player_object_id: &str,
    player_body_id: u32,
    snapshot: &SimSnapshot,
) -> Result<LiveRuntimeSnapshot, EngineError> {
    let body = snapshot
        .bodies
        .iter()
        .find(|body| body.id.index() == player_body_id)
        .ok_or_else(|| live_hot_reload_error("runtime.player", "player body missing"))?;
    Ok(LiveRuntimeSnapshot {
        player_object_id: player_object_id.to_string(),
        player_body_id,
        tick: snapshot.tick,
        state_hash: snapshot.state_hash,
        player_position: body.position,
        body_count: snapshot.bodies.len(),
    })
}

pub(super) fn player_body_id(
    session: &PlaySession,
    player_object_id: &str,
) -> Result<u32, EngineError> {
    session
        .build
        .spawned_bodies
        .iter()
        .find(|binding| binding.object_id == player_object_id)
        .map(|binding| binding.body_id)
        .ok_or_else(|| {
            live_hot_reload_error(
                "player_object_id",
                format!("missing spawned body for {player_object_id}"),
            )
        })
}

pub(super) fn apply_patch_to_document(
    document: &EditorDocument,
    patch: &LiveHotReloadPatch,
) -> Result<EditorDocument, EngineError> {
    let mut next = document.clone();
    match patch {
        LiveHotReloadPatch::UnitStat {
            object_id,
            stat_key,
            value,
            ..
        } => {
            let object = unit_mut(&mut next, object_id)?;
            object
                .properties
                .insert(format!("stat.{stat_key}"), value.clone());
        }
        LiveHotReloadPatch::TriggerFlag {
            object_id,
            trigger_id,
            enabled,
            ..
        } => {
            let object = next.objects.get_mut(object_id).ok_or_else(|| {
                live_hot_reload_error("patch.trigger.object", format!("missing {object_id}"))
            })?;
            object
                .properties
                .insert(format!("trigger.{trigger_id}"), enabled.to_string());
        }
        LiveHotReloadPatch::TextureMaterial {
            object_id,
            material_id,
            ..
        } => {
            let object = unit_mut(&mut next, object_id)?;
            object
                .properties
                .insert("faction".to_string(), material_id.clone());
        }
        LiveHotReloadPatch::DialogueLine {
            object_id, line, ..
        } => {
            let object = next.objects.get_mut(object_id).ok_or_else(|| {
                live_hot_reload_error("patch.dialogue.object", format!("missing {object_id}"))
            })?;
            if object.kind != "Dialogue" {
                return invalid_live_hot_reload(
                    "patch.dialogue.kind",
                    "dialogue line patch target must be a Dialogue object",
                );
            }
            object
                .properties
                .insert("dialogue.line".to_string(), line.clone());
        }
        LiveHotReloadPatch::CinematicSubtitle {
            timeline_id, text, ..
        } => {
            let timeline = next
                .cinematic_timelines
                .get_mut(timeline_id)
                .ok_or_else(|| {
                    live_hot_reload_error(
                        "patch.cinematic.timeline",
                        format!("missing timeline {timeline_id}"),
                    )
                })?;
            if let Some(subtitle) = timeline.subtitles.first_mut() {
                subtitle.text = text.clone();
            } else {
                timeline
                    .properties
                    .insert("subtitle.text".to_string(), text.clone());
            }
        }
        LiveHotReloadPatch::StructuralAddUnit {
            object_id,
            parent_id,
            position,
            ..
        } => {
            let mut object = EditorObject::new(
                object_id,
                "Unit",
                parent_id.clone(),
                litd_engine::math::Transform::from_translation(fixed_to_vec3(*position)),
            );
            object
                .properties
                .insert("structural_preview".to_string(), "true".to_string());
            next.objects.insert(object_id.clone(), object);
        }
    }
    next.validate()?;
    Ok(next)
}

pub(super) fn patch_reflected(
    document: &EditorDocument,
    session: &PlaySession,
    player_body_id: u32,
    patch: &LiveHotReloadPatch,
) -> Result<bool, EngineError> {
    Ok(match patch {
        LiveHotReloadPatch::UnitStat {
            object_id,
            stat_key,
            value,
            ..
        } => {
            document
                .objects
                .get(object_id)
                .and_then(|object| object.properties.get(&format!("stat.{stat_key}")))
                == Some(value)
        }
        LiveHotReloadPatch::TriggerFlag {
            object_id,
            trigger_id,
            enabled,
            ..
        } => {
            document
                .objects
                .get(object_id)
                .and_then(|object| object.properties.get(&format!("trigger.{trigger_id}")))
                == Some(&enabled.to_string())
        }
        LiveHotReloadPatch::TextureMaterial { material_id, .. } => {
            session
                .build
                .presentation
                .instances
                .iter()
                .find(|instance| instance.body_id.index() == player_body_id)
                .map(|instance| instance.render_instance.material_id.as_str())
                == Some(material_id.as_str())
        }
        LiveHotReloadPatch::DialogueLine {
            object_id, line, ..
        } => {
            document
                .objects
                .get(object_id)
                .and_then(|object| object.properties.get("dialogue.line"))
                == Some(line)
        }
        LiveHotReloadPatch::CinematicSubtitle {
            timeline_id, text, ..
        } => document
            .cinematic_timelines
            .get(timeline_id)
            .map(|timeline| {
                timeline
                    .subtitles
                    .first()
                    .map(|s| s.text.as_str() == text.as_str())
                    .unwrap_or_else(|| timeline.properties.get("subtitle.text") == Some(text))
            })
            .unwrap_or(false),
        LiveHotReloadPatch::StructuralAddUnit { .. } => false,
    })
}

fn unit_mut<'a>(
    document: &'a mut EditorDocument,
    object_id: &str,
) -> Result<&'a mut EditorObject, EngineError> {
    let object = document.objects.get_mut(object_id).ok_or_else(|| {
        live_hot_reload_error("patch.unit.object", format!("missing {object_id}"))
    })?;
    if object.kind != "Unit" {
        return invalid_live_hot_reload("patch.unit.kind", "patch target must be a Unit");
    }
    Ok(object)
}

fn fixed_to_vec3(value: FixedVec3) -> litd_engine::math::Vec3 {
    litd_engine::math::Vec3::new(
        value.x.to_f64() as f32,
        value.y.to_f64() as f32,
        value.z.to_f64() as f32,
    )
}
