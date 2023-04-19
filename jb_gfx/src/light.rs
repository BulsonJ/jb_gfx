use std::ops::Neg;
use cgmath::{Array, Deg, EuclideanSpace, InnerSpace, Matrix4, Point3, Vector3};

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
}

impl DirectionalLight {
    pub fn new(direction: Vector3<f32>, colour: Vector3<f32>, render_offset: f32,) -> Self {
        Self {
            direction: direction.normalize(),
            colour,
            znear: 0.1f32,
            zfar: 4000.0f32,
            render_offset,
        }
    }

    pub(crate) fn build_view_matrix(&self) -> Matrix4<f32> {
        let position = Point3::from_vec(self.direction.normalize().neg()) * self.render_offset;
        println!("{}", self.direction.normalize().y);
        Matrix4::look_to_rh(position, self.direction.normalize(), Vector3::unit_y())
    }

    pub(crate) fn build_projection_matrix(&self) -> Matrix4<f32> {
        cgmath::perspective(Deg(45.0f32), 1f32, self.znear, self.zfar)
    }
}
