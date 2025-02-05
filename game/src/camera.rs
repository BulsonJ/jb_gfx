use cgmath::{Deg, EuclideanSpace, Euler, Matrix4, Point3, Quaternion, Rotation3, Vector3};

pub struct Camera {
    pub position: Point3<f32>,
    pub rotation: Vector3<f32>,
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
        //Matrix4::look_to_rh(self.position, self.direction, cgmath::Vector3::unit_y())

        let translation = Matrix4::from_translation(self.position.to_vec());
        let rotation_euler = Euler {
            x: Deg(self.rotation.x),
            y: Deg(self.rotation.y),
            z: Deg(self.rotation.z),
        };
        let rotation = Matrix4::from(Quaternion::from(rotation_euler));

        translation * rotation
    }

    fn position(&self) -> Point3<f32> {
        self.position
    }
}
