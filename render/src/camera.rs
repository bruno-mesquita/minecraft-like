use glam::Mat4;
use voxel_core::{CameraTransform, RenderConfig};
use winit::dpi::PhysicalSize;

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub transform: CameraTransform,
    pub aspect_ratio: f32,
    pub fov_degrees: f32,
    pub near_plane: f32,
    pub far_plane: f32,
    pub view_distance: i32,
}

impl Camera {
    pub fn from_transform(transform: CameraTransform, config: &RenderConfig, size: PhysicalSize<u32>, view_distance: i32) -> Self {
        Self {
            transform,
            aspect_ratio: (size.width.max(1) as f32) / (size.height.max(1) as f32),
            fov_degrees: config.fov_degrees,
            near_plane: config.near_plane,
            far_plane: config.far_plane,
            view_distance,
        }
    }

    pub fn view_projection(self) -> Mat4 {
        let view = self.transform.view_matrix();
        let projection = Mat4::perspective_rh_gl(
            self.fov_degrees.to_radians(),
            self.aspect_ratio,
            self.near_plane,
            self.far_plane,
        );
        projection * view
    }
}