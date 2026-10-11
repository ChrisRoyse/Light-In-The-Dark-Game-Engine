//! Input foundation: stable action bindings over keyboard, mouse, and gamepad inputs.

use std::collections::{BTreeMap, HashMap, HashSet};

use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

pub const ACTION_CAMERA_FORWARD: &str = "editor.camera.forward";
pub const ACTION_CAMERA_BACK: &str = "editor.camera.back";
pub const ACTION_CAMERA_LEFT: &str = "editor.camera.left";
pub const ACTION_CAMERA_RIGHT: &str = "editor.camera.right";
/// Yaw left/right for RTS orbit. **Not** Q/E in play defaults (#1893: QWER = abilities).
pub const ACTION_CAMERA_YAW_LEFT: &str = "editor.camera.yaw_left";
pub const ACTION_CAMERA_YAW_RIGHT: &str = "editor.camera.yaw_right";
pub const ACTION_CAMERA_ZOOM_IN: &str = "editor.camera.zoom_in";
pub const ACTION_CAMERA_ZOOM_OUT: &str = "editor.camera.zoom_out";
/// MOBA-style ability bar slots 0..=3 on QWER (#1893/#224).
pub const ACTION_ABILITY_SLOT_0: &str = "game.ability.slot.0";
pub const ACTION_ABILITY_SLOT_1: &str = "game.ability.slot.1";
pub const ACTION_ABILITY_SLOT_2: &str = "game.ability.slot.2";
pub const ACTION_ABILITY_SLOT_3: &str = "game.ability.slot.3";
pub const ACTION_CONFIRM: &str = "editor.confirm";
pub const ACTION_ORBIT: &str = "editor.viewport.orbit";

/// Ordered ability-slot action ids for QWER (#1893).
pub const ABILITY_SLOT_ACTIONS: [&str; 4] = [
    ACTION_ABILITY_SLOT_0,
    ACTION_ABILITY_SLOT_1,
    ACTION_ABILITY_SLOT_2,
    ACTION_ABILITY_SLOT_3,
];

