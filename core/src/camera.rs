use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CameraTransform {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

impl CameraTransform {
    pub fn view_matrix(self) -> Mat4 {
        Mat4::look_to_rh(self.position, self.forward, self.up)
    }
}
