use cgmath::Vector3;

#[derive(Copy, Clone)]
pub struct Colour {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Colour {
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn red() -> Self {
        Self::new(1f32, 0f32, 0f32)
    }

    pub fn green() -> Self {
        Self::new(0f32, 1f32, 0f32)
    }

    pub fn blue() -> Self {
        Self::new(0f32, 0f32, 1f32)
    }

    pub fn black() -> Self {
        Self::new(0f32, 0f32, 0f32)
    }
}

impl From<Vector3<f32>> for Colour {
    fn from(value: Vector3<f32>) -> Self {
        Colour::new(value.x, value.y, value.z)
    }
}

impl From<Colour> for Vector3<f32> {
    fn from(value: Colour) -> Self {
        Vector3::new(value.r, value.g, value.b)
    }
}
