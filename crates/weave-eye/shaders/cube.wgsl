struct Frame {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(7) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let model = mat4x4<f32>(v.model_0, v.model_1, v.model_2, v.model_3);
    var out: VertexOut;
    out.clip = frame.view_proj * model * vec4<f32>(v.position, 1.0);
    out.normal = normalize((model * vec4<f32>(v.normal, 0.0)).xyz);
    out.color = v.color;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(input.normal);
    let lambert = max(dot(n, -normalize(frame.light_dir.xyz)), 0.0);
    let direct = frame.light_color.rgb * frame.light_color.w * lambert;
    let ambient = vec3<f32>(frame.params.y);
    let lit = (direct + ambient) * input.color.rgb * frame.params.x;
    return vec4<f32>(lit, input.color.a);
}
