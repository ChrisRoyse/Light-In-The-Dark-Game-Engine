//! FSV for issue #1524 item 2: prove the render-batch occlusion test stopped being
//! `O(instances x occluders)` and now scales ~linearly via the broad-phase grid.
//!
//! Source of truth = wall time of the occlusion phase at growing scene sizes, plus an
//! exact equality check that the grid culls the identical instance set the old linear
//! scan did (no behavior change). We measure with REAL geometry (instances laid out on
//! a screen grid, large occluder tiles in front of a fraction of them).
//!
//! BEFORE = the old linear scan (`occluders.iter().any(...)` per instance), reproduced
//! here from the public types. AFTER = the real `build_batch_plan`; the occlusion cost
//! is isolated as `time(plan with hzb on) - time(plan with hzb off)`, which cancels the
//! frustum + batching work common to both, leaving only the grid build + queries.

use std::{env, fs, path::PathBuf, time::Instant};

use litd_engine::{
    math::{Aabb, Vec2, Vec3},
    render::batching::{
        BatchingSettings, Frustum, HzbOccluder, RenderInstance, ScreenRect, build_batch_plan,
    },
};

const SIZES: [usize; 3] = [1_000, 4_000, 10_000];
const REPEATS: usize = 25;

fn scene(n: usize) -> (Vec<RenderInstance>, Vec<HzbOccluder>) {
    let side = (n as f64).sqrt().ceil() as usize;
    let step = 1.0 / side as f32;
    let inst_half = step * 0.2;
    let mut instances = Vec::with_capacity(n);
    for i in 0..n {
        let gx = (i % side) as f32;
        let gy = (i / side) as f32;
        let cx = (gx + 0.5) * step;
        let cy = (gy + 0.5) * step;
        let min = Vec2::new(
            (cx - inst_half).clamp(0.0, 1.0),
            (cy - inst_half).clamp(0.0, 1.0),
        );
        let max = Vec2::new(
            (cx + inst_half).clamp(0.0, 1.0),
            (cy + inst_half).clamp(0.0, 1.0),
        );
        instances.push(RenderInstance {
            id: (i as u64) + 1,
            mesh_id: "mesh".to_string(),
            material_id: "mat".to_string(),
            world_bounds: Aabb {
                min: Vec3::new(-0.5, -0.5, -0.5),
                max: Vec3::new(0.5, 0.5, 0.5),
            },
            screen_rect: ScreenRect::new(min, max).expect("valid instance rect"),
            // Half the instances are behind the occluders (depth 0.8), half in front
            // (depth 0.1) so the scan has a realistic mix of hits and misses.
            nearest_depth: if i % 2 == 0 { 0.8 } else { 0.1 },
            index_count: 6,
        });
    }

    // Occluder count grows with the scene (m ~ n / 8), so the old scan is genuinely
    // O(n * m) ~ O(n^2). Each occluder is a large tile covering many instance cells.
    let m = (n / 8).max(8);
    let occ_side = (m as f64).sqrt().ceil() as usize;
    let occ_step = 1.0 / occ_side as f32;
    let occ_half = occ_step * 0.6; // overlap so tiles fully contain interior instances
    let mut occluders = Vec::with_capacity(m);
    for j in 0..m {
        let gx = (j % occ_side) as f32;
        let gy = (j / occ_side) as f32;
        let cx = (gx + 0.5) * occ_step;
        let cy = (gy + 0.5) * occ_step;
        let min = Vec2::new(
            (cx - occ_half).clamp(0.0, 1.0),
            (cy - occ_half).clamp(0.0, 1.0),
        );
        let max = Vec2::new(
            (cx + occ_half).clamp(0.0, 1.0),
            (cy + occ_half).clamp(0.0, 1.0),
        );
        occluders.push(HzbOccluder::new(
            ScreenRect::new(min, max).expect("valid occluder rect"),
            0.2, // nearer than the depth-0.8 instances -> those get occluded
        ));
    }
    (instances, occluders)
}

