@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

const SHARPEN_PEAK: f32 = -0.1538;

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(scene));
    let c = textureSampleLevel(scene, scene_sampler, input.uv, 0.0);
    let u = textureSampleLevel(scene, scene_sampler, input.uv + vec2<f32>(0.0, -texel.y), 0.0).rgb;
    let d = textureSampleLevel(scene, scene_sampler, input.uv + vec2<f32>(0.0, texel.y), 0.0).rgb;
    let l = textureSampleLevel(scene, scene_sampler, input.uv + vec2<f32>(-texel.x, 0.0), 0.0).rgb;
    let r = textureSampleLevel(scene, scene_sampler, input.uv + vec2<f32>(texel.x, 0.0), 0.0).rgb;
    let mn = min(min(min(u, d), min(l, r)), c.rgb);
    let mx = max(max(max(u, d), max(l, r)), c.rgb);
    let amp = sqrt(clamp(min(mn, vec3<f32>(1.0) - mx) / max(mx, vec3<f32>(1e-4)), vec3<f32>(0.0), vec3<f32>(1.0)));
    let w = amp * SHARPEN_PEAK;
    let sharpened = (c.rgb + (u + d + l + r) * w) / (1.0 + 4.0 * w);
    return vec4<f32>(clamp(sharpened, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
}