const GAMEPAD_AXIS_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    South,
    East,
    West,
    North,
    LeftShoulder,
    RightShoulder,
    Start,
    Select,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxisDirection {
    Negative,
    Positive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputBinding {
    Key(KeyCode),
    Mouse(MouseButton),
    GamepadButton(GamepadButton),
    GamepadAxis {
        axis: GamepadAxis,
        direction: AxisDirection,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputBindings {
    actions: BTreeMap<String, Vec<InputBinding>>,
}

impl InputBindings {
    pub fn editor_defaults() -> Self {
        let mut bindings = Self::default();
        // Editor fly/orbit keeps WASD; RTS play must use `rts_play_defaults` (#1892).
        bindings.bind(ACTION_CAMERA_FORWARD, InputBinding::Key(KeyCode::KeyW));
        bindings.bind(
            ACTION_CAMERA_FORWARD,
            InputBinding::GamepadAxis {
                axis: GamepadAxis::LeftY,
                direction: AxisDirection::Negative,
            },
        );
        bindings.bind(ACTION_CAMERA_BACK, InputBinding::Key(KeyCode::KeyS));
        bindings.bind(ACTION_CAMERA_LEFT, InputBinding::Key(KeyCode::KeyA));
        bindings.bind(ACTION_CAMERA_RIGHT, InputBinding::Key(KeyCode::KeyD));
        bindings.bind(ACTION_CAMERA_FORWARD, InputBinding::Key(KeyCode::ArrowUp));
        bindings.bind(ACTION_CAMERA_BACK, InputBinding::Key(KeyCode::ArrowDown));
        bindings.bind(ACTION_CAMERA_LEFT, InputBinding::Key(KeyCode::ArrowLeft));
        bindings.bind(ACTION_CAMERA_RIGHT, InputBinding::Key(KeyCode::ArrowRight));
        bindings.bind(ACTION_CAMERA_YAW_LEFT, InputBinding::Key(KeyCode::KeyQ));
        bindings.bind(ACTION_CAMERA_YAW_RIGHT, InputBinding::Key(KeyCode::KeyE));
        bindings.bind(ACTION_CAMERA_ZOOM_IN, InputBinding::Key(KeyCode::Equal));
        bindings.bind(ACTION_CAMERA_ZOOM_OUT, InputBinding::Key(KeyCode::Minus));
        bindings.bind(ACTION_CAMERA_ZOOM_IN, InputBinding::Key(KeyCode::PageUp));
        bindings.bind(ACTION_CAMERA_ZOOM_OUT, InputBinding::Key(KeyCode::PageDown));
        bindings.bind(ACTION_CONFIRM, InputBinding::Mouse(MouseButton::Left));
        bindings.bind(
            ACTION_CONFIRM,
            InputBinding::GamepadButton(GamepadButton::South),
        );
        bindings.bind(ACTION_ORBIT, InputBinding::Mouse(MouseButton::Right));
        bindings
    }

    /// #1892/#1893 WC3/MOBA-class RTS play defaults:
    /// - **no WASD pan**
    /// - **QWER = ability slots** (not camera)
    /// - Edge-scroll + MMB pan live in the host; yaw optional on `[`/`]`
    pub fn rts_play_defaults() -> Self {
        let mut bindings = Self::default();
        // Arrow keys optional secondary pan (not WASD).
        bindings.bind(ACTION_CAMERA_FORWARD, InputBinding::Key(KeyCode::ArrowUp));
        bindings.bind(ACTION_CAMERA_BACK, InputBinding::Key(KeyCode::ArrowDown));
        bindings.bind(ACTION_CAMERA_LEFT, InputBinding::Key(KeyCode::ArrowLeft));
        bindings.bind(ACTION_CAMERA_RIGHT, InputBinding::Key(KeyCode::ArrowRight));
        // Camera yaw: brackets only — never Q/E (#1893).
        bindings.bind(
            ACTION_CAMERA_YAW_LEFT,
            InputBinding::Key(KeyCode::BracketLeft),
        );
        bindings.bind(
            ACTION_CAMERA_YAW_RIGHT,
            InputBinding::Key(KeyCode::BracketRight),
        );
        bindings.bind(ACTION_CAMERA_ZOOM_IN, InputBinding::Key(KeyCode::Equal));
        bindings.bind(ACTION_CAMERA_ZOOM_OUT, InputBinding::Key(KeyCode::Minus));
        bindings.bind(ACTION_CAMERA_ZOOM_IN, InputBinding::Key(KeyCode::PageUp));
        bindings.bind(ACTION_CAMERA_ZOOM_OUT, InputBinding::Key(KeyCode::PageDown));
        // MOBA ability bar: Q W E R -> slots 0..=3.
        bindings.bind(ACTION_ABILITY_SLOT_0, InputBinding::Key(KeyCode::KeyQ));
        bindings.bind(ACTION_ABILITY_SLOT_1, InputBinding::Key(KeyCode::KeyW));
        bindings.bind(ACTION_ABILITY_SLOT_2, InputBinding::Key(KeyCode::KeyE));
        bindings.bind(ACTION_ABILITY_SLOT_3, InputBinding::Key(KeyCode::KeyR));
        bindings.bind(ACTION_CONFIRM, InputBinding::Mouse(MouseButton::Left));
        bindings.bind(
            ACTION_CONFIRM,
            InputBinding::GamepadButton(GamepadButton::South),
        );
        // Middle-mouse is pan drag (host); right-click reserved for orders later.
        bindings.bind(ACTION_ORBIT, InputBinding::Mouse(MouseButton::Middle));
        bindings
    }

    pub fn bind(&mut self, action: impl Into<String>, binding: InputBinding) {
        let bindings = self.actions.entry(action.into()).or_default();
        if !bindings.contains(&binding) {
            bindings.push(binding);
        }
    }

    pub fn replace(&mut self, action: impl Into<String>, bindings: Vec<InputBinding>) {
        let mut deduped = Vec::new();
        for binding in bindings {
            if !deduped.contains(&binding) {
                deduped.push(binding);
            }
        }
        self.actions.insert(action.into(), deduped);
    }

    pub fn clear(&mut self, action: &str) {
        self.actions.remove(action);
    }

    pub fn bindings_for(&self, action: &str) -> &[InputBinding] {
        self.actions
            .get(action)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    pub fn binding_count(&self) -> usize {
        self.actions.values().map(Vec::len).sum()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionState {
    pub down: bool,
    pub just_pressed: bool,
    pub just_released: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputStats {
    pub frame_index: u64,
    pub action_count: usize,
    pub binding_count: usize,
    pub active_binding_count: usize,
    pub events_seen: u64,
    pub cursor_position: Option<(f64, f64)>,
}

#[derive(Debug, Clone)]
pub struct InputState {
    bindings: InputBindings,
    active: HashSet<InputBinding>,
    previous: HashSet<InputBinding>,
    gamepad_axes: HashMap<(u32, GamepadAxis), f32>,
    cursor_position: Option<(f64, f64)>,
    /// Accumulated mouse-wheel Y since last `take_scroll_y` (positive = scroll up / zoom in).
    scroll_y: f32,
    events_seen: u64,
    frame_index: u64,
}

impl InputState {
    pub fn new(bindings: InputBindings) -> Self {
        Self {
            bindings,
            active: HashSet::new(),
            previous: HashSet::new(),
            gamepad_axes: HashMap::new(),
            cursor_position: None,
            scroll_y: 0.0,
            events_seen: 0,
            frame_index: 0,
        }
    }

    pub fn with_editor_defaults() -> Self {
        Self::new(InputBindings::editor_defaults())
    }

    /// RTS package play: edge-scroll / MMB / wheel defaults (#1892).
    pub fn with_rts_play_defaults() -> Self {
        Self::new(InputBindings::rts_play_defaults())
    }

    pub fn bindings(&self) -> &InputBindings {
        &self.bindings
    }

    pub fn bindings_mut(&mut self) -> &mut InputBindings {
        &mut self.bindings
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event, .. } => match event.physical_key {
                PhysicalKey::Code(code) => {
                    self.set_key(code, event.state == ElementState::Pressed);
                    true
                }
                PhysicalKey::Unidentified(_) => false,
            },
            WindowEvent::MouseInput { state, button, .. } => {
                self.set_mouse_button(*button, *state == ElementState::Pressed);
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = Some((position.x, position.y));
                self.events_seen += 1;
                true
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, lines) => *lines,
                    MouseScrollDelta::PixelDelta(pixels) => (pixels.y as f32) / 40.0,
                };
                if y.is_finite() && y != 0.0 {
                    self.scroll_y += y;
                    self.events_seen += 1;
                }
                true
            }
            _ => false,
        }
    }

    /// Drain accumulated mouse-wheel Y (positive = wheel-up / zoom-in intent).
    pub fn take_scroll_y(&mut self) -> f32 {
        let y = self.scroll_y;
        self.scroll_y = 0.0;
        y
    }

    pub fn set_key(&mut self, key: KeyCode, pressed: bool) {
        self.set_binding_state(InputBinding::Key(key), pressed);
    }

    pub fn set_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        self.set_binding_state(InputBinding::Mouse(button), pressed);
    }

    pub fn set_gamepad_button(&mut self, _gamepad_id: u32, button: GamepadButton, pressed: bool) {
        self.set_binding_state(InputBinding::GamepadButton(button), pressed);
    }

    pub fn set_gamepad_axis(&mut self, gamepad_id: u32, axis: GamepadAxis, value: f32) {
        let value = value.clamp(-1.0, 1.0);
        self.gamepad_axes.insert((gamepad_id, axis), value);
        self.set_binding_state(
            InputBinding::GamepadAxis {
                axis,
                direction: AxisDirection::Negative,
            },
            value <= -GAMEPAD_AXIS_THRESHOLD,
        );
        self.set_binding_state(
            InputBinding::GamepadAxis {
                axis,
                direction: AxisDirection::Positive,
            },
            value >= GAMEPAD_AXIS_THRESHOLD,
        );
    }

    pub fn action_state(&self, action: &str) -> ActionState {
        let down = self
            .bindings
            .bindings_for(action)
            .iter()
            .any(|binding| self.active.contains(binding));
        let was_down = self
            .bindings
            .bindings_for(action)
            .iter()
            .any(|binding| self.previous.contains(binding));
        ActionState {
            down,
            just_pressed: down && !was_down,
            just_released: !down && was_down,
        }
    }

    pub fn finish_frame(&mut self) {
        self.previous.clone_from(&self.active);
        self.frame_index += 1;
    }

    pub fn stats(&self) -> InputStats {
        InputStats {
            frame_index: self.frame_index,
            action_count: self.bindings.action_count(),
            binding_count: self.bindings.binding_count(),
            active_binding_count: self.active.len(),
            events_seen: self.events_seen,
            cursor_position: self.cursor_position,
        }
    }

    fn set_binding_state(&mut self, binding: InputBinding, pressed: bool) {
        if pressed {
            self.active.insert(binding);
        } else {
            self.active.remove(&binding);
        }
        self.events_seen += 1;
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::with_editor_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_binding_tracks_pressed_and_released_edges() {
        let mut input = InputState::with_editor_defaults();
        input.set_key(KeyCode::KeyW, true);
        let pressed = input.action_state(ACTION_CAMERA_FORWARD);
        assert!(pressed.down);
        assert!(pressed.just_pressed);

        input.finish_frame();
        input.set_key(KeyCode::KeyW, false);
        let released = input.action_state(ACTION_CAMERA_FORWARD);
        assert!(!released.down);
        assert!(released.just_released);
    }

    #[test]
    fn actions_can_be_remapped_without_duplicate_bindings() {
        let mut bindings = InputBindings::default();
        bindings.bind("jump", InputBinding::Key(KeyCode::Space));
        bindings.bind("jump", InputBinding::Key(KeyCode::Space));
        bindings.replace(
            "jump",
            vec![
                InputBinding::Key(KeyCode::ArrowUp),
                InputBinding::Key(KeyCode::ArrowUp),
            ],
        );
        assert_eq!(bindings.binding_count(), 1);
        assert_eq!(
            bindings.bindings_for("jump")[0],
            InputBinding::Key(KeyCode::ArrowUp)
        );
    }

    #[test]
    fn gamepad_button_and_axis_drive_actions() {
        let mut input = InputState::with_editor_defaults();
        input.set_gamepad_button(0, GamepadButton::South, true);
        input.set_gamepad_axis(0, GamepadAxis::LeftY, -1.0);

        assert!(input.action_state(ACTION_CONFIRM).down);
        assert!(input.action_state(ACTION_CAMERA_FORWARD).down);
    }

    #[test]
    fn rts_play_defaults_map_qwer_to_ability_slots_not_camera() {
        let bindings = InputBindings::rts_play_defaults();
        assert_eq!(
            bindings.bindings_for(ACTION_ABILITY_SLOT_0),
            &[InputBinding::Key(KeyCode::KeyQ)]
        );
        assert_eq!(
            bindings.bindings_for(ACTION_ABILITY_SLOT_1),
            &[InputBinding::Key(KeyCode::KeyW)]
        );
        assert_eq!(
            bindings.bindings_for(ACTION_ABILITY_SLOT_2),
            &[InputBinding::Key(KeyCode::KeyE)]
        );
        assert_eq!(
            bindings.bindings_for(ACTION_ABILITY_SLOT_3),
            &[InputBinding::Key(KeyCode::KeyR)]
        );
        // Q/E must not yaw the camera in RTS play (#1893).
        assert!(
            !bindings
                .bindings_for(ACTION_CAMERA_YAW_LEFT)
                .contains(&InputBinding::Key(KeyCode::KeyQ))
        );
        assert!(
            !bindings
                .bindings_for(ACTION_CAMERA_YAW_RIGHT)
                .contains(&InputBinding::Key(KeyCode::KeyE))
        );
        // WASD must not pan.
        assert!(
            !bindings
                .bindings_for(ACTION_CAMERA_FORWARD)
                .contains(&InputBinding::Key(KeyCode::KeyW))
        );
    }
}
