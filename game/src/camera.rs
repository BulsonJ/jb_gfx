use cgmath::{Deg, Matrix4, Point3, Vector3};

pub struct Camera {
    pub position: Point3<f32>,
    pub direction: Vector3<f32>,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl jb_gfx::CameraTrait for Camera {
    fn build_projection_matrix(&self) -> Matrix4<f32> {
        cgmath::perspective(Deg(self.fovy), self.aspect, self.znear, self.zfar)
    }

    fn build_view_matrix(&self) -> Matrix4<f32> {
        Matrix4::look_to_rh(self.position, self.direction, cgmath::Vector3::unit_y())
    }

    fn position(&self) -> Point3<f32> {
        self.position
    }
}
