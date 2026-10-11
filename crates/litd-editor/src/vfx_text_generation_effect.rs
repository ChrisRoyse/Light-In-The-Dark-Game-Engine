use litd_engine::{
    EngineError,
    math::{Aabb, Vec3},
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint, VfxEffectAsset,
        VfxEmitter, VfxGradient, VfxGradientStop, VfxInitializeModule, VfxOutputModule, VfxRange,
        VfxShape, VfxSpawnModule, VfxUpdateModule, VfxVelocity, validate_effect,
    },
};

use super::{VfxParticleBudget, VfxTextElement, VfxTextPattern, VfxTextRegion, VfxTextTaxonomy};

pub(super) fn build_effect(
    taxonomy: &VfxTextTaxonomy,
    budget: &VfxParticleBudget,
) -> Result<VfxEffectAsset, EngineError> {
    let color = element_color(&taxonomy.element);
    let effect = VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id(taxonomy),
        display_name: display_name(taxonomy),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "generated_primary".to_string(),
            display_name: "Generated Primary".to_string(),
            max_particles: budget.emitted_max_particles,
            local_bounds: bounds_for(taxonomy)?,
            spawn: spawn_modules(taxonomy, budget),
            initialize: initialize_modules(taxonomy, color),
            update: update_modules(taxonomy, color),
            output: output_modules(taxonomy, color),
        }],
    };
    validate_effect(&effect)?;
    Ok(effect)
}

fn spawn_modules(taxonomy: &VfxTextTaxonomy, budget: &VfxParticleBudget) -> Vec<VfxSpawnModule> {
    let burst = (budget.emitted_max_particles / 8).clamp(128, 4096);
    let rate = match taxonomy.pattern {
        VfxTextPattern::Explosion => 2048.0,
        VfxTextPattern::Aura => 1024.0,
        VfxTextPattern::Bolt | VfxTextPattern::Trail => 768.0,
    };
    vec![
        VfxSpawnModule::ConstantRate {
            particles_per_second: rate,
        },
        VfxSpawnModule::Burst {
            count: burst,
            time_seconds: 0.0,
        },
    ]
}

fn initialize_modules(taxonomy: &VfxTextTaxonomy, color: VfxColor) -> Vec<VfxInitializeModule> {
    vec![
        VfxInitializeModule::Shape(shape_for(taxonomy)),
        VfxInitializeModule::Velocity(velocity_for(taxonomy)),
        VfxInitializeModule::Color { color },
        VfxInitializeModule::Size {
            pixels: VfxRange::new(14.0, 58.0),
        },
        VfxInitializeModule::Lifetime {
            seconds: VfxRange::new(0.45, 2.2),
        },
    ]
}

fn update_modules(taxonomy: &VfxTextTaxonomy, color: VfxColor) -> Vec<VfxUpdateModule> {
    let upward = taxonomy
        .features
        .iter()
        .any(|feature| feature == "rising_embers");
    vec![
        VfxUpdateModule::Acceleration {
            world_acceleration: if upward {
                Vec3::new(0.0, 7.5, 0.0)
            } else {
                Vec3::new(0.0, -1.2, 0.0)
            },
        },
        VfxUpdateModule::Drag { coefficient: 0.22 },
        VfxUpdateModule::Noise {
            amplitude: 0.38,
            frequency: 3.5,
        },
        VfxUpdateModule::ColorOverLife {
            gradient: gradient_for(color),
        },
        VfxUpdateModule::SizeOverLife {
            curve: size_curve(),
        },
    ]
}

fn output_modules(taxonomy: &VfxTextTaxonomy, color: VfxColor) -> Vec<VfxOutputModule> {
    let material = material_for(&taxonomy.element);
    vec![
        VfxOutputModule::Sprite {
            material_id: material.to_string(),
            texture_atlas_slot: atlas_for(taxonomy).to_string(),
            blend_mode: VfxBlendMode::SoftAdditive,
        },
        VfxOutputModule::Light {
            color,
            intensity_lumens: 6400.0,
            radius_meters: 6.0,
        },
    ]
}

fn effect_id(taxonomy: &VfxTextTaxonomy) -> String {
    format!(
        "generated.vfx.{}.{}",
        taxonomy.element.slug(),
        taxonomy.pattern.slug()
    )
}

fn display_name(taxonomy: &VfxTextTaxonomy) -> String {
    format!(
        "Generated {} {}",
        taxonomy.element.title(),
        taxonomy.pattern.title()
    )
}

fn bounds_for(taxonomy: &VfxTextTaxonomy) -> Result<Aabb, EngineError> {
    let extents = match taxonomy.region {
        VfxTextRegion::Point => Vec3::splat(1.5),
        VfxTextRegion::Sphere => Vec3::splat(3.2),
        VfxTextRegion::Cone => Vec3::new(3.0, 3.0, 5.5),
        VfxTextRegion::Line => Vec3::new(1.2, 1.2, 5.5),
    };
    Aabb::from_center_half_extents(Vec3::ZERO, extents)
}

