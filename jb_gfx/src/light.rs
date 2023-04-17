use cgmath::{Deg, InnerSpace, Matrix4, Point3, Vector3};

#[derive(Copy, Clone)]
pub struct Light {
    pub position: Point3<f32>,
    pub colour: Vector3<f32>,
}

impl Light {
    pub fn new(position: Point3<f32>, colour: Vector3<f32>) -> Self {
        Self { position, colour }
    }
}

#[derive(Copy, Clone)]
pub struct DirectionalLight {
    pub position: Point3<f32>,
    direction: Vector3<f32>,
    pub colour: Vector3<f32>,
    znear: f32,
    zfar: f32,
}

impl DirectionalLight {
    pub fn new(position: Point3<f32>, direction: Vector3<f32>, colour: Vector3<f32>) -> Self {
        Self {
            position,
            direction: direction.normalize(),
            colour,
            znear: 0.1f32,
            zfar: 15000.0f32,
        }
    }

    pub fn direction(&self) -> Vector3<f32> {
        self.direction
    }

    pub fn set_direction(&mut self, direction: Vector3<f32>) {
        self.direction = direction.normalize();
    }

    pub(crate) fn build_view_matrix(&self) -> Matrix4<f32> {
        Matrix4::look_to_rh(self.position, self.direction, Vector3::unit_y())
    }

    pub(crate) fn build_projection_matrix(&self) -> Matrix4<f32> {
        cgmath::perspective(Deg(45.0f32), 1f32, self.znear, self.zfar)
    }
}
