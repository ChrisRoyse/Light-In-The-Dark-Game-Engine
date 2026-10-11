use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessOracleKind {
    DeterministicReplay,
    PresentationNonEmpty,
    NoErrorVerdict,
    CrashFree,
    SoftLockFree,
    PhysicsBounds,
    PerfBudget,
    EconomyRewardSanity,
    SoakStable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessOracleCheck {
    pub oracle: HarnessOracleKind,
    pub passed: bool,
    pub actual: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessOracleRunRecord {
    pub oracle_run_id: String,
    pub run_id: String,
    pub check_count: usize,
    pub passed_count: usize,
    pub failed_count: usize,
    pub verdict: String,
    pub checks: Vec<HarnessOracleCheck>,
}
