//! Minimal editor capability-surface smoke test for the fast edit loop.
//!
//! This package deliberately depends on the production MCP crate as a normal
//! dependency, so its historical dev-dependencies are not compiled into the
//! routine target.

use litd_editor_mcp::capabilities::{
    build_capability_layer_report, default_capability_layer_manifest,
    validate_capability_layer_manifest,
};

#[test]
fn editor_capability_surface_manifest_is_a_valid_golden_path() {
    let manifest = default_capability_layer_manifest().expect("default editor capability manifest");
    assert!(
        !manifest.loopback_server.tools.is_empty(),
        "editor capability manifest has no tools"
    );
    let modules = litd_editor_mcp::modules::default_module_rebase_manifest()
        .expect("default editor module manifest");
    validate_capability_layer_manifest(&manifest, &modules)
        .expect("valid editor capability manifest");
    build_capability_layer_report(&manifest).expect("editor capability manifest report");
}
