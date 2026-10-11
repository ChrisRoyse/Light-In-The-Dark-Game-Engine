use std::{
    fs,
    path::{Path, PathBuf},
};

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_engine::{
    EngineError,
    ai::{
        CalyxObjectVisualSignal, ObjectMicroAgentActivationGrade, ObjectMicroAgentBudget,
        ObjectMicroAgentDefinition, ObjectMicroAgentInput, ObjectMicroAgentState,
        ObjectMicroAgentVisualIntent, evaluate_object_micro_agents,
        validate_object_micro_agent_readback,
    },
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
    math::{Aabb, Vec3},
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint, VfxEffectAsset,
        VfxEmitter, VfxGradient, VfxGradientStop, VfxInitializeModule, VfxOutputModule, VfxRange,
        VfxShape, VfxSpawnModule, VfxUpdateModule, VfxVelocity,
    },
    sim::SimBodyId,
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const OBJECT_COUNT: usize = 256;
pub const MODEL_BYTES: u64 = 5 * 1024 * 1024;
pub const MODEL_ID: &str = "model.embedded.lantern_glow_5mb";
pub const VFX_EFFECT_ID: &str = "vfx.micro.lantern_glow";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedModelBlob {
    pub path: PathBuf,
    pub model_id: String,
    pub bytes: u64,
    pub sha256: String,
    pub media_type: String,
}

pub fn write_embedded_model_blob(run_dir: &Path) -> std::io::Result<EmbeddedModelBlob> {
    let path = run_dir.join("embedded-model-lantern-glow-5mib.gguf.bin");
    let mut bytes = Vec::with_capacity(MODEL_BYTES as usize);
    for index in 0..MODEL_BYTES as usize {
        bytes.push(((index.wrapping_mul(31).wrapping_add(7)) & 0xFF) as u8);
    }
    fs::write(&path, &bytes)?;
    let metadata = fs::metadata(&path)?;
    Ok(EmbeddedModelBlob {
        path,
        model_id: MODEL_ID.to_string(),
        bytes: metadata.len(),
        sha256: sha256_hex(&bytes),
        media_type: "application/vnd.litd.embedded-object-micro-agent".to_string(),
    })
}

pub fn content(
    kind: EngineContentKind,
    id: &str,
    display: &str,
    payload: Value,
) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display.to_string(),
        payload,
        source: EngineContentSource {
            path: format!("synthetic://issue971/{id}"),
            author: "issue971-fsv".to_string(),
            source_hash: sha256_hex(format!("issue971:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue971_object_micro_agent_anchor".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

pub fn lantern_glow_effect() -> Result<VfxEffectAsset, EngineError> {
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: VFX_EFFECT_ID.to_string(),
        display_name: "Embedded Lantern Glow".to_string(),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "emitter.lantern.glow".to_string(),
            display_name: "Lantern Glow Pulse".to_string(),
            max_particles: 8,
            local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(0.8))?,
            spawn: vec![VfxSpawnModule::Burst {
                count: 8,
                time_seconds: 0.0,
            }],
            initialize: vec![
                VfxInitializeModule::Shape(VfxShape::Sphere {
                    radius_meters: 0.18,
                }),
                VfxInitializeModule::Velocity(VfxVelocity::Radial {
                    speed_meters_per_second: VfxRange::new(0.05, 0.35),
                }),
                VfxInitializeModule::Color {
                    color: VfxColor::rgba(1.0, 0.74, 0.26, 1.0),
                },
                VfxInitializeModule::Size {
                    pixels: VfxRange::new(5.0, 13.0),
                },
                VfxInitializeModule::Lifetime {
                    seconds: VfxRange::new(0.25, 0.9),
                },
            ],
            update: vec![
                VfxUpdateModule::Drag { coefficient: 0.15 },
                VfxUpdateModule::ColorOverLife {
                    gradient: VfxGradient {
                        stops: vec![
                            VfxGradientStop {
                                t: 0.0,
                                color: VfxColor::rgba(1.0, 0.84, 0.42, 1.0),
                            },
                            VfxGradientStop {
                                t: 1.0,
                                color: VfxColor::rgba(0.9, 0.32, 0.04, 0.0),
                            },
                        ],
                    },
                },
                VfxUpdateModule::SizeOverLife {
                    curve: VfxCurve {
                        points: vec![
                            VfxCurvePoint {
                                t: 0.0,
                                value: 0.35,
                            },
                            VfxCurvePoint { t: 0.5, value: 1.0 },
                            VfxCurvePoint { t: 1.0, value: 0.0 },
                        ],
                    },
                },
            ],
            output: vec![
                VfxOutputModule::Sprite {
                    material_id: "mat.object.lantern.awake".to_string(),
                    texture_atlas_slot: "atlas.objects.lantern.glow_01".to_string(),
                    blend_mode: VfxBlendMode::SoftAdditive,
                },
                VfxOutputModule::Light {
                    color: VfxColor::rgba(1.0, 0.72, 0.24, 1.0),
                    intensity_lumens: 120.0,
                    radius_meters: 1.25,
                },
            ],
        }],
    })
}

