use cgmath::{Deg, Matrix4, Vector3};

pub struct Camera {
    pub position: Vector3<f32>,
    pub rotation: f32,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn build_view_matrix(&self) -> Matrix4<f32> {
        Matrix4::from_translation(self.position) * Matrix4::from_angle_y(Deg(self.rotation))
    }

    pub fn build_projection_matrix(&self) -> Matrix4<f32> {
        cgmath::perspective(Deg(self.fovy), self.aspect, self.znear, self.zfar)
    }
}
