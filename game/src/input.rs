use winit::event::{ElementState, KeyboardInput, VirtualKeyCode, WindowEvent};

pub struct Input {
    pub(crate) now_keys: [bool; 255],
    pub(crate) prev_keys: [bool; 255],
    pub(crate) mouse_pos: (f32, f32),
}

impl Input {
    pub fn is_just_pressed(&self, key: VirtualKeyCode) -> bool {
        self.now_keys[key as usize] && !self.prev_keys[key as usize]
    }

    pub fn is_held(&self, key: VirtualKeyCode) -> bool {
        self.now_keys[key as usize] && self.prev_keys[key as usize]
    }

    pub fn was_released(&self, key: VirtualKeyCode) -> bool {
        !self.now_keys[key as usize] && self.prev_keys[key as usize]
    }

    pub fn get_mouse_pos(&self) -> (f32, f32) {
        self.mouse_pos
    }
}

impl Default for Input {
    fn default() -> Self {
        Self {
            now_keys: [false; 255],
            prev_keys: [false; 255],
            mouse_pos: (0.0, 0.0),
        }
    }
}

// Winit integration
impl Input {
    pub(crate) fn update_input_from_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::CursorMoved {
                device_id: _device_id,
                position,
                modifiers,
            } => {
                self.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        state,
                        virtual_keycode: Some(keycode),
                        ..
                    },
                ..
            } => match state {
                ElementState::Pressed => {
                    self.now_keys[*keycode as usize] = true;
                }
                ElementState::Released => {
                    self.now_keys[*keycode as usize] = false;
                }
            },
            _ => {}
        }
    }
}
