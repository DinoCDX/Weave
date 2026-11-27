struct Frame {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
}

fn instance_matrix(c0: vec4<f32>, c1: vec4<f32>, c2: vec4<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(
        vec4<f32>(c0.xyz, 0.0),
        vec4<f32>(c1.xyz, 0.0),
        vec4<f32>(c2.xyz, 0.0),
        vec4<f32>(c0.w, c1.w, c2.w, 1.0),
    );
}

@vertex
fn vs_main(v: VertexIn) -> @builtin(position) vec4<f32> {
    let model = instance_matrix(v.model_0, v.model_1, v.model_2);
    let world = (model * vec4<f32>(v.position, 1.0)).xyz;
    return frame.view_proj * vec4<f32>(world, 1.0);
}
