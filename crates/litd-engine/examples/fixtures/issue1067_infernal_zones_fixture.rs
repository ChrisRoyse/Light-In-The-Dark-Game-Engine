// Single-source shim (#1502): the authoritative infernal-zones fixture lives at
// crates/litd-engine/src/rpg/infernal_zones_test_fixture.rs and is the SAME file the
// unit tests use. It is `include!`d here textually so the FSV examples and the unit
// tests can never drift. The fixture uses `litd_engine::rpg::...` paths, which resolve
// both inside the lib (via `extern crate self as litd_engine`) and here in the example.
include!("../../src/rpg/infernal_zones_test_fixture.rs");