/// The OLD occlusion behavior: linear scan of every occluder per instance.
fn linear_occluded_ids(
    settings: BatchingSettings,
    occluders: &[HzbOccluder],
    instances: &[RenderInstance],
) -> Vec<u64> {
    let mut out = Vec::new();
    for instance in instances {
        let occluded = settings.hzb_enabled
            && occluders.iter().any(|occluder| {
                occluder.screen_rect.contains(instance.screen_rect)
                    && instance.nearest_depth > occluder.depth + settings.occlusion_bias
            });
        if occluded {
            out.push(instance.id);
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    // Self-cleaning scratch dir (#1535): removed on Drop, so keep `temp` alive.
    let temp = litd_engine::test_temp::TestTempDir::new("issue1524-occlusion");
    let path = match env::var("LITD_ISSUE1524_OCC_STATS") {
        Ok(path) => PathBuf::from(path),
        Err(_) => temp.path().join("issue1524-occlusion-scaling-fsv.txt"),
    };

    let settings_on = BatchingSettings {
        hzb_enabled: true,
        occlusion_bias: 0.001,
        max_draw_batches: 8,
    };
    let settings_off = BatchingSettings {
        hzb_enabled: false,
        ..settings_on
    };
    let frustum = Frustum::orthographic(Aabb {
        min: Vec3::new(-1.0, -1.0, -1.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    })?;

    let mut lines = Vec::new();
    for &n in &SIZES {
        let (instances, occluders) = scene(n);

        // Correctness: grid must cull the exact set the linear scan does.
        let before_ids = linear_occluded_ids(settings_on, &occluders, &instances);
        let plan = build_batch_plan(settings_on, &frustum, &occluders, &instances)?;
        let mut after_ids = plan.occlusion_culled_ids.clone();
        after_ids.sort_unstable();
        let mut before_sorted = before_ids.clone();
        before_sorted.sort_unstable();
        if after_ids != before_sorted {
            return Err(format!(
                "n={n}: grid culled {} instances, linear scan culled {} — sets differ",
                after_ids.len(),
                before_sorted.len()
            )
            .into());
        }
        let culled = after_ids.len();

        // BEFORE timing: the linear occlusion scan alone.
        let t = Instant::now();
        let mut sink = 0usize;
        for _ in 0..REPEATS {
            sink += linear_occluded_ids(settings_on, &occluders, &instances).len();
        }
        let before_ms = t.elapsed().as_secs_f64() * 1e3 / REPEATS as f64;

        // AFTER timing: isolate occlusion as plan(hzb on) - plan(hzb off).
        let t = Instant::now();
        for _ in 0..REPEATS {
            let p = build_batch_plan(settings_on, &frustum, &occluders, &instances)?;
            sink += p.occlusion_culled_ids.len();
        }
        let plan_on_ms = t.elapsed().as_secs_f64() * 1e3 / REPEATS as f64;
        let t = Instant::now();
        for _ in 0..REPEATS {
            let p = build_batch_plan(settings_off, &frustum, &occluders, &instances)?;
            sink += p.visible_instance_ids.len();
        }
        let plan_off_ms = t.elapsed().as_secs_f64() * 1e3 / REPEATS as f64;
        let after_ms = (plan_on_ms - plan_off_ms).max(0.0);
        std::hint::black_box(sink);

        lines.push(format!(
            "n={n} occluders={} culled={culled} before_linear_ms={before_ms:.4} after_grid_ms={after_ms:.4} plan_on_ms={plan_on_ms:.4} plan_off_ms={plan_off_ms:.4}",
            occluders.len(),
        ));
    }

    let stats = format!("{}\n", lines.join("\n"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &stats)?;
    let persisted = fs::read_to_string(&path)?;
    if persisted != stats {
        return Err("issue1524 occlusion FSV stats changed after physical write".into());
    }
    print!("{stats}");
    println!(
        "wrote issue1524 occlusion scaling FSV stats to {}",
        path.display()
    );
    Ok(())
}
