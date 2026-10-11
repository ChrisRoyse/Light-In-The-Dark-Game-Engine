use super::*;

#[test]
fn default_registry_has_searchable_commands_and_unique_bindings() {
    let registry = default_editor_command_registry().unwrap();
    let bindings = default_editor_command_bindings().unwrap();

    assert!(registry.commands.len() >= 30);
    assert_eq!(registry.search("vfx")[0].command_id, COMMAND_MODE_VFX);
    assert!(bindings.binding_count() >= 12);
    bindings.validate(&registry).unwrap();
}

#[test]
fn conflicting_keybindings_fail_loud() {
    let registry = default_editor_command_registry().unwrap();
    let mut bindings = EditorCommandBindings::defaults(&registry);
    bindings
        .replace_binding(&registry, COMMAND_MODE_VFX, EditorKeyChord::new("F"))
        .expect_err("Frame Selected already owns F");
}

#[test]
fn dispatch_order_keeps_specific_shortcuts_before_broad_shortcuts() {
    let registry = default_editor_command_registry().unwrap();
    let bindings = EditorCommandBindings::defaults(&registry);
    let dispatch = bindings.dispatch_order();
    let save_one = dispatch
        .iter()
        .position(|binding| binding.command_id == COMMAND_SAVE_BOOKMARK_1)
        .unwrap();
    let restore_one = dispatch
        .iter()
        .position(|binding| binding.command_id == COMMAND_RESTORE_BOOKMARK_1)
        .unwrap();

    assert!(save_one < restore_one);
}

#[test]
fn unknown_commands_and_keys_are_rejected() {
    let registry = default_editor_command_registry().unwrap();
    let mut bindings = EditorCommandBindings::defaults(&registry);
    bindings
        .replace_binding(&registry, "editor.unknown", EditorKeyChord::new("F"))
        .expect_err("unknown command must fail");
    bindings
        .replace_binding(&registry, COMMAND_MODE_VFX, EditorKeyChord::new("Pause"))
        .expect_err("unsupported key must fail");
}
