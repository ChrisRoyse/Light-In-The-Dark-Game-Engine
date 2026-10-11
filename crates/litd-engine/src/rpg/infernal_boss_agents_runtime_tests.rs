//! Unit coverage for the #1455 / #1874 Infernal boss-agents runtime tuning consumer.
//!
//! The full author -> freeze -> read-roster -> engage path (which needs the real measurement panel
//! and the CANONICAL vault) is proven end-to-end by
//! `examples/issue1455_infernal_boss_agents_runtime_slice_fsv.rs`. These unit tests pin the pure
//! contracts that need no Calyx: the engagement mechanic against a known frozen threshold, the
//! no-recompile boundary move, apex selection, and the loud-miss error when the frozen table has no
//! boss rows.

use super::{
    BossEngagement, INFERNAL_BOSS_ROLE_APEX_TYRANT, InfernalBossRuntimeRoster,
    InfernalBossRuntimeSpec, boss_engages,
};
use crate::calyx_content_runtime::{ENGINE_FROZEN_CONTENT_TABLE_SCHEMA, FrozenContentTable};

fn spec(agent: &str, role: &str, tier: i64, min_milli: i64) -> InfernalBossRuntimeSpec {
    InfernalBossRuntimeSpec {
        agent_id: agent.to_string(),
        boss_id: format!("boss.{agent}"),
        circle_id: "circle.test".to_string(),
        role: role.to_string(),
        lethality_tier: tier,
        engagement_strength_milli: min_milli,
    }
}

#[test]
fn engages_party_meeting_threshold() {
    let boss = spec("agent.test.warden", "circle_lord", 7, 650);
    assert_eq!(boss_engages(&boss, 650), BossEngagement::Engages);
    assert_eq!(boss_engages(&boss, 900), BossEngagement::Engages);
}

#[test]
fn withholds_party_below_threshold_with_frozen_value() {
    let boss = spec("agent.test.warden", "circle_lord", 7, 650);
    assert_eq!(
        boss_engages(&boss, 649),
        BossEngagement::Withholds {
            required_strength_milli: 650,
            actual_strength_milli: 649,
        }
    );
}

#[test]
fn raising_the_frozen_minimum_moves_the_engagement_boundary() {
    // The same 700-strength party that a min-700 boss engages is withheld once the authored minimum
    // is re-authored to 720 -- the boundary is data, not a compiled literal.
    assert_eq!(
        boss_engages(&spec("a", "circle_lord", 7, 700), 700),
        BossEngagement::Engages
    );
    assert_eq!(
        boss_engages(&spec("a", "circle_lord", 7, 720), 700),
        BossEngagement::Withholds {
            required_strength_milli: 720,
            actual_strength_milli: 700,
        }
    );
}

#[test]
fn apex_selects_the_authored_apex_tyrant() {
    let roster = InfernalBossRuntimeRoster {
        bosses: vec![
            spec("agent.lord.a", "circle_lord", 8, 750),
            spec("agent.apex", INFERNAL_BOSS_ROLE_APEX_TYRANT, 10, 950),
            spec("agent.lord.b", "circle_lord", 9, 850),
        ],
    };
    assert_eq!(roster.len(), 3);
    assert_eq!(
        roster.apex().map(|boss| boss.agent_id.as_str()),
        Some("agent.apex")
    );
    assert_eq!(
        roster.boss("agent.lord.b").map(|boss| boss.lethality_tier),
        Some(9)
    );
    assert!(roster.boss("agent.absent").is_none());
}

#[test]
fn apex_is_none_when_roster_has_only_circle_lords() {
    let roster = InfernalBossRuntimeRoster {
        bosses: vec![spec("agent.lord.a", "circle_lord", 8, 750)],
    };
    assert!(roster.apex().is_none());
}

#[test]
fn from_frozen_errors_loud_when_no_boss_rows() {
    // An empty frozen table has no boss rows: building the roster must fail loudly with the
    // structured field path, never hand back an empty roster a mechanic would silently trust.
    let empty = FrozenContentTable {
        schema: ENGINE_FROZEN_CONTENT_TABLE_SCHEMA.to_string(),
        rows: Vec::new(),
    };
    let error = InfernalBossRuntimeRoster::from_frozen(&empty).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("infernal_boss_agents_runtime.roster"),
        "expected a structured missing-roster error, got {message}"
    );
}