fn shape_for(taxonomy: &VfxTextTaxonomy) -> VfxShape {
    match taxonomy.region {
        VfxTextRegion::Point => VfxShape::Point,
        VfxTextRegion::Sphere => VfxShape::Sphere { radius_meters: 1.8 },
        VfxTextRegion::Cone => VfxShape::Cone {
            radius_meters: 1.6,
            angle_degrees: 42.0,
        },
        VfxTextRegion::Line => VfxShape::Box {
            half_extents_meters: Vec3::new(0.35, 0.35, 3.4),
        },
    }
}

fn velocity_for(taxonomy: &VfxTextTaxonomy) -> VfxVelocity {
    match taxonomy.region {
        VfxTextRegion::Line => VfxVelocity::Directional {
            direction: Vec3::new(0.0, 0.0, 1.0),
            speed_meters_per_second: VfxRange::new(10.0, 28.0),
        },
        _ => VfxVelocity::Radial {
            speed_meters_per_second: VfxRange::new(8.0, 22.0),
        },
    }
}

fn element_color(element: &VfxTextElement) -> VfxColor {
    match element {
        VfxTextElement::Fire => VfxColor::rgba(1.0, 0.44, 0.1, 1.0),
        VfxTextElement::Lightning => VfxColor::rgba(0.45, 0.72, 1.0, 1.0),
        VfxTextElement::Frost => VfxColor::rgba(0.62, 0.9, 1.0, 1.0),
        VfxTextElement::Arcane => VfxColor::rgba(0.78, 0.42, 1.0, 1.0),
        VfxTextElement::Smoke => VfxColor::rgba(0.55, 0.54, 0.5, 0.86),
    }
}

fn gradient_for(color: VfxColor) -> VfxGradient {
    VfxGradient {
        stops: vec![
            VfxGradientStop {
                t: 0.0,
                color: VfxColor::rgba(1.0, 0.92, 0.42, 1.0),
            },
            VfxGradientStop {
                t: 0.45,
                color: VfxColor::rgba(color.r, color.g, color.b, 0.82),
            },
            VfxGradientStop {
                t: 1.0,
                color: VfxColor::rgba(0.03, 0.02, 0.01, 0.0),
            },
        ],
    }
}

fn size_curve() -> VfxCurve {
    VfxCurve {
        points: vec![
            VfxCurvePoint { t: 0.0, value: 0.2 },
            VfxCurvePoint {
                t: 0.34,
                value: 1.0,
            },
            VfxCurvePoint { t: 1.0, value: 0.0 },
        ],
    }
}

fn material_for(element: &VfxTextElement) -> &'static str {
    match element {
        VfxTextElement::Fire => "mat.vfx.fire.additive",
        VfxTextElement::Lightning => "mat.vfx.lightning.additive",
        VfxTextElement::Frost => "mat.vfx.frost.additive",
        VfxTextElement::Arcane => "mat.vfx.arcane.additive",
        VfxTextElement::Smoke => "mat.vfx.smoke.alpha",
    }
}

fn atlas_for(taxonomy: &VfxTextTaxonomy) -> &'static str {
    match (&taxonomy.element, &taxonomy.pattern) {
        (VfxTextElement::Fire, VfxTextPattern::Explosion) => "atlas.vfx.fireball.impact_01",
        (VfxTextElement::Lightning, _) => "atlas.vfx.lightning.bolt_01",
        (VfxTextElement::Frost, _) => "atlas.vfx.frost.shard_01",
        (VfxTextElement::Arcane, _) => "atlas.vfx.arcane.bloom_01",
        (VfxTextElement::Smoke, _) => "atlas.vfx.smoke.puff_01",
        _ => "atlas.vfx.generated.sprite_01",
    }
}

impl VfxTextElement {
    fn slug(&self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Lightning => "lightning",
            Self::Frost => "frost",
            Self::Arcane => "arcane",
            Self::Smoke => "smoke",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::Fire => "Fire",
            Self::Lightning => "Lightning",
            Self::Frost => "Frost",
            Self::Arcane => "Arcane",
            Self::Smoke => "Smoke",
        }
    }
}

impl VfxTextPattern {
    fn slug(&self) -> &'static str {
        match self {
            Self::Explosion => "explosion",
            Self::Bolt => "bolt",
            Self::Aura => "aura",
            Self::Trail => "trail",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::Explosion => "Explosion",
            Self::Bolt => "Bolt",
            Self::Aura => "Aura",
            Self::Trail => "Trail",
        }
    }
}