pub fn object_micro_agent_input(
    model: &EmbeddedModelBlob,
    provenance_refs: &[String],
) -> ObjectMicroAgentInput {
    ObjectMicroAgentInput {
        run_id: "issue971-object-visual-swarm".to_string(),
        world_id: "world.ashen_reach".to_string(),
        budget: ObjectMicroAgentBudget::hundreds_spawnable_defaults(),
        definitions: (0..OBJECT_COUNT)
            .map(|index| definition(index, model))
            .collect(),
        states: (0..OBJECT_COUNT)
            .map(|index| ObjectMicroAgentState {
                agent_id: format!("object.agent.{index:03}"),
                spawned_instances: 0,
                last_activation_tick: None,
            })
            .collect(),
        signals: (0..OBJECT_COUNT)
            .map(|index| signal(index, provenance_refs))
            .collect(),
    }
}

pub fn edge_cases(input: &ObjectMicroAgentInput) -> Vec<Value> {
    vec![
        edge_small_model(input),
        edge_duplicate_agent(input),
        edge_ward_blocked(input),
        edge_particle_budget(input),
        edge_tampered_fingerprint(input),
    ]
}

fn definition(index: usize, model: &EmbeddedModelBlob) -> ObjectMicroAgentDefinition {
    ObjectMicroAgentDefinition {
        agent_id: format!("object.agent.{index:03}"),
        object_id: format!("object.lantern.{index:03}"),
        display_name: format!("Lantern {index:03}"),
        embedded_model_id: model.model_id.clone(),
        embedded_model_bytes: model.bytes,
        embedded_model_sha256: model.sha256.clone(),
        target_body: SimBodyId::from_raw(10_000 + index as u32),
        trigger_tags: vec![
            "lantern".to_string(),
            "proximity".to_string(),
            "awake_visual".to_string(),
        ],
        ward_tags: vec![
            "lantern".to_string(),
            "proximity".to_string(),
            "awake_visual".to_string(),
        ],
        visual_intent: match index % 4 {
            0 => ObjectMicroAgentVisualIntent::Glow,
            1 => ObjectMicroAgentVisualIntent::Flicker,
            2 => ObjectMicroAgentVisualIntent::ParticlePulse,
            _ => ObjectMicroAgentVisualIntent::AwakenObject,
        },
        vfx_effect_id: VFX_EFFECT_ID.to_string(),
        material_variant: "mat.object.lantern.awake".to_string(),
        particle_budget: 8,
    }
}

fn signal(index: usize, provenance_refs: &[String]) -> CalyxObjectVisualSignal {
    let mut refs = provenance_refs.to_vec();
    refs.push(format!("calyx://signal/object/{index:03}"));
    CalyxObjectVisualSignal {
        signal_id: format!("signal.object.{index:03}"),
        world_id: "world.ashen_reach".to_string(),
        tick: 100 + index as u64,
        strength_milli: 900,
        tags: vec![
            "lantern".to_string(),
            "proximity".to_string(),
            "awake_visual".to_string(),
        ],
        summary: "Calyx proximity signal wakes embedded lantern micro-agent".to_string(),
        target_object_id: Some(format!("object.lantern.{index:03}")),
        provenance_refs: refs,
    }
}

fn edge_small_model(input: &ObjectMicroAgentInput) -> Value {
    let mut edge_input = single_input(input);
    edge_input.definitions[0].embedded_model_bytes = MODEL_BYTES - 1;
    invalid_edge(
        "small_model_rejected",
        evaluate_object_micro_agents(&edge_input),
        "definition.embedded_model_bytes",
        "5-20 MiB",
        input_sha256(&edge_input),
        input_sha256(&edge_input),
    )
}

fn edge_duplicate_agent(input: &ObjectMicroAgentInput) -> Value {
    let mut edge_input = subset_input(input, 2);
    edge_input.definitions[1].agent_id = edge_input.definitions[0].agent_id.clone();
    invalid_edge(
        "duplicate_agent_rejected",
        evaluate_object_micro_agents(&edge_input),
        "input.agent_id",
        "unique",
        input_sha256(&edge_input),
        input_sha256(&edge_input),
    )
}

