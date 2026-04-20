use glam::Vec2;
use voxel_sim::PlayerInput;
use winit::{
    dpi::PhysicalPosition,
    event::ElementState,
    keyboard::KeyCode,
    window::{CursorGrabMode, Window},
};

#[derive(Default)]
pub struct InputState {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub sprint: bool,
    pub jump_held: bool,
    pub jump_pressed: bool,
    pub mouse_delta: Vec2,
    pub cursor_captured: bool,
    pub last_cursor_position: Option<PhysicalPosition<f64>>,
    pub received_raw_mouse: bool,
}

impl InputState {
    pub fn handle_key(&mut self, code: KeyCode, state: ElementState, repeat: bool) {
        let pressed = state == ElementState::Pressed;

        match code {
            KeyCode::KeyW => self.forward = pressed,
            KeyCode::KeyS => self.back = pressed,
            KeyCode::KeyA => self.left = pressed,
            KeyCode::KeyD => self.right = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.sprint = pressed,
            KeyCode::Space => {
                if pressed && !repeat && !self.jump_held {
                    self.jump_pressed = true;
                }
                self.jump_held = pressed;
            }
            _ => {}
        }
    }

    pub fn to_player_input(&self, mouse_sensitivity: f32) -> PlayerInput {
        PlayerInput {
            move_forward: axis(self.forward, self.back),
            move_right: axis(self.right, self.left),
            look_delta: self.mouse_delta * mouse_sensitivity,
            jump_pressed: self.jump_pressed,
            sprint_held: self.sprint,
        }
    }

    pub fn end_frame(&mut self) {
        self.mouse_delta = Vec2::ZERO;
        self.jump_pressed = false;
        self.received_raw_mouse = false;
    }

    pub fn clear_focus_state(&mut self) {
        self.forward = false;
        self.back = false;
        self.left = false;
        self.right = false;
        self.sprint = false;
        self.jump_held = false;
        self.jump_pressed = false;
        self.mouse_delta = Vec2::ZERO;
        self.cursor_captured = false;
        self.last_cursor_position = None;
        self.received_raw_mouse = false;
    }

    pub fn handle_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        let previous = self.last_cursor_position.replace(position);
        if !self.cursor_captured || self.received_raw_mouse {
            return;
        }

        let Some(previous) = previous else {
            return;
        };

        self.mouse_delta += Vec2::new((position.x - previous.x) as f32, (position.y - previous.y) as f32);
    }
}

fn axis(positive: bool, negative: bool) -> f32 {
    match (positive, negative) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    }
}

pub fn capture_cursor(window: &Window, capture: bool) -> bool {
    if capture {
        window.set_cursor_visible(false);

        let grab_mode = window
            .set_cursor_grab(CursorGrabMode::Confined)
            .map(|_| CursorGrabMode::Confined)
            .or_else(|confined_error| {
                tracing::debug!(%confined_error, "confined cursor grab failed, trying locked");
                window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .map(|_| CursorGrabMode::Locked)
            });

        use winit::dpi::LogicalPosition;
        match grab_mode {
            Ok(mode) => {
                let size = window.inner_size();
                let center = LogicalPosition::new(
                    f64::from(size.width.max(1)) / 2.0,
                    f64::from(size.height.max(1)) / 2.0,
                );
                if let Err(error) = window.set_cursor_position(center) {
                    tracing::debug!(%error, "failed to place cursor at window center after capture");
                }
                tracing::debug!(?mode, "captured cursor");
                true
            }
            Err(error) => {
                tracing::warn!(%error, "failed to capture cursor");
                false
            }
        }
    } else {
        if let Err(error) = window.set_cursor_grab(CursorGrabMode::None) {
            tracing::warn!(%error, "failed to release cursor");
            return false;
        }
        window.set_cursor_visible(true);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_state_maps_keyboard_axes() {
        let mut input = InputState::default();
        input.handle_key(KeyCode::KeyW, ElementState::Pressed, false);
        input.handle_key(KeyCode::KeyD, ElementState::Pressed, false);
        input.handle_key(KeyCode::ShiftLeft, ElementState::Pressed, false);
        input.handle_key(KeyCode::Space, ElementState::Pressed, false);

        let player_input = input.to_player_input(1.0);
        assert_eq!(player_input.move_forward, 1.0);
        assert_eq!(player_input.move_right, 1.0);
        assert!(player_input.sprint_held);
        assert!(player_input.jump_pressed);
    }

    #[test]
    fn clear_focus_state_resets_pressed_input() {
        let mut input = InputState {
            forward: true,
            back: true,
            left: true,
            right: true,
            sprint: true,
            jump_held: true,
            jump_pressed: true,
            mouse_delta: Vec2::new(3.0, -2.0),
            cursor_captured: true,
            last_cursor_position: Some(PhysicalPosition::new(10.0, 12.0)),
            received_raw_mouse: true,
        };

        input.clear_focus_state();

        let player_input = input.to_player_input(1.0);
        assert_eq!(player_input.move_forward, 0.0);
        assert_eq!(player_input.move_right, 0.0);
        assert_eq!(player_input.look_delta, Vec2::ZERO);
        assert!(!player_input.jump_pressed);
        assert!(!player_input.sprint_held);
        assert!(!input.cursor_captured);
        assert!(input.last_cursor_position.is_none());
        assert!(!input.received_raw_mouse);
    }

    #[test]
    fn cursor_move_fallback_ignores_first_sample_after_capture() {
        let mut input = InputState {
            cursor_captured: true,
            ..InputState::default()
        };

        input.handle_cursor_moved(PhysicalPosition::new(100.0, 100.0));
        assert_eq!(input.mouse_delta, Vec2::ZERO);

        input.handle_cursor_moved(PhysicalPosition::new(112.0, 94.0));
        assert_eq!(input.mouse_delta, Vec2::new(12.0, -6.0));

        input.clear_focus_state();
        input.cursor_captured = true;
        input.handle_cursor_moved(PhysicalPosition::new(90.0, 90.0));
        assert_eq!(input.mouse_delta, Vec2::ZERO);
    }
}