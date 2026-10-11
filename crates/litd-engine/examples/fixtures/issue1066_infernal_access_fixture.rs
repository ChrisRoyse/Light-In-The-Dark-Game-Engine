// Single-source shim (#1502): authoritative fixture at
// crates/litd-engine/src/rpg/infernal_access_test_fixture.rs, `include!`d textually so the
// FSV examples and the unit tests share one file and can never drift.
include!("../../src/rpg/infernal_access_test_fixture.rs");
