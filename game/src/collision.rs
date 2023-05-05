use cgmath::Vector3;

pub struct CollisionBox {
    pub position: Vector3<f32>,
    pub size: Vector3<f32>,
}

impl CollisionBox {
    pub fn check_collision(&self, other: &CollisionBox) -> bool {
        let collision_x_axis = self.position.x + self.size.x >= other.position.x
            && other.position.x + other.size.x >= self.position.x;
        let collision_y_axis = self.position.y + self.size.y >= other.position.y
            && other.position.y + other.size.y >= self.position.y;
        let collision_z_axis = self.position.z + self.size.z >= other.position.z
            && other.position.z + other.size.z >= self.position.z;

        collision_x_axis && collision_y_axis && collision_z_axis
    }
}