fn edge_ward_blocked(input: &ObjectMicroAgentInput) -> Value {
    let mut edge_input = single_input(input);
    edge_input.signals[0].tags.push("nuclear".to_string());
    let before = input_sha256(&edge_input);
    match evaluate_object_micro_agents(&edge_input) {
        Ok(readback) => {
            let matched = readback.accepted_count == 0
                && readback.lowered_effects.is_empty()
                && readback.decisions[0].grade == ObjectMicroAgentActivationGrade::WardBlocked
                && before == input_sha256(&edge_input);
            json!({
                "label": "ward_blocked_signal",
                "matched": matched,
                "accepted_count": readback.accepted_count,
                "grade": readback.decisions[0].grade,
                "before_input_sha256": before,
                "after_input_sha256": input_sha256(&edge_input),
                "input_unchanged": before == input_sha256(&edge_input)
            })
        }
        Err(error) => unexpected_edge(
            "ward_blocked_signal",
            error,
            before,
            input_sha256(&edge_input),
        ),
    }
}

fn edge_particle_budget(input: &ObjectMicroAgentInput) -> Value {
    let mut edge_input = subset_input(input, 4);
    edge_input.budget.max_particles_per_tick = 16;
    let before = input_sha256(&edge_input);
    match evaluate_object_micro_agents(&edge_input) {
        Ok(readback) => {
            let matched = readback.accepted_count == 2
                && readback.total_particles_reserved == 16
                && readback.decisions[2].grade
                    == ObjectMicroAgentActivationGrade::ParticleBudgetExceeded
                && before == input_sha256(&edge_input);
            json!({
                "label": "particle_budget_exhausted",
                "matched": matched,
                "accepted_count": readback.accepted_count,
                "total_particles_reserved": readback.total_particles_reserved,
                "grade": readback.decisions[2].grade,
                "before_input_sha256": before,
                "after_input_sha256": input_sha256(&edge_input),
                "input_unchanged": before == input_sha256(&edge_input)
            })
        }
        Err(error) => unexpected_edge(
            "particle_budget_exhausted",
            error,
            before,
            input_sha256(&edge_input),
        ),
    }
}

fn edge_tampered_fingerprint(input: &ObjectMicroAgentInput) -> Value {
    let edge_input = single_input(input);
    let before = input_sha256(&edge_input);
    let mut readback = evaluate_object_micro_agents(&edge_input).expect("valid one-agent input");
    readback.readback_fingerprint = "0000".to_string();
    match validate_object_micro_agent_readback(&readback) {
        Err(EngineError::AiReadbackMismatch { field, .. }) => {
            let matched = field == "object_micro_agent.readback_fingerprint"
                && before == input_sha256(&edge_input);
            json!({
                "label": "tampered_readback_fingerprint",
                "matched": matched,
                "field": field,
                "before_input_sha256": before,
                "after_input_sha256": input_sha256(&edge_input),
                "input_unchanged": before == input_sha256(&edge_input)
            })
        }
        other => json!({
            "label": "tampered_readback_fingerprint",
            "matched": false,
            "unexpected": format!("{other:?}"),
            "before_input_sha256": before,
            "after_input_sha256": input_sha256(&edge_input)
        }),
    }
}

fn invalid_edge<T: std::fmt::Debug>(
    label: &str,
    result: Result<T, EngineError>,
    expected_field: &str,
    expected_reason: &str,
    before_input_sha256: String,
    after_input_sha256: String,
) -> Value {
    match result {
        Err(EngineError::AiInvalidConfig { field, reason }) => {
            let matched = field.ends_with(expected_field)
                && reason.contains(expected_reason)
                && before_input_sha256 == after_input_sha256;
            json!({
                "label": label,
                "matched": matched,
                "field": field,
                "reason": reason,
                "before_input_sha256": before_input_sha256,
                "after_input_sha256": after_input_sha256,
                "input_unchanged": before_input_sha256 == after_input_sha256
            })
        }
        other => json!({
            "label": label,
            "matched": false,
            "unexpected": format!("{other:?}"),
            "before_input_sha256": before_input_sha256,
            "after_input_sha256": after_input_sha256
        }),
    }
}

fn unexpected_edge(
    label: &str,
    error: EngineError,
    before_input_sha256: String,
    after_input_sha256: String,
) -> Value {
    json!({
        "label": label,
        "matched": false,
        "error": error.to_string(),
        "before_input_sha256": before_input_sha256,
        "after_input_sha256": after_input_sha256
    })
}

fn subset_input(input: &ObjectMicroAgentInput, count: usize) -> ObjectMicroAgentInput {
    let mut edge_input = input.clone();
    edge_input.definitions.truncate(count);
    edge_input.states.truncate(count);
    edge_input.signals.truncate(count);
    edge_input
}

fn single_input(input: &ObjectMicroAgentInput) -> ObjectMicroAgentInput {
    subset_input(input, 1)
}

fn input_sha256(input: &ObjectMicroAgentInput) -> String {
    sha256_hex(&serde_json::to_vec(input).expect("object input serializes"))
}
