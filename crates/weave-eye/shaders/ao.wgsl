@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var depth_map: texture_depth_2d;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn view_at(pixel: vec2<i32>) -> vec3<f32> {
    let size = vec2<i32>(frame.viewport.xy);
    let clamped = clamp(pixel, vec2<i32>(0, 0), size - vec2<i32>(1, 1));
    let depth = textureLoad(depth_map, clamped, 0);
    let uv = (vec2<f32>(clamped) + 0.5) / frame.viewport.xy;
    let clip = frame.inv_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return clip.xyz / clip.w;
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if (frame.hd.w < 0.5) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    }
    let pixel = vec2<i32>(position.xy) * 2;
    let size = vec2<i32>(frame.viewport.xy);
    if (pixel.x >= size.x || pixel.y >= size.y || textureLoad(depth_map, pixel, 0) >= 1.0) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    }
    let p = view_at(pixel);
    let dx = view_at(pixel + vec2<i32>(1, 0)) - p;
    let dy = view_at(pixel + vec2<i32>(0, 1)) - p;
    var n = normalize(cross(dy, dx));
    if (n.z < 0.0) {
        n = -n;
    }

    let angle = hash(position.xy) * 6.2831853;
    let random = vec3<f32>(cos(angle), sin(angle), 0.0);
    let tangent = normalize(random - n * dot(random, n));
    let bitangent = cross(n, tangent);

    let radius = frame.hd.x;
    let bias = frame.hd.y;
    var occluded = 0.0;
    for (var k = 0u; k < 8u; k++) {
        let fk = f32(k);
        let z = 1.0 - (fk + 0.5) / 8.0;
        let r = sqrt(max(0.0, 1.0 - z * z));
        let phi = fk * 2.3999632;
        let local = vec3<f32>(r * cos(phi), r * sin(phi), z);
        let scale = mix(0.15, 1.0, (fk + 1.0) / 8.0 * (fk + 1.0) / 8.0);
        let sample_point = p + (tangent * local.x + bitangent * local.y + n * local.z) * radius * scale;

        let clip = frame.proj * vec4<f32>(sample_point, 1.0);
        let ndc = clip.xy / clip.w;
        let sample_pixel = vec2<i32>(
            i32((ndc.x * 0.5 + 0.5) * frame.viewport.x),
            i32((0.5 - ndc.y * 0.5) * frame.viewport.y)
        );
        let size = vec2<i32>(frame.viewport.xy);
        let clamped = clamp(sample_pixel, vec2<i32>(0, 0), size - vec2<i32>(1, 1));
        let scene_depth = textureLoad(depth_map, clamped, 0);
        let scene_z = -frame.proj[3][2] / (scene_depth + frame.proj[2][2]);
        let range = smoothstep(0.0, 1.0, radius / max(abs(p.z - scene_z), 0.0001));
        let hit = scene_depth < 1.0 && scene_z >= sample_point.z + bias;
        occluded = occluded + select(0.0, range, hit);
    }
    let visibility = 1.0 - clamp(occluded / 8.0 * frame.hd.z, 0.0, 1.0);
    return vec4<f32>(visibility, visibility, visibility, 1.0);
}
