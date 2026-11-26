use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let projection = directx::perspective(self.fov_y_radians, aspect, self.near, self.far);
        let view = view::look_at_mat4(self.eye, self.target, self.up);
        projection * view
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 1.5, 4.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_radians: 60f32.to_radians(),
            near: 0.1,
            far: 100.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalLight {
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: Vec3::new(-0.4, -1.0, -0.6).normalize(),
            color: Vec3::ONE,
            intensity: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameParams {
    pub exposure: f32,
    pub ambient: f32,
}

impl Default for FrameParams {
    fn default() -> Self {
        Self {
            exposure: 1.0,
            ambient: 0.08,
        }
    }
}
