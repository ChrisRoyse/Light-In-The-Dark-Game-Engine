use super::*;

pub fn default_editor_command_registry() -> Result<EditorCommandRegistry, EngineError> {
    let mut commands = vec![
        entry(
            COMMAND_OPEN_PALETTE,
            "Open Command Palette",
            &["palette", "search", "commands"],
            false,
            Some(EditorKeyChord::command("K")),
        ),
        entry(
            COMMAND_CREATE_WORLD,
            "Create New World",
            &["world", "new", "create", "map", "terrain"],
            true,
            None,
        ),
        entry(
            COMMAND_VALIDATE_WORLD,
            "Validate World",
            &["validate", "analyze", "preflight", "errors"],
            true,
            None,
        ),
        entry(
            COMMAND_MODERATION_SUBMIT_REPORT,
            "Submit Moderation Report",
            &["moderation", "report", "safety", "server"],
            true,
            None,
        ),
        entry(
            COMMAND_TOGGLE_PLAYTEST,
            "Toggle Playtest",
            &["play", "test", "run", "stop"],
            true,
            Some(EditorKeyChord::command("Enter")),
        ),
        entry(
            COMMAND_FRAME_SELECTED,
            "Frame Selected",
            &["camera", "view", "selection"],
            true,
            Some(EditorKeyChord::new("F")),
        ),
        entry(
            COMMAND_SNAP_GAME_CAMERA,
            "Snap to Game Camera",
            &["camera", "play", "runtime"],
            true,
            Some(EditorKeyChord::command("G")),
        ),
        entry(
            COMMAND_INSPECTOR_POP_OUT,
            "Pop Out Inspector",
            &["panel", "dock", "inspector"],
            true,
            None,
        ),
        entry(
            COMMAND_INSPECTOR_REATTACH,
            "Reattach Inspector",
            &["panel", "dock", "inspector"],
            true,
            None,
        ),
    ];
    commands.extend(mode_entries());
    commands.extend(bookmark_entries());
    commands.extend(world_layer_entries());
    let registry = EditorCommandRegistry {
        schema_version: 1,
        parent_issue: 1217,
        shared_registry_id: "litd.editor.command_registry.v1".to_string(),
        commands,
    };
    registry.validate()?;
    EditorCommandBindings::defaults(&registry).validate(&registry)?;
    Ok(registry)
}

pub fn default_editor_command_bindings() -> Result<EditorCommandBindings, EngineError> {
    let registry = default_editor_command_registry()?;
    Ok(EditorCommandBindings::defaults(&registry))
}

fn mode_entries() -> Vec<EditorCommandEntry> {
    [
        (COMMAND_MODE_TERRAIN, "Switch to Terrain Mode", "1"),
        (COMMAND_MODE_OBJECTS, "Switch to Objects Mode", "2"),
        (COMMAND_MODE_TRIGGERS, "Switch to Triggers Mode", "3"),
        (COMMAND_MODE_CINEMATICS, "Switch to Cinematics Mode", "4"),
        (COMMAND_MODE_VFX, "Switch to VFX Mode", "5"),
        (COMMAND_MODE_AI_COAUTHOR, "Switch to AI Co-Author Mode", "6"),
    ]
    .into_iter()
    .map(|(id, title, key)| {
        entry(
            id,
            title,
            &["mode", "tab"],
            true,
            Some(EditorKeyChord::alt(key)),
        )
    })
    .collect()
}

fn bookmark_entries() -> Vec<EditorCommandEntry> {
    [
        (COMMAND_SAVE_BOOKMARK_1, "Save Camera Bookmark 1", "1", true),
        (COMMAND_SAVE_BOOKMARK_2, "Save Camera Bookmark 2", "2", true),
        (COMMAND_SAVE_BOOKMARK_3, "Save Camera Bookmark 3", "3", true),
        (
            COMMAND_RESTORE_BOOKMARK_1,
            "Restore Camera Bookmark 1",
            "1",
            false,
        ),
        (
            COMMAND_RESTORE_BOOKMARK_2,
            "Restore Camera Bookmark 2",
            "2",
            false,
        ),
        (
            COMMAND_RESTORE_BOOKMARK_3,
            "Restore Camera Bookmark 3",
            "3",
            false,
        ),
    ]
    .into_iter()
    .map(|(id, title, key, save)| {
        let chord = if save {
            EditorKeyChord::command_shift(key)
        } else {
            EditorKeyChord::command(key)
        };
        entry(
            id,
            title,
            &["camera", "bookmark", "view"],
            true,
            Some(chord),
        )
    })
    .collect()
}

fn world_layer_entries() -> Vec<EditorCommandEntry> {
    [
        ("terrain", "Terrain"),
        ("foliage", "Foliage"),
        ("doodads", "Doodads"),
        ("units", "Units"),
        ("water", "Water"),
        ("blight", "Blight"),
        ("shadows", "Shadows"),
        ("lighting", "Lighting"),
        ("weather", "Weather"),
        ("sky", "Sky"),
        ("fog_effects", "Fog Effects"),
        ("grid", "Grid"),
        ("camera_bounds", "Camera Bounds"),
        ("pathing_ground", "Pathing Ground"),
        ("pathing_naval", "Pathing Naval"),
        ("regions", "Regions"),
        ("camera_objects", "Camera Objects"),
        ("info_display", "Info Display"),
        ("letterbox_mode", "Letterbox Mode"),
        ("lock_to_game_camera", "Lock to Game Camera"),
    ]
    .into_iter()
    .map(|(id, label)| {
        entry(
            &format!("editor.view.toggle_{id}"),
            &format!("Toggle {label} Layer"),
            &["view", "layer", "visibility"],
            true,
            None,
        )
    })
    .collect()
}

fn entry(
    id: &str,
    title: &str,
    terms: &[&str],
    mutates_shell_state: bool,
    default_keybinding: Option<EditorKeyChord>,
) -> EditorCommandEntry {
    EditorCommandEntry {
        command_id: id.to_string(),
        title: title.to_string(),
        search_terms: terms.iter().map(|term| (*term).to_string()).collect(),
        operation_id: id.to_string(),
        gui_route: format!("egui://editor.commands/{id}"),
        mcp_tool_name: format!("litd.editor.commands.{}", id.replace('.', "_")),
        mutates_shell_state,
        default_keybinding,
    }
}
