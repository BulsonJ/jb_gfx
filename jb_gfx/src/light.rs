use std::ops::Neg;

use cgmath::{
    abs_diff_eq, AbsDiffEq, EuclideanSpace, InnerSpace, Matrix4, Point3, RelativeEq, Vector3,
};

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
    pub direction: Vector3<f32>,
    pub colour: Vector3<f32>,
    znear: f32,
    zfar: f32,
    render_offset: f32,
    ortho_size: f32,
}

impl DirectionalLight {
    pub fn new(direction: Vector3<f32>, colour: Vector3<f32>, render_offset: f32) -> Self {
        Self {
            direction: direction.normalize(),
            colour,
            znear: -4000.0f32,
            zfar: 4000.0f32,
            render_offset,
            ortho_size: 300f32,
        }
    }

    pub(crate) fn build_view_matrix(&self) -> Matrix4<f32> {
        let position = Point3::from_vec(self.direction.normalize().neg()) * self.render_offset;
        // Temp workaround for look at returning NAN when direction aligned with UP
        if abs_diff_eq!(self.direction.normalize(), Vector3::unit_y())
            || abs_diff_eq!(-self.direction.normalize(), Vector3::unit_y())
        {
            Matrix4::look_to_rh(position, self.direction, Vector3::unit_z())
        } else {
            Matrix4::look_to_rh(position, self.direction, Vector3::unit_y())
        }
    }

    pub(crate) fn build_projection_matrix(&self) -> Matrix4<f32> {
        cgmath::ortho(
            -self.ortho_size,
            self.ortho_size,
            -self.ortho_size,
            self.ortho_size,
            self.znear,
            self.zfar,
        )
    }
}
