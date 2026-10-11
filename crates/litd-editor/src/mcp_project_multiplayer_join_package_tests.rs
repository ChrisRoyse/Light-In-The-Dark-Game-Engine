use std::fs;

use serde_json::json;

use super::multiplayer_tests::{
    host_args, join_args, multiplayer_summary, prepare_hub_fixture, published_content_address,
};
use crate::mcp_project_tools::{
    MULTIPLAYER_SESSION_TOOL, call_mcp_project_tool, read_litdgame_install_manifest,
    read_multiplayer_session_state,
};

#[test]
fn issue805_guest_join_persists_exact_package_runtime_proof() {
    let paths = prepare_hub_fixture("issue805_join_proof");
    let content_address = published_content_address(&paths.catalog_path);
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_args(&paths, &content_address, "lobby.issue805", 4, "localhost"),
    )
    .unwrap();

    let install = read_litdgame_install_manifest(&paths.install_manifest_path).unwrap();
    let installed_path = &install.manifest.records[0].installed_package_path;
    let guest_path = paths.root.join("guest-package.litdgame.json");
    fs::copy(installed_path, &guest_path).unwrap();
    let mut args = join_args(&paths, "lobby.issue805", "player.guest");
    args["joining_package_path"] = json!(guest_path);
    call_mcp_project_tool(MULTIPLAYER_SESSION_TOOL, args).unwrap();

    let state = read_multiplayer_session_state(&paths.multiplayer_state_path)
        .unwrap()
        .state;
    let lobby = &state.lobbies[0];
    let join = &state.joins[0];
    assert_eq!(state.join_count, 1);
    assert!(lobby.member_ids.contains(&"player.guest".to_string()));
    assert_eq!(join.package_sha256, lobby.package_sha256);
    assert_eq!(join.package_bytes, lobby.package_bytes);
    assert!(join.runtime_lock_verified);
}

#[test]
fn issue805_guest_join_rejects_corrupt_or_missing_package_without_state_change() {
    let paths = prepare_hub_fixture("issue805_join_rejections");
    let content_address = published_content_address(&paths.catalog_path);
    call_mcp_project_tool(
        MULTIPLAYER_SESSION_TOOL,
        host_args(&paths, &content_address, "lobby.issue805", 4, "localhost"),
    )
    .unwrap();

    let install = read_litdgame_install_manifest(&paths.install_manifest_path).unwrap();
    let installed_path = &install.manifest.records[0].installed_package_path;
    let corrupt_path = paths.root.join("guest-corrupt.litdgame.json");
    let mut corrupt_bytes = fs::read(installed_path).unwrap();
    corrupt_bytes[0] ^= 1;
    fs::write(&corrupt_path, corrupt_bytes).unwrap();

    let before_corrupt = multiplayer_summary(&paths);
    let mut corrupt_args = join_args(&paths, "lobby.issue805", "player.corrupt");
    corrupt_args["joining_package_path"] = json!(corrupt_path);
    let corrupt_error = call_mcp_project_tool(MULTIPLAYER_SESSION_TOOL, corrupt_args).unwrap_err();
    let after_corrupt = multiplayer_summary(&paths);
    assert_eq!(before_corrupt, after_corrupt);
    assert!(
        corrupt_error
            .to_string()
            .contains("package bytes do not match expected proof")
    );

    let before_missing = multiplayer_summary(&paths);
    let mut missing_args = join_args(&paths, "lobby.issue805", "player.missing");
    missing_args
        .as_object_mut()
        .unwrap()
        .remove("joining_package_path");
    let missing_error = call_mcp_project_tool(MULTIPLAYER_SESSION_TOOL, missing_args).unwrap_err();
    let after_missing = multiplayer_summary(&paths);
    assert_eq!(before_missing, after_missing);
    assert!(
        missing_error
            .to_string()
            .contains("joining package path is required")
    );
}
