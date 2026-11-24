#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn scaled(self, factor: u32) -> Self {
        Self {
            width: self.width.saturating_mul(factor),
            height: self.height.saturating_mul(factor),
        }
    }

    pub fn as_f32(self) -> [f32; 2] {
        [self.width as f32, self.height as f32]
    }
}
