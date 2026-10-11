use std::{env, fs, path::PathBuf};

use litd_engine::input::{
    ACTION_CAMERA_FORWARD, ACTION_CONFIRM, GamepadAxis, GamepadButton, InputBinding, InputBindings,
    InputState,
};
use winit::keyboard::KeyCode;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_INPUT_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1077-input-fsv.txt"));

    let mut bindings = InputBindings::editor_defaults();
    bindings.replace(
        "editor.quick.save",
        vec![
            InputBinding::Key(KeyCode::F5),
            InputBinding::Key(KeyCode::F5),
        ],
    );
    let mut input = InputState::new(bindings);

    input.set_key(KeyCode::KeyW, true);
    let keyboard_pressed = input.action_state(ACTION_CAMERA_FORWARD);
    input.finish_frame();
    input.set_key(KeyCode::KeyW, false);
    let keyboard_released = input.action_state(ACTION_CAMERA_FORWARD);
    input.set_gamepad_button(0, GamepadButton::South, true);
    input.set_gamepad_axis(0, GamepadAxis::LeftY, -1.0);
    let confirm = input.action_state(ACTION_CONFIRM);
    let axis_forward = input.action_state(ACTION_CAMERA_FORWARD);
    let stats = input.stats();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "action_count={}\n\
             binding_count={}\n\
             active_binding_count={}\n\
             events_seen={}\n\
             keyboard_forward_just_pressed={}\n\
             keyboard_forward_down_after_release={}\n\
             keyboard_forward_just_released={}\n\
             gamepad_confirm_down={}\n\
             gamepad_axis_forward_down={}\n\
             quick_save_binding_count={}\n",
            stats.action_count,
            stats.binding_count,
            stats.active_binding_count,
            stats.events_seen,
            keyboard_pressed.just_pressed,
            keyboard_released.down,
            keyboard_released.just_released,
            confirm.down,
            axis_forward.down,
            input.bindings().bindings_for("editor.quick.save").len(),
        ),
    )?;
    println!("wrote input FSV stats to {}", path.display());
    Ok(())
}
