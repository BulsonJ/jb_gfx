use winit::event::VirtualKeyCode;

pub struct Input {
    pub(crate) now_keys: [bool; 255],
    pub(crate) prev_keys: [bool; 255],
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
}
