struct Light {
    position_type: vec4<f32>,
    direction: vec4<f32>,
    color_intensity: vec4<f32>,
    params: vec4<f32>,
}

struct Frame {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    params: vec4<f32>,
    grid: vec4<f32>,
    viewport: vec4<f32>,
    shadow_view_proj: mat4x4<f32>,
    shadow_params: vec4<f32>,
    directional: array<Light, 4>,
    inv_proj: mat4x4<f32>,
    proj: mat4x4<f32>,
    hd: vec4<f32>,
    sky: vec4<f32>,
    ground: vec4<f32>,
}
