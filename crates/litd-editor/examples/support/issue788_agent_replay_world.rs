use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

const ISSUE: u32 = 788;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Issue788World {
    pub schema: String,
    pub issue: u32,
    pub run_id: String,
    pub world_id: String,
    pub source_of_truth: String,
    pub sim_seed: u64,
    pub ticks_per_second: u32,
    pub nav_width: i32,
    pub sight_range: i32,
    pub path_max_expansions: usize,
    pub attack_damage_fp: i64,
    pub agent: Issue788Agent,
    pub units: Vec<Issue788Unit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Issue788Agent {
    pub agent_id: String,
    pub binding_id: String,
    pub object_id: String,
    pub display_name: String,
    pub deterministic_order_sink: String,
    pub source_id: u32,
    pub faction: String,
    pub prompt: String,
    pub model_id: String,
    pub grounding_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Issue788Unit {
    pub object_id: String,
    pub role: String,
    pub faction: String,
    pub position: [i32; 3],
    pub nav_cell: [i32; 2],
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Issue788WorldReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub world: Issue788World,
}

impl Issue788World {
    fn new(run_id: &str) -> Self {
        Self {
            schema: "litd.editor.issue788.world_source.v1".to_string(),
            issue: ISSUE,
            run_id: run_id.to_string(),
            world_id: "world.issue788.guardian_lane".to_string(),
            source_of_truth: "persisted JSON world source for AI-agent replay FSV".to_string(),
            sim_seed: 788,
            ticks_per_second: 1,
            nav_width: 3,
            sight_range: 8,
            path_max_expansions: 64,
            attack_damage_fp: 25,
            agent: Issue788Agent {
                agent_id: "agent.guardian".to_string(),
                binding_id: "binding.guardian".to_string(),
                object_id: "npc.guardian".to_string(),
                display_name: "Guardian".to_string(),
                deterministic_order_sink: "orders.guardian".to_string(),
                source_id: ISSUE,
                faction: "guardian".to_string(),
                prompt: "Protect the lane and lower every decision to deterministic orders."
                    .to_string(),
                model_id: "model.local.bt".to_string(),
                grounding_refs: vec![
                    "ward.guardian".to_string(),
                    "world.issue788.lane".to_string(),
                ],
            },
            units: vec![
                Issue788Unit {
                    object_id: "npc.guardian".to_string(),
                    role: "agent_body".to_string(),
                    faction: "guardian".to_string(),
                    position: [0, 0, 0],
                    nav_cell: [0, 0],
                    alive: true,
                },
                Issue788Unit {
                    object_id: "player.one".to_string(),
                    role: "target".to_string(),
                    faction: "player".to_string(),
                    position: [2, 0, 0],
                    nav_cell: [2, 0],
                    alive: true,
                },
            ],
        }
    }
}

pub(crate) fn write_world_source(
    path: &Path,
    run_id: &str,
) -> Result<Issue788WorldReadback, Box<dyn std::error::Error>> {
    let world = Issue788World::new(run_id);
    let bytes = serde_json::to_vec_pretty(&world)?;
    fs::write(path, &bytes)?;
    let readback = read_world_source(path)?;
    if readback.world != world {
        return Err("issue788 world source readback mismatch".into());
    }
    Ok(readback)
}

fn read_world_source(path: &Path) -> Result<Issue788WorldReadback, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let world = serde_json::from_slice::<Issue788World>(&bytes)?;
    if world.schema != "litd.editor.issue788.world_source.v1" || world.issue != ISSUE {
        return Err("issue788 world source schema mismatch".into());
    }
    Ok(Issue788WorldReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        world,
    })
}
